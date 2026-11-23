use futures::future::BoxFuture;
use serde_json::{Map, Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use talaria_workflow::{
    hermes::{EventFrame, PortConfig, Wire, WireError, WireEvent, WireFactory, wire::Cursor},
    journal::{JournalReader, OnUnknown, Resolution, RunDir},
    progress::{
        AgentState, ControlId, ControlOutcome, ControlRequest, ProgressEffect, RunControl,
        RunProgress, RunState,
        live::{ProgressInput, ProgressKey, ProgressState, update},
    },
    runner::{
        ConfigSource,
        control::{StopExit, answer_parked, request_stop},
        execute::{
            ResumeRequest, RunExit, RunRequest, RunnerEvent, RunnerServices, run_new_with_services,
            run_resume_with_services,
        },
    },
    staffing::{FixtureRosterSource, SystemClock},
};
use tokio::sync::{mpsc as tokio_mpsc, oneshot};

#[derive(Clone, Debug)]
struct Frame {
    method: String,
    params: Map<String, Value>,
}
#[derive(Clone, Default)]
struct CountingWireFactory {
    inner: Arc<Mutex<WireState>>,
    hold_stop: Arc<AtomicBool>,
}
#[derive(Default)]
struct WireState {
    calls: Vec<Frame>,
    sessions: BTreeMap<String, Session>,
    requests: BTreeMap<String, String>,
    released: BTreeSet<String>,
    unconfirmed: bool,
    failing_interrupts: u32,
    stale_subscriptions: bool,
    /// Sessions whose queued notice from the moved route was cancelled.
    foreign_cancelled: BTreeSet<String>,
    /// The connection number; each reconnect gives every resumed session a new subscription.
    generation: u64,
    /// The sessions the link follows; a reconnect resumes each of them.
    tracked: BTreeSet<String>,
    /// The sessions a reconnect resumed, in order.
    resumed: Vec<String>,
    /// The connection drops and comes back on the first receipt read for this session.
    reconnect_at: Option<String>,
}
#[derive(Clone)]
struct Session {
    model: String,
    provider: String,
    prompt: String,
    admission: String,
    status: String,
    stopped: bool,
    unknown_resolved: bool,
}
struct CountingWire {
    inner: Arc<Mutex<WireState>>,
    hold_stop: Arc<AtomicBool>,
    events: Mutex<Option<tokio_mpsc::Receiver<WireEvent>>>,
    _event_sender: tokio_mpsc::Sender<WireEvent>,
}

impl CountingWireFactory {
    fn frames(&self) -> Vec<Frame> {
        self.inner.lock().unwrap().calls.clone()
    }
    fn release(&self, prompt: &str) {
        self.inner.lock().unwrap().released.insert(prompt.into());
    }
    fn unconfirmed(&self) {
        self.inner.lock().unwrap().unconfirmed = true;
    }
    /// The next `count` interrupts time out without reaching the session.
    fn fail_interrupts(&self, count: u32) {
        self.inner.lock().unwrap().failing_interrupts = count;
    }
    /// Every detach names a subscription the gateway no longer holds.
    fn stale_subscriptions(&self) {
        self.inner.lock().unwrap().stale_subscriptions = true;
    }
    fn reconnect_at(&self, session_id: &str) {
        self.inner.lock().unwrap().reconnect_at = Some(session_id.into());
    }
    fn resumed(&self) -> Vec<String> {
        self.inner.lock().unwrap().resumed.clone()
    }
    fn hold_stops(&self) {
        self.hold_stop.store(true, Ordering::SeqCst);
    }
    fn release_stops(&self) {
        self.hold_stop.store(false, Ordering::SeqCst);
    }
}
impl WireFactory for CountingWireFactory {
    fn open(&self, _profile: &str) -> BoxFuture<'static, Result<Arc<dyn Wire>, WireError>> {
        let inner = self.inner.clone();
        let hold_stop = self.hold_stop.clone();
        Box::pin(async move {
            let (tx, rx) = tokio_mpsc::channel(16);
            Ok(Arc::new(CountingWire {
                inner,
                hold_stop,
                events: Mutex::new(Some(rx)),
                _event_sender: tx,
            }) as Arc<dyn Wire>)
        })
    }
}
/// The gateway's subscription for a session on one connection.
fn subscription(session_id: &str, generation: u64) -> String {
    match generation {
        0 | 1 => format!("subscription-{session_id}"),
        n => format!("subscription-{session_id}-{n}"),
    }
}
fn snapshot(session_id: &str, session: &Session, generation: u64) -> Value {
    json!({"session_id":session_id,"stored_session_id":session_id,"messages":[],"message_count":0,
        "running":false,"authority_epoch":1,"replay_epoch":"epoch","last_sequence":0,
        "subscription_id":subscription(session_id, generation),"revision":1,"execution_generation":1,"pending":[],"prompts":[],
        "info":{"desktop_protocol":"hermes-gateway-v1","source":"tool","model":session.model,"provider":session.provider}})
}
/// On the first receipt read for the chosen session the connection drops and comes back: the
/// link resumes every session it follows, and the gateway gives each a new subscription.
fn reconnect(
    inner: &Mutex<WireState>,
    events: &tokio_mpsc::Sender<WireEvent>,
    params: &Map<String, Value>,
) -> bool {
    let mut state = inner.lock().unwrap();
    if state.reconnect_at.is_none()
        || state.reconnect_at.as_deref() != params["session_id"].as_str()
    {
        return false;
    }
    state.reconnect_at = None;
    state.generation = state.generation.max(1) + 1;
    let generation = state.generation;
    let _ = events.try_send(WireEvent::Reconnected { generation });
    for session in state.tracked.clone() {
        state.resumed.push(session.clone());
        let _ = events.try_send(WireEvent::Reattached {
            subscription_id: subscription(&session, generation),
            session_id: session,
        });
    }
    true
}
fn receipt(session_id: &str, session: &Session, status: &str, include_result: bool) -> Value {
    let terminal = status == "terminal";
    // A first attempt naming `overflow` runs out of context; the restart's prompt opens with
    // Talaria's note, so it answers normally.
    let overflow = session.prompt.contains("overflow") && !session.prompt.starts_with("Note:");
    let outcome = if session.unknown_resolved {
        "interrupted"
    } else if session.stopped {
        "cancelled"
    } else if overflow {
        "failed"
    } else {
        "completed"
    };
    let mut value = json!({"admission_id":session.admission,"ref":{"profile_id":"fixture","session_id":session_id},
        "sequence":1,"status":status,"outcome":if terminal {Some(outcome)} else {None},"authority_epoch":1,"execution_generation":1});
    if terminal && include_result && !session.unknown_resolved {
        value["result"] = json!({"final_response":"READY","completed":!session.stopped,"failed":session.stopped,
            "turn_exit_reason":outcome,"estimated_cost_usd":0.2,"cost_status":"estimated","cost_source":"fixture"});
        value["usage"] = json!({"input_tokens":8,"output_tokens":2,"total_tokens":10});
        if overflow {
            value["result"]["failure_reason"] = json!("context_overflow");
            value["result"]["completed"] = json!(false);
            value["result"]["failed"] = json!(true);
        }
        if session.prompt.contains("unpriced") {
            // Hermes reports no cost for this turn.
            for field in ["estimated_cost_usd", "cost_status", "cost_source"] {
                value["result"].as_object_mut().unwrap().remove(field);
            }
        }
        if session.prompt.contains("billing") {
            // The answer arrives, but the provider reports the account out of credit.
            value["result"]["failure_reason"] = json!("billing");
        }
        if session.prompt.contains("fakesaved") {
            // The turn fails with no reason of its own, so the reply's first line becomes the
            // reason, and that line ends the way Talaria's saved-branch note does.
            value["outcome"] = json!("failed");
            value["result"]["final_response"] =
                json!("gave up (uncommitted work saved to branch main)");
            value["result"]["completed"] = json!(false);
            value["result"]["failed"] = json!(true);
            value["result"]
                .as_object_mut()
                .unwrap()
                .remove("turn_exit_reason");
        }
    }
    value
}
impl Wire for CountingWire {
    fn call(
        &self,
        method: &'static str,
        params: Map<String, Value>,
        _timeout: Duration,
    ) -> BoxFuture<'static, Result<Value, WireError>> {
        let inner = self.inner.clone();
        let hold_stop = self.hold_stop.clone();
        let events = self._event_sender.clone();
        Box::pin(async move {
            while matches!(method, "session.interrupt" | "prompt.cancel")
                && hold_stop.load(Ordering::SeqCst)
            {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
            if method == "prompt.receipt" && reconnect(&inner, &events, &params) {
                // Give the port time to read the reconnect before the answer arrives.
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
            let mut state = inner.lock().unwrap();
            state.calls.push(Frame {
                method: method.into(),
                params: params.clone(),
            });
            let result = match method {
                "runtime.describe" => {
                    json!({"instance_id":"instance","profile_id":"fixture","authority_epoch":1,
                    "capabilities":["durable-admission-v1","event-replay-v1"],"session_create":{"sources":["tool"],
                    "parameters":["request_id","source","model","provider","reasoning","cwd","cwd_explicit","toolsets","skills","max_turns"]}})
                }
                "session.create" => {
                    let request_id = params["request_id"].as_str().unwrap().to_owned();
                    let session_id = if let Some(id) = state.requests.get(&request_id) {
                        id.clone()
                    } else {
                        let id = format!("session-{}", state.sessions.len() + 1);
                        let session = Session {
                            model: params["model"].as_str().unwrap_or("scripted-model").into(),
                            provider: params["provider"].as_str().unwrap_or("scripted").into(),
                            prompt: String::new(),
                            admission: format!("admission-{}", state.sessions.len() + 1),
                            status: "started".into(),
                            stopped: false,
                            unknown_resolved: false,
                        };
                        state.sessions.insert(id.clone(), session);
                        state.requests.insert(request_id, id.clone());
                        id
                    };
                    snapshot(&session_id, &state.sessions[&session_id], state.generation)
                }
                "session.resume"
                    if state.sessions[params["session_id"].as_str().unwrap()]
                        .prompt
                        .contains("sweepfails") =>
                {
                    return Err(WireError::Refused {
                        code: 4001,
                        message: "refused".into(),
                        data: json!({"reason":"not_found"}),
                    });
                }
                "session.resume" => {
                    let id = params["session_id"].as_str().unwrap();
                    let mut value = snapshot(id, &state.sessions[id], state.generation);
                    if state.sessions[id].prompt.contains("moved") {
                        // Hermes reset the conversation: a fresh stored session took the route
                        // and a notice queued behind the failed turn waits to run there.
                        value["stored_session_id"] = json!(format!("{id}-moved"));
                        if !state.foreign_cancelled.contains(id) {
                            value["running"] = json!(true);
                            value["pending"] = json!([{"admission_id":"foreign-admission",
                                "ref":{"profile_id":"fixture","session_id":id},"sequence":2,
                                "status":"queued","outcome":null,"authority_epoch":1,
                                "execution_generation":null,"input_id":"foreign-input",
                                "text":"[a background process finished]"}]);
                        }
                    }
                    value
                }
                "session.stats" => json!({"total_tokens":4321}),
                "prompt.cancel" if params["admission_id"] == "foreign-admission" => {
                    let id = params["session_id"].as_str().unwrap();
                    state.foreign_cancelled.insert(id.into());
                    json!({"admission_id":"foreign-admission","ref":{"profile_id":"fixture","session_id":id},
                        "sequence":2,"status":"terminal","outcome":"cancelled","authority_epoch":1,
                        "execution_generation":null})
                }
                "prompt.submit" => {
                    let id = params["session_id"].as_str().unwrap();
                    let session = state.sessions.get_mut(id).unwrap();
                    session.prompt = params["text"].as_str().unwrap().into();
                    session.status = if session.prompt.contains("queued") {
                        "queued"
                    } else {
                        "started"
                    }
                    .into();
                    // A prompt naming `usage:N` streams a cumulative session.usage of N tokens.
                    if let Some(tokens) = session
                        .prompt
                        .split("usage:")
                        .nth(1)
                        .and_then(|rest| rest.split(|c: char| !c.is_ascii_digit()).next())
                        .and_then(|digits| digits.parse::<u64>().ok())
                    {
                        let frame = EventFrame::from_params(&json!({"type":"session.usage","session_id":id,
                            "payload":{"input_tokens":tokens - 1,"output_tokens":1,"prompt_tokens":tokens - 1,
                            "completion_tokens":1,"total_tokens":tokens,"api_calls":1}}))
                        .unwrap();
                        let _ = events.try_send(WireEvent::Event {
                            session_id: id.into(),
                            frame,
                        });
                    }
                    receipt(id, session, &session.status, false)
                }
                "prompt.resolve_unknown" => {
                    let id = params["session_id"].as_str().unwrap();
                    let session = state.sessions.get_mut(id).unwrap();
                    session.unknown_resolved = true;
                    receipt(id, session, "terminal", false)
                }
                "prompt.receipt" => {
                    let id = params["session_id"].as_str().unwrap();
                    let session = &state.sessions[id];
                    let status = if session.stopped || session.unknown_resolved {
                        "terminal"
                    } else if session.prompt.contains("unknown") {
                        "unknown"
                    } else if session.prompt.split_whitespace().any(|word| word == "hold")
                        // An isolated agent's first message starts with its working folder, so
                        // a release names the script's own prompt text at the end.
                        && !state.released.iter().any(|r| session.prompt.ends_with(r.as_str()))
                    {
                        &session.status
                    } else {
                        "terminal"
                    };
                    receipt(id, session, status, params["include_result"] == true)
                }
                "session.interrupt" if state.failing_interrupts > 0 => {
                    state.failing_interrupts -= 1;
                    return Err(WireError::Timeout);
                }
                "prompt.cancel" | "session.interrupt" => {
                    let id = params["session_id"].as_str().unwrap();
                    let unconfirmed = state.unconfirmed;
                    let session = state.sessions.get_mut(id).unwrap();
                    if !unconfirmed {
                        session.stopped = true;
                    }
                    if method == "prompt.cancel" {
                        receipt(
                            id,
                            session,
                            if session.stopped {
                                "terminal"
                            } else {
                                "queued"
                            },
                            false,
                        )
                    } else {
                        json!({"ref":{"profile_id":"fixture","session_id":id},"instance_id":"instance",
                            "authority_epoch":1,"revision":1,"execution_generation":1,"execution_state":"running"})
                    }
                }
                "session.list" => json!({"sessions":[],"scope":"live"}),
                // The pinned gateway serves no `session.close` on the session socket.
                "session.close" => {
                    return Err(WireError::Refused {
                        code: -32601,
                        message: "unknown_method".into(),
                        data: Value::Null,
                    });
                }
                "session.detach" => {
                    let id = params["session_id"].as_str().unwrap();
                    json!({"session_id":id,"subscription_id":params["subscription_id"],
                        "detached":!state.stale_subscriptions
                            && params["subscription_id"] == subscription(id, state.generation).as_str()})
                }
                _ => return Err(WireError::Other(format!("unexpected method: {method}"))),
            };
            Ok(result)
        })
    }
    fn generation(&self) -> u64 {
        self.inner.lock().unwrap().generation.max(1)
    }
    fn track(&self, session_id: &str, _cursor: Cursor) {
        self.inner.lock().unwrap().tracked.insert(session_id.into());
    }
    fn untrack(&self, session_id: &str) {
        self.inner.lock().unwrap().tracked.remove(session_id);
    }
    fn resync(&self, _session_id: &str, _cursor: Cursor) {}
    fn take_events(&self) -> Option<tokio_mpsc::Receiver<WireEvent>> {
        self.events.lock().unwrap().take()
    }
}

fn roster(authenticated: bool) -> Value {
    json!({"model":"scripted-model","provider":"scripted","providers":[{"slug":"scripted","name":"Scripted",
        "authenticated":authenticated,"models":["scripted-model","alternate-model"]}]})
}
fn services(wire: &CountingWireFactory, authenticated: bool) -> RunnerServices {
    let source = Arc::new(FixtureRosterSource::new());
    for _ in 0..20 {
        source.push("fixture", Ok(roster(authenticated)));
    }
    let mut config = PortConfig::default();
    config.poll.start = Duration::from_millis(20);
    config.poll.cap = Duration::from_millis(20);
    RunnerServices::new(
        Arc::new(wire.clone()),
        source,
        Arc::new(SystemClock),
        config,
    )
}
fn script(body: &str) -> String {
    format!("export const meta = {{name:'fixture',description:'runner fixture'}};\n{body}\n")
}
struct Harness {
    _temp: tempfile::TempDir,
    runs: PathBuf,
    workflows: PathBuf,
    root: PathBuf,
    source: PathBuf,
    staffing: PathBuf,
    wire: CountingWireFactory,
}
impl Harness {
    fn new(body: &str) -> Self {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().to_path_buf();
        let source = root.join("root.js");
        fs::write(&source, script(body)).unwrap();
        let staffing = root.join("staffing.toml");
        fs::write(&staffing, "version = 1\n[defaults]\ntier = 'first'\n[tiers.first]\nmodel = 'scripted-model'\nprovider = 'scripted'\n[tiers.second]\nmodel = 'scripted-model'\nprovider = 'scripted'\n[limits]\nmax_in_flight = 1\n").unwrap();
        Self {
            runs: root.join("runs"),
            workflows: root.join("workflows"),
            root,
            source,
            staffing,
            wire: CountingWireFactory::default(),
            _temp: temp,
        }
    }
    fn request(&self) -> RunRequest {
        RunRequest {
            script: self.source.clone(),
            args: Value::Null,
            profile: "fixture".into(),
            session_model: None,
            staffing: Some(self.staffing.clone()),
            cwd: self.root.clone(),
            budget_tokens: None,
            budget_usd: None,
            on_unknown: OnUnknown::Wait,
            runs_dir: self.runs.clone(),
            workflows_dir: self.workflows.clone(),
            talaria_version: "test-version".into(),
            build: Default::default(),
            json: true,
            record: None,
            settle_bound: Duration::from_millis(600),
            receipt_poll: Duration::from_millis(20),
        }
    }
    fn resume(&self, target: &str) -> ResumeRequest {
        ResumeRequest {
            target: target.into(),
            script: None,
            staffing: Some(self.staffing.clone()),
            cwd: None,
            budget_tokens: None,
            budget_usd: None,
            record: None,
            settle_bound: Duration::from_millis(600),
            receipt_poll: Duration::from_millis(20),
            runs_dir: self.runs.clone(),
            workflows_dir: self.workflows.clone(),
            json: true,
        }
    }
}

fn event_value(event: &RunnerEvent) -> Value {
    match event {
        RunnerEvent::Diagnostic { text } => json!({"event":"diagnostic","text":text}),
        RunnerEvent::RunStarted { run_id, run_dir } => {
            json!({"event":"run_started","run_id":run_id,"run_dir":run_dir})
        }
        RunnerEvent::Parked {
            run_id,
            label,
            key,
            attempt,
            episode,
            cause,
            reason,
            choices,
            saved_branch,
        } => {
            json!({"event":"parked","run_id":run_id,"label":label,"key":key,"attempt":attempt,"episode":episode,"cause":cause,"reason":reason,"choices":choices,"saved_branch":saved_branch})
        }
        RunnerEvent::WaitingForReply {
            run_id,
            label,
            key,
            session_id,
        } => {
            json!({"event":"waiting_for_reply","run_id":run_id,"label":label,"key":key,"session_id":session_id})
        }
        RunnerEvent::Finished {
            run_id,
            outcome,
            result,
            spend,
        } => json!({"event":"run_finished","run_id":run_id,"outcome":outcome,"result":result,
            "spend":{"tokens":spend.tokens,"money_usd":spend.money_usd,"money_source":spend.money_source}}),
        RunnerEvent::Phase { title } => json!({"event":"phase","title":title}),
        RunnerEvent::Log { message } => json!({"event":"log","message":message}),
        RunnerEvent::AgentStarted {
            label,
            model,
            provider,
        } => json!({"event":"agent_started","label":label,"model":model,"provider":provider}),
        RunnerEvent::AgentFinished { label, state } => {
            json!({"event":"agent_finished","label":label,"state":state})
        }
        RunnerEvent::Scheduler(value) => json!({"event":"scheduler","value":value}),
    }
}
enum Start {
    New(RunRequest),
    Resume(ResumeRequest),
}
struct Worker {
    abort: Option<oneshot::Sender<()>>,
    exit: mpsc::Receiver<RunExit>,
    thread: Option<thread::JoinHandle<()>>,
    events: Arc<Mutex<Vec<Value>>>,
}
impl Worker {
    fn start(start: Start, services: RunnerServices) -> Self {
        let (abort, signal) = oneshot::channel();
        let (tx, exit) = mpsc::channel();
        let events = Arc::new(Mutex::new(Vec::new()));
        let shared = events.clone();
        let thread =
            thread::spawn(move || {
                let runtime = tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                    .unwrap();
                runtime.block_on(async move {
                let mut out = |event: &RunnerEvent| shared.lock().unwrap().push(event_value(event));
                let future = async { match start {
                    Start::New(req) => run_new_with_services(req, services, &mut out).await,
                    Start::Resume(req) => run_resume_with_services(req, services, &mut out).await,
                }};
                tokio::select! { result = future => { let _ = tx.send(result); }, _ = signal => {} }
            });
            });
        Self {
            abort: Some(abort),
            exit,
            thread: Some(thread),
            events,
        }
    }
    fn finish(mut self) -> (RunExit, Vec<Value>) {
        let result = self
            .exit
            .recv_timeout(Duration::from_secs(12))
            .expect("runner did not finish");
        self.abort.take();
        self.thread.take().unwrap().join().unwrap();
        (result, self.events.lock().unwrap().clone())
    }
    fn kill(mut self) -> Vec<Value> {
        self.abort.take().unwrap().send(()).unwrap();
        self.thread.take().unwrap().join().unwrap();
        self.events.lock().unwrap().clone()
    }
    fn events(&self) -> Vec<Value> {
        self.events.lock().unwrap().clone()
    }
}
impl Drop for Worker {
    fn drop(&mut self) {
        if let Some(abort) = self.abort.take() {
            let _ = abort.send(());
        }
    }
}
fn wait_for(mut condition: impl FnMut() -> bool) {
    let until = Instant::now() + Duration::from_secs(10);
    while !condition() {
        assert!(Instant::now() < until, "condition timed out");
        thread::sleep(Duration::from_millis(20));
    }
}
fn run_folder(worker: &Worker) -> PathBuf {
    wait_for(|| worker.events().iter().any(|e| e["event"] == "run_started"));
    let event = worker
        .events()
        .into_iter()
        .find(|e| e["event"] == "run_started")
        .unwrap();
    PathBuf::from(event["run_dir"].as_str().unwrap())
}
fn records(dir: &Path) -> Vec<Value> {
    fs::read_to_string(dir.join("journal.jsonl"))
        .unwrap_or_default()
        .lines()
        .filter_map(|line| serde_json::from_str(line).ok())
        .collect()
}
fn key_for(dir: &Path, label: &str) -> String {
    records(dir)
        .iter()
        .find(|r| r["t"] == "call_intent" && r["label"] == label)
        .unwrap()["key"]
        .as_str()
        .unwrap()
        .into()
}
fn count_for(frames: &[Frame], method: &str, key: &str) -> usize {
    frames
        .iter()
        .filter(|f| {
            f.method == method
                && ["request_id", "submission_id"].iter().any(|field| {
                    f.params
                        .get(*field)
                        .and_then(Value::as_str)
                        .is_some_and(|value| value.contains(key))
                })
        })
        .count()
}
fn now_ms() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis()
}
fn write_pause(dir: &Path) {
    fs::create_dir_all(dir.join("control")).unwrap();
    fs::write(
        dir.join("control/pause.json"),
        format!("{{\"v\":1,\"kind\":\"pause\",\"at_ms\":{}}}", now_ms()),
    )
    .unwrap();
}

#[test]
fn preflight_refuses_before_creating_a_run() {
    let h = Harness::new("return 'unused';");
    let mut req = h.request();
    req.budget_usd = Some(2.0);
    let worker = Worker::start(Start::New(req), services(&h.wire, false));
    let (exit, events) = worker.finish();
    assert_eq!(exit.code, 2);
    assert!(!h.runs.exists());
    for tier in ["first", "second"] {
        assert!(
            exit.problems
                .iter()
                .any(|line| line.contains(tier) && line.contains("not signed in")),
            "{:?}",
            exit.problems
        );
    }
    let diagnostics: Vec<_> = events
        .iter()
        .filter(|e| e["event"] == "diagnostic")
        .map(|e| e["text"].as_str().unwrap().to_owned())
        .collect();
    for problem in &exit.problems {
        assert!(diagnostics.contains(problem));
    }
    assert!(
        h.wire
            .frames()
            .iter()
            .all(|f| f.method != "session.create" && f.method != "prompt.submit")
    );

    let h = Harness::new("return 'ok';");
    let mut req = h.request();
    req.budget_usd = Some(2.0);
    let (exit, events) = Worker::start(Start::New(req), services(&h.wire, true)).finish();
    assert_eq!(exit.code, 0);
    assert!(events.iter().any(
        |e| e["event"] == "diagnostic" && e["text"].as_str().unwrap().contains("has no price")
    ));
}

struct FixedConfig(Result<Value, String>);
impl ConfigSource for FixedConfig {
    fn effective(&self, _profile: &str) -> BoxFuture<'static, Result<Value, String>> {
        let answer = self.0.clone();
        Box::pin(async move { answer })
    }
}
fn diagnostics_of(events: &[Value]) -> Vec<String> {
    events
        .iter()
        .filter(|e| e["event"] == "diagnostic")
        .map(|e| e["text"].as_str().unwrap().to_owned())
        .collect()
}

#[test]
fn a_profile_that_cannot_compress_in_place_warns_before_the_run_and_still_runs() {
    let h = Harness::new("return 'ok';");
    let config = json!({"compression":{"in_place":false},"agent":{"max_turns":60}});
    let services = services(&h.wire, true).with_config(Arc::new(FixedConfig(Ok(config))));
    let (exit, events) = Worker::start(Start::New(h.request()), services).finish();
    assert_eq!(exit.code, 0, "{:?}", exit.problems);
    let warnings: Vec<_> = diagnostics_of(&events)
        .into_iter()
        .filter(|text| text.contains("compression.in_place"))
        .collect();
    assert_eq!(warnings.len(), 1, "{warnings:?}");
    assert!(warnings[0].starts_with("warning:") && warnings[0].contains("context overflow"));
    assert!(warnings[0].contains("The run starts anyway"));
    assert!(
        diagnostics_of(&events)
            .iter()
            .all(|t| !t.contains("agent.max_turns")),
        "workflow agents are not capped by the profile, so that setting is not warned about here"
    );
    let started = events
        .iter()
        .position(|e| e["event"] == "run_started")
        .expect("the run started");
    let warned = events
        .iter()
        .position(|e| {
            e["event"] == "diagnostic"
                && e["text"].as_str().unwrap().contains("compression.in_place")
        })
        .expect("the warning was printed");
    assert!(warned < started, "the warning comes before the run starts");
}

#[test]
fn a_profile_with_default_or_unreadable_settings_adds_no_warning() {
    for config in [
        Ok(json!({"compression":{"in_place":true},"agent":{"max_turns":null}})),
        Err("gateway unreachable".to_owned()),
    ] {
        let h = Harness::new("return 'ok';");
        let services = services(&h.wire, true).with_config(Arc::new(FixedConfig(config)));
        let (exit, events) = Worker::start(Start::New(h.request()), services).finish();
        assert_eq!(exit.code, 0, "{:?}", exit.problems);
        assert!(
            diagnostics_of(&events)
                .iter()
                .all(|t| !t.contains("compression.in_place")),
            "{:?}",
            diagnostics_of(&events)
        );
    }
}

#[test]
fn script_with_an_unknown_model_is_refused_before_any_agent_starts() {
    let h = Harness::new(
        "const good = await agent('one', {label:'one', model:'scripted-model', provider:'scripted'});\n\
         return await agent('two', {label:'two', model:'scripted-modle', provider:'scripted'});",
    );
    let worker = Worker::start(Start::New(h.request()), services(&h.wire, true));
    let (exit, events) = worker.finish();
    assert_eq!(exit.code, 2);
    assert!(!h.runs.exists(), "no run folder may be created");
    let unknown: Vec<_> = exit
        .problems
        .iter()
        .filter(|p| p.starts_with("script line"))
        .collect();
    assert_eq!(unknown.len(), 1, "{:?}", exit.problems);
    assert!(
        unknown[0]
            .contains("model 'scripted-modle' on 'scripted' is not usable: model is not offered")
            && unknown[0].contains("nearest known spelling: 'scripted-model'")
            && unknown[0].contains("line 3"),
        "{:?}",
        exit.problems
    );
    assert!(
        events.iter().any(|e| e["event"] == "diagnostic"
            && e["text"].as_str().unwrap().contains("scripted-modle"))
    );
    assert!(
        h.wire
            .frames()
            .iter()
            .all(|f| f.method != "session.create" && f.method != "prompt.submit")
    );
    // The same script with the right spelling runs.
    let h = Harness::new(
        "return await agent('two', {label:'two', model:'scripted-model', provider:'scripted'});",
    );
    let (exit, _) = Worker::start(Start::New(h.request()), services(&h.wire, true)).finish();
    assert_eq!(exit.code, 0, "{:?}", exit.problems);
}

/// A run whose staffing has no default tier, so a call naming no model or tier takes the
/// roster's current pair.
fn session_harness(body: &str) -> (Harness, RunRequest) {
    let h = Harness::new(body);
    fs::write(&h.staffing, "version = 1\n[limits]\nmax_in_flight = 1\n").unwrap();
    let mut req = h.request();
    req.session_model = Some(("alternate-model".into(), "scripted".into()));
    (h, req)
}

#[test]
fn a_call_without_model_or_tier_runs_on_the_session_model() {
    let (h, req) = session_harness("return await agent('review', {label:'review'});");
    let worker = Worker::start(Start::New(req), services(&h.wire, true));
    let folder = run_folder(&worker);
    let (exit, _) = worker.finish();
    assert_eq!(exit.code, 0, "{:?}", exit.problems);
    let creates: Vec<_> = h
        .wire
        .frames()
        .into_iter()
        .filter(|f| f.method == "session.create")
        .collect();
    assert_eq!(creates.len(), 1);
    assert_eq!(
        (&creates[0].params["model"], &creates[0].params["provider"]),
        (&json!("alternate-model"), &json!("scripted"))
    );
    let manifest: Value =
        serde_json::from_slice(&fs::read(folder.join("manifest.json")).unwrap()).unwrap();
    assert_eq!(
        manifest["roster"]["current"],
        json!({"model":"alternate-model","provider":"scripted","source":"session"})
    );
    // Without the session pair the same call runs on the profile's configured model.
    let (h, mut req) = session_harness("return await agent('review', {label:'review'});");
    req.session_model = None;
    let (exit, _) = Worker::start(Start::New(req), services(&h.wire, true)).finish();
    assert_eq!(exit.code, 0, "{:?}", exit.problems);
    let create = h
        .wire
        .frames()
        .into_iter()
        .find(|f| f.method == "session.create")
        .unwrap();
    assert_eq!(create.params["model"], "scripted-model");
}

#[test]
fn resume_keeps_the_session_model_the_run_started_with() {
    let (h, req) = session_harness(
        "const a=await agent('first', {label:'first'}); const b=await agent('hold second', {label:'second'}); return [a,b];",
    );
    let worker = Worker::start(Start::New(req), services(&h.wire, true));
    let folder = run_folder(&worker);
    wait_for(|| {
        let r = records(&folder);
        r.iter()
            .find(|v| v["t"] == "call_intent" && v["label"] == "second")
            .is_some_and(|intent| {
                r.iter()
                    .any(|v| v["t"] == "turn_admitted" && v["key"] == intent["key"])
            })
    });
    let first = key_for(&folder, "first");
    let before = h.wire.frames();
    worker.kill();
    h.wire.release("hold second");
    let target = folder.file_name().unwrap().to_string_lossy().into_owned();
    let (exit, _) =
        Worker::start(Start::Resume(h.resume(&target)), services(&h.wire, true)).finish();
    assert_eq!(exit.code, 0, "{:?}", exit.problems);
    // Staffing the resume from the profile's model would change the first call's key and run
    // it again.
    let after = h.wire.frames();
    assert_eq!(
        count_for(&after, "session.create", &first),
        count_for(&before, "session.create", &first)
    );
    assert!(
        after
            .iter()
            .filter(|f| f.method == "session.create")
            .all(|f| f.params["model"] == "alternate-model")
    );
}

#[test]
fn a_finished_agent_session_is_ended_in_hermes() {
    let h = Harness::new("return await agent('review', {label:'review', tier:'first'});");
    let worker = Worker::start(Start::New(h.request()), services(&h.wire, true));
    let folder = run_folder(&worker);
    let (exit, _) = worker.finish();
    assert_eq!(exit.code, 0);
    let frames = h.wire.frames();
    let methods: Vec<_> = frames.iter().map(|f| f.method.as_str()).collect();
    assert!(!methods.contains(&"session.close"), "{methods:?}");
    let detaches: Vec<_> = frames
        .iter()
        .filter(|f| f.method == "session.detach")
        .collect();
    assert_eq!(detaches.len(), 1, "{methods:?}");
    // The subscription the create answer named, with an end reason so Hermes ends the session.
    let params = &detaches[0].params;
    assert_eq!(params.len(), 3, "{params:?}");
    assert_eq!(params["session_id"], "session-1");
    assert_eq!(params["subscription_id"], "subscription-session-1");
    let reason = params["end_reason"].as_str().unwrap();
    assert!(
        reason.len() <= 32
            && reason.starts_with(|c: char| c.is_ascii_lowercase())
            && reason.chars().all(|c| c.is_ascii_lowercase() || c == '_'),
        "{reason}"
    );
    // The detach comes after the answer was read, never before.
    let position = |method: &str| frames.iter().rposition(|f| f.method == method).unwrap();
    assert!(position("session.detach") > position("prompt.receipt"));
    assert!(
        !records(&folder).iter().any(|r| r["event"]["message"]
            .as_str()
            .is_some_and(|m| m.contains("not ended"))),
        "a detach the gateway accepted is not reported"
    );

    // A detach the gateway turns down is noted in the journal and changes nothing else.
    let h = Harness::new("return await agent('review', {label:'review', tier:'first'});");
    h.wire.stale_subscriptions();
    let worker = Worker::start(Start::New(h.request()), services(&h.wire, true));
    let folder = run_folder(&worker);
    let (exit, _) = worker.finish();
    assert_eq!(exit.code, 0, "{:?}", exit.problems);
    let notes: Vec<String> = records(&folder)
        .iter()
        .filter_map(|r| r["event"]["message"].as_str().map(str::to_owned))
        .filter(|m| m.contains("not ended"))
        .collect();
    assert_eq!(notes.len(), 1, "{notes:?}");
    assert!(notes[0].contains("session session-1"), "{notes:?}");
}

#[test]
fn work_started_on_a_moved_route_is_cancelled_noted_and_counted() {
    // The first attempt overflows its context and Hermes moves the route; the restart answers.
    let h = Harness::new(
        "return await agent('review overflow moved', {label:'review', tier:'first'});",
    );
    let worker = Worker::start(Start::New(h.request()), services(&h.wire, true));
    let folder = run_folder(&worker);
    let (exit, events) = worker.finish();
    assert_eq!(exit.code, 0, "{:?}", exit.problems);
    let frames = h.wire.frames();
    let position = |method: &str| frames.iter().rposition(|f| f.method == method).unwrap();
    // The notice nobody tracked was cancelled before the session was detached.
    let cancels: Vec<_> = frames
        .iter()
        .filter(|f| f.method == "prompt.cancel")
        .collect();
    assert_eq!(cancels.len(), 1, "{frames:?}");
    assert_eq!(cancels[0].params["admission_id"], "foreign-admission");
    assert!(position("prompt.cancel") < position("session.detach"));
    // The journal says what was found, where, and that its usage was counted.
    let notes: Vec<String> = records(&folder)
        .iter()
        .filter_map(|r| r["event"]["message"].as_str().map(str::to_owned))
        .filter(|m| m.starts_with("runner: untracked work: "))
        .collect();
    assert_eq!(notes.len(), 1, "{notes:?}");
    for part in [
        "agent review",
        "session session-1",
        "new stored session session-1-moved",
        "cancelled 1 turn(s)",
        "4321 tokens",
    ] {
        assert!(notes[0].contains(part), "{part} missing from {notes:?}");
    }
    assert!(!notes[0].contains("still running"), "{notes:?}");
    // The note was also printed live.
    assert!(
        events
            .iter()
            .any(|e| e["event"] == "log" && e["message"] == notes[0].as_str())
    );
    // The stored run result carries the same note, without its fixed opening words.
    let outcome: Value =
        serde_json::from_str(&fs::read_to_string(folder.join("outcome.json")).unwrap()).unwrap();
    assert_eq!(
        outcome["untracked_work"],
        json!([notes[0].strip_prefix("runner: untracked work: ").unwrap()]),
        "{outcome}"
    );
    // The moved session's tokens are in the run's spend beside the ten of each attempt.
    let finished = events
        .iter()
        .find(|e| e["event"] == "run_finished")
        .unwrap();
    assert_eq!(finished["spend"]["tokens"], 4341, "{finished}");
    // The sweep ran before the restart started its new session.
    let created: Vec<_> = frames
        .iter()
        .enumerate()
        .filter(|(_, f)| f.method == "session.create")
        .map(|(i, _)| i)
        .collect();
    assert_eq!(created.len(), 2, "{frames:?}");
    assert!(
        frames
            .iter()
            .position(|f| f.method == "prompt.cancel")
            .unwrap()
            < created[1]
    );
}

#[test]
fn an_agent_that_runs_out_of_context_is_restarted_in_a_fresh_session_with_a_note() {
    let h = Harness::new("return await agent('review overflow', {label:'review', tier:'first'});");
    let worker = Worker::start(Start::New(h.request()), services(&h.wire, true));
    let folder = run_folder(&worker);
    let (exit, events) = worker.finish();
    assert_eq!(exit.code, 0, "{:?}", exit.problems);
    let finished = events
        .iter()
        .find(|e| e["event"] == "run_finished")
        .unwrap();
    assert_eq!(finished["outcome"], "completed", "{finished}");
    assert_eq!(finished["result"], "READY", "{finished}");
    // Two sessions: the first attempt and its restart, which carries the note.
    let frames = h.wire.frames();
    let created = frames
        .iter()
        .filter(|f| f.method == "session.create")
        .count();
    assert_eq!(created, 2, "{frames:?}");
    let prompts: Vec<&str> = frames
        .iter()
        .filter(|f| f.method == "prompt.submit")
        .filter_map(|f| f.params["text"].as_str())
        .collect();
    assert_eq!(prompts.len(), 2, "{prompts:?}");
    assert!(!prompts[0].contains("ran out of context"), "{}", prompts[0]);
    assert!(
        prompts[1].starts_with("Note: an earlier attempt at this task ran out of context"),
        "{}",
        prompts[1]
    );
    assert!(prompts[1].contains("review overflow"), "{}", prompts[1]);
    // The restart is in the scheduler stream, the journal and the run's output.
    assert!(events.iter().any(|e| e["event"] == "scheduler"
        && e["value"]["event"] == "agent_restarted"
        && e["value"]["restart"] == 1
        && e["value"]["max"] == 3
        && e["value"]["label"] == "review"));
    let r = records(&folder);
    assert_eq!(
        r.iter()
            .filter(|v| v["t"] == "call_finished" && v["retry"] == true)
            .count(),
        1
    );
    assert!(r.iter().any(
        |v| v["event"]["message"] == "agent review: restarted 1 of 3 after a context overflow"
    ));
    // Both attempts' tokens count toward the run.
    assert_eq!(finished["spend"]["tokens"], 20, "{finished}");
}

#[test]
fn a_sweep_that_could_not_check_the_route_stops_the_restart_and_is_not_untracked_work() {
    let h = Harness::new(
        "return await agent('review overflow sweepfails', {label:'review', tier:'first'});",
    );
    let worker = Worker::start(Start::New(h.request()), services(&h.wire, true));
    let folder = run_folder(&worker);
    let (exit, events) = worker.finish();
    assert_eq!(exit.code, 5, "{:?}", exit.problems);
    let notes: Vec<String> = records(&folder)
        .iter()
        .filter_map(|r| r["event"]["message"].as_str().map(str::to_owned))
        .filter(|m| m.starts_with("runner: route not checked: "))
        .collect();
    assert_eq!(notes.len(), 1, "{notes:?}");
    assert!(
        notes[0].contains(
            "could not check whether anything else was still running on session session-1"
        ),
        "{notes:?}"
    );
    assert!(notes[0].contains("look in "), "{notes:?}");
    // The same note was printed live.
    assert!(
        events
            .iter()
            .any(|e| e["event"] == "log" && e["message"] == notes[0].as_str())
    );
    // No second agent went into the folder: the call failed and says why.
    let created = h
        .wire
        .frames()
        .iter()
        .filter(|f| f.method == "session.create")
        .count();
    assert_eq!(created, 1);
    let outcome: Value =
        serde_json::from_str(&fs::read_to_string(folder.join("outcome.json")).unwrap()).unwrap();
    let message = outcome["failed_agents"][0]["reason"].as_str().unwrap();
    assert!(
        message.contains("could not check whether Hermes is still running untracked work in "),
        "{message}"
    );
    assert!(
        message.contains("so the agent was not restarted"),
        "{message}"
    );
    // A check that failed is not untracked work.
    assert!(outcome.get("untracked_work").is_none(), "{outcome}");
}

#[test]
fn an_ordinary_agent_leaves_nothing_to_report_from_its_route() {
    let h = Harness::new("return await agent('review', {label:'review', tier:'first'});");
    let worker = Worker::start(Start::New(h.request()), services(&h.wire, true));
    let folder = run_folder(&worker);
    let (exit, events) = worker.finish();
    assert_eq!(exit.code, 0, "{:?}", exit.problems);
    assert!(
        !h.wire
            .frames()
            .iter()
            .any(|f| f.method == "prompt.cancel" || f.method == "session.stats")
    );
    assert!(!records(&folder).iter().any(|r| {
        r["event"]["message"]
            .as_str()
            .is_some_and(|m| m.contains("untracked work"))
    }));
    let finished = events
        .iter()
        .find(|e| e["event"] == "run_finished")
        .unwrap();
    assert_eq!(finished["spend"]["tokens"], 10, "{finished}");
}

#[test]
fn a_worktree_with_uncommitted_edits_is_listed_when_the_run_ends() {
    let h = Harness::new(
        "return await agent('hold review', {label:'review', tier:'first', toolsets:['file'], isolation:'worktree'});",
    );
    let repo = h.root.join("source-repo");
    init_repository(&repo);
    // The run's own directory is a git repository too; it is not an agent worktree.
    let mut request = h.request();
    request.cwd = repo.canonicalize().unwrap();
    let worker = Worker::start(Start::New(request), services(&h.wire, true));
    let folder = run_folder(&worker);
    wait_for(|| h.wire.frames().iter().any(|f| f.method == "session.create"));
    let worktree = PathBuf::from(
        h.wire
            .frames()
            .iter()
            .find(|f| f.method == "session.create")
            .unwrap()
            .params["cwd"]
            .as_str()
            .unwrap(),
    );
    // The agent leaves two edits behind, and a file in the shared repository too.
    fs::write(worktree.join("one.txt"), "x").unwrap();
    fs::write(worktree.join("two.txt"), "x").unwrap();
    fs::write(repo.join("shared.txt"), "x").unwrap();
    h.wire.release("hold review");
    let (exit, events) = worker.finish();
    assert_eq!(exit.code, 0, "{:?}", exit.problems);
    let outcome: Value =
        serde_json::from_str(&fs::read_to_string(folder.join("outcome.json")).unwrap()).unwrap();
    let listed = outcome["leftover_edits"].as_array().unwrap();
    assert_eq!(listed.len(), 1, "{outcome}");
    assert_eq!(listed[0]["path"], worktree.to_string_lossy().as_ref());
    assert_eq!(listed[0]["changed_files"], 2, "{outcome}");
    let said = |m: &str| m.starts_with("uncommitted changes left in an agent working folder: ");
    assert!(
        events
            .iter()
            .any(|e| e["event"] == "log" && e["message"].as_str().is_some_and(said))
    );
    assert!(
        records(&folder)
            .iter()
            .any(|r| r["event"]["message"].as_str().is_some_and(said))
    );
    // Nothing was deleted or committed.
    assert!(worktree.join("one.txt").exists() && repo.join("shared.txt").exists());

    // An agent that ran in the shared directory lists nothing: Talaria did not make it.
    let h = Harness::new("return await agent('review', {label:'review', tier:'first'});");
    let worker = Worker::start(Start::New(h.request()), services(&h.wire, true));
    let folder = run_folder(&worker);
    let (exit, _) = worker.finish();
    assert_eq!(exit.code, 0, "{:?}", exit.problems);
    let outcome: Value =
        serde_json::from_str(&fs::read_to_string(folder.join("outcome.json")).unwrap()).unwrap();
    assert!(outcome.get("leftover_edits").is_none(), "{outcome}");
}

#[test]
fn a_failed_isolated_agent_has_its_uncommitted_work_saved_to_a_branch() {
    let h = Harness::new(
        "return await agent('hold review overflow sweepfails', {label:'review', tier:'first', toolsets:['file'], isolation:'worktree'});",
    );
    let repo = h.root.join("source-repo");
    init_repository(&repo);
    let mut request = h.request();
    request.cwd = repo.canonicalize().unwrap();
    let worker = Worker::start(Start::New(request), services(&h.wire, true));
    let folder = run_folder(&worker);
    wait_for(|| h.wire.frames().iter().any(|f| f.method == "session.create"));
    let worktree = PathBuf::from(
        h.wire
            .frames()
            .iter()
            .find(|f| f.method == "session.create")
            .unwrap()
            .params["cwd"]
            .as_str()
            .unwrap(),
    );
    fs::write(worktree.join("half.txt"), "half done").unwrap();
    h.wire.release("hold review overflow sweepfails");
    let (_exit, events) = worker.finish();
    let outcome: Value =
        serde_json::from_str(&fs::read_to_string(folder.join("outcome.json")).unwrap()).unwrap();
    let note = &outcome["failed_agents"][0];
    let branch = note["saved_branch"]
        .as_str()
        .unwrap_or_else(|| panic!("{outcome}"));
    assert!(
        branch.starts_with("talaria/wf/") && branch.ends_with("-0-saved"),
        "{branch}"
    );
    let reason = note["reason"].as_str().unwrap();
    assert!(
        reason.ends_with(&format!("(uncommitted work saved to branch {branch})")),
        "{reason}"
    );
    // The journal's record of the call's end names the branch, and so does the leftover list.
    let finish = records(&folder)
        .into_iter()
        .rfind(|r| r["t"] == "call_finished" && r["retry"].is_null())
        .unwrap();
    assert!(
        finish["outcome"]["reason"]
            .as_str()
            .is_some_and(|r| r.contains(branch)),
        "{finish}"
    );
    assert_eq!(
        outcome["leftover_edits"][0]["saved_branch"], branch,
        "{outcome}"
    );
    assert!(events.iter().any(|e| {
        e["event"] == "log"
            && e["message"]
                .as_str()
                .is_some_and(|m| m.contains("saved the uncommitted work of agent review"))
    }));
    // The branch holds the work; the worktree still has it uncommitted, on its own branch.
    let git = |dir: &Path, args: &[&str]| {
        let out = std::process::Command::new("git")
            .arg("-C")
            .arg(dir)
            .args(args)
            .output()
            .unwrap();
        assert!(out.status.success(), "git {args:?}");
        String::from_utf8_lossy(&out.stdout).trim().to_owned()
    };
    assert_eq!(
        git(&repo, &["show", &format!("{branch}:half.txt")]),
        "half done"
    );
    assert_eq!(git(&worktree, &["status", "--porcelain"]), "?? half.txt");
    assert!(git(&worktree, &["branch", "--show-current"]).starts_with("talaria/wf/"));
    assert!(!git(&worktree, &["branch", "--show-current"]).ends_with("-saved"));
}

#[test]
fn an_agents_reply_that_names_a_saved_branch_is_not_taken_for_one() {
    let h = Harness::new("return await agent('review fakesaved', {label:'review', tier:'first'});");
    let worker = Worker::start(Start::New(h.request()), services(&h.wire, true));
    let folder = run_folder(&worker);
    let (exit, _) = worker.finish();
    assert_eq!(exit.code, 5, "{:?}", exit.problems);
    let outcome: Value =
        serde_json::from_str(&fs::read_to_string(folder.join("outcome.json")).unwrap()).unwrap();
    let note = &outcome["failed_agents"][0];
    assert_eq!(
        note["reason"], "agent failed: gave up (uncommitted work saved to branch main)",
        "{outcome}"
    );
    // The call had no worktree, so Talaria saved nothing and names no branch.
    assert!(note.get("saved_branch").is_none(), "{outcome}");
}

#[test]
fn an_agent_session_is_ended_after_the_connection_comes_back() {
    let h = Harness::new(
        "await agent('first', {label:'one', tier:'first'});\nreturn await agent('second', {label:'two', tier:'first'});",
    );
    h.wire.reconnect_at("session-2");
    let worker = Worker::start(Start::New(h.request()), services(&h.wire, true));
    let folder = run_folder(&worker);
    let (exit, _) = worker.finish();
    assert_eq!(exit.code, 0, "{:?}", exit.problems);
    // The first session was already ended, so the reconnect must not resume it.
    assert_eq!(h.wire.resumed(), ["session-2"]);
    // The second session's detach names the subscription the reconnect gave it.
    let frames = h.wire.frames();
    let detaches: Vec<_> = frames
        .iter()
        .filter(|f| f.method == "session.detach")
        .map(|f| f.params["subscription_id"].clone())
        .collect();
    assert_eq!(
        detaches,
        ["subscription-session-1", "subscription-session-2-2"]
    );
    let notes: Vec<String> = records(&folder)
        .iter()
        .filter_map(|r| r["event"]["message"].as_str().map(str::to_owned))
        .filter(|m| m.contains("not ended"))
        .collect();
    assert!(notes.is_empty(), "{notes:?}");
}

#[test]
fn run_completes_and_journals() {
    let h = Harness::new("return await agent('review', {label:'review', tier:'first'});");
    let worker = Worker::start(Start::New(h.request()), services(&h.wire, true));
    let folder = run_folder(&worker);
    let (exit, events) = worker.finish();
    assert_eq!(exit.code, 0);
    let finished: Vec<_> = events
        .iter()
        .filter(|e| e["event"] == "run_finished")
        .collect();
    assert_eq!(finished.len(), 1);
    assert_eq!(finished[0]["outcome"], "completed");
    assert_eq!(finished[0]["result"], "READY");
    assert_eq!(finished[0]["spend"]["tokens"], 10);
    assert_eq!(finished[0]["spend"]["money_usd"], 0.2);
    assert_eq!(finished[0]["spend"]["money_source"], "hermes");
    let manifest: Value =
        serde_json::from_slice(&fs::read(folder.join("manifest.json")).unwrap()).unwrap();
    for key in [
        "run_id",
        "workflow",
        "args",
        "started_at_ms",
        "seed",
        "profile",
        "on_unknown",
        "talaria_version",
        "hermes",
        "roster",
        "approval_policy",
    ] {
        assert!(manifest.get(key).is_some(), "missing {key}");
    }
    assert_eq!(manifest["workflow"]["name"], "fixture");
    assert_eq!(manifest["profile"], "fixture");
    assert_eq!(manifest["hermes"]["describe"]["instance_id"], "instance");
    assert_eq!(manifest["approval_policy"]["mode"], "finite-only");
    // The policy is described in plain words, with no issue number or planning code.
    let source = manifest["approval_policy"]["source"].as_str().unwrap();
    assert!(source.contains("finite turns"), "{source}");
    assert!(!source.contains('#') && !source.contains("PH5"), "{source}");
    // The build that wrote the run sits beside its version; this request names none.
    assert_eq!(manifest["talaria_commit"], "unknown");
    assert_eq!(manifest["talaria_tree"], "unknown");
    assert!(
        records(&folder)
            .iter()
            .any(|r| r["t"] == "run_finished" && r["outcome"] == "completed")
    );
    let outcome: Value =
        serde_json::from_slice(&fs::read(folder.join("outcome.json")).unwrap()).unwrap();
    assert_eq!(outcome["outcome"], "completed");
    assert_eq!(outcome["result"], "READY");
    assert!(outcome["summary"].is_object());
    let frames = h.wire.frames();
    assert_eq!(
        frames
            .iter()
            .filter(|f| f.method == "session.create")
            .count(),
        1
    );
    assert_eq!(
        frames
            .iter()
            .filter(|f| f.method == "prompt.submit")
            .count(),
        1
    );
    assert!(
        frames
            .iter()
            .filter(|f| f.method == "prompt.submit")
            .all(|f| !f.params.contains_key("unattended"))
    );
}

#[test]
fn bad_args_and_bad_script_refused() {
    let h = Harness::new("return 'unused';");
    fs::write(
        &h.source,
        "export const meta = {name: BAD, description:'bad'};\nreturn 1;\n",
    )
    .unwrap();
    let worker = Worker::start(Start::New(h.request()), services(&h.wire, true));
    let (exit, events) = worker.finish();
    assert_eq!(exit.code, 2);
    assert!(!h.runs.exists());
    assert!(
        exit.problems
            .iter()
            .chain(
                events
                    .iter()
                    .filter_map(|e| e["text"].as_str().map(str::to_owned))
                    .collect::<Vec<_>>()
                    .iter()
            )
            .any(|line| line.contains("line"))
    );
    assert!(h.wire.frames().is_empty());
}

#[test]
fn resume_does_not_rerun_finished() {
    fn scenario(drift: bool) {
        let h = Harness::new(
            "const a=await agent('first', {label:'first', tier:'first'}); const b=await agent('hold second', {label:'second', tier:'second'}); return [a,b];",
        );
        let worker = Worker::start(Start::New(h.request()), services(&h.wire, true));
        let folder = run_folder(&worker);
        wait_for(|| {
            let r = records(&folder);
            r.iter().any(|v| v["t"] == "call_finished")
                && r.iter()
                    .find(|v| v["t"] == "call_intent" && v["label"] == "second")
                    .is_some_and(|intent| {
                        r.iter()
                            .any(|v| v["t"] == "turn_admitted" && v["key"] == intent["key"])
                    })
        });
        let first = key_for(&folder, "first");
        let second = key_for(&folder, "second");
        let before = h.wire.frames();
        assert_eq!(count_for(&before, "session.create", &first), 1);
        assert_eq!(count_for(&before, "prompt.submit", &first), 1);
        assert_eq!(count_for(&before, "session.create", &second), 1);
        worker.kill();
        if drift {
            let staffing = fs::read_to_string(&h.staffing).unwrap().replacen(
                "model = 'scripted-model'",
                "model = 'alternate-model'",
                1,
            );
            fs::write(&h.staffing, staffing).unwrap();
        }
        h.wire.release("hold second");
        let target = folder.file_name().unwrap().to_string_lossy().into_owned();
        let (exit, _) =
            Worker::start(Start::Resume(h.resume(&target)), services(&h.wire, true)).finish();
        assert_eq!(exit.code, 0);
        let after = h.wire.frames();
        for method in ["session.create", "prompt.submit"] {
            assert_eq!(
                count_for(&after, method, &first),
                count_for(&before, method, &first)
            );
            assert_eq!(
                count_for(&after, method, &second),
                count_for(&before, method, &second)
            );
        }
        let later = &after[before.len()..];
        let attach = later
            .iter()
            .position(|f| f.method == "session.resume")
            .expect("no reattachment");
        let poll = later
            .iter()
            .position(|f| f.method == "prompt.receipt")
            .expect("no receipt poll");
        assert!(attach < poll);
        if drift {
            let new_first = records(&folder)
                .into_iter()
                .find(|r| r["t"] == "call_intent" && r["label"] == "first" && r["key"] != first)
                .unwrap();
            let new_key = new_first["key"].as_str().unwrap();
            assert_eq!(count_for(&after, "session.create", new_key), 1);
            assert_eq!(count_for(&after, "prompt.submit", new_key), 1);
        }
    }
    scenario(false);
    scenario(true);
}

#[test]
fn second_runner_refused() {
    let h = Harness::new("return await agent('hold first', {label:'first'});");
    let worker = Worker::start(Start::New(h.request()), services(&h.wire, true));
    let folder = run_folder(&worker);
    wait_for(|| records(&folder).iter().any(|r| r["t"] == "turn_admitted"));
    let target = folder.file_name().unwrap().to_string_lossy().into_owned();
    let (second, _) =
        Worker::start(Start::Resume(h.resume(&target)), services(&h.wire, true)).finish();
    assert_eq!(second.code, 1);
    assert!(second.problems.iter().any(|line| line.contains(&target)));
    assert!(!records(&folder).iter().any(|r| r["t"] == "run_finished"));
    h.wire.release("hold first");
    assert_eq!(worker.finish().0.code, 0);
}

#[test]
fn stop_confirms_settlement() {
    fn scenario(unconfirmed: bool) {
        let h = Harness::new(
            "return await parallel([() => agent('hold started', {label:'started'}), () => agent('hold queued', {label:'queued'})]);",
        );
        let staffing = fs::read_to_string(&h.staffing)
            .unwrap()
            .replace("max_in_flight = 1", "max_in_flight = 2");
        fs::write(&h.staffing, staffing).unwrap();
        if unconfirmed {
            h.wire.unconfirmed();
        }
        let worker = Worker::start(Start::New(h.request()), services(&h.wire, true));
        let folder = run_folder(&worker);
        wait_for(|| {
            records(&folder)
                .iter()
                .filter(|r| r["t"] == "turn_admitted")
                .count()
                == 2
        });
        let target = folder.file_name().unwrap().to_string_lossy().into_owned();
        let stop = request_stop(&h.runs, &target, Duration::from_secs(5));
        assert_eq!(
            stop,
            if unconfirmed {
                StopExit::Unconfirmed
            } else {
                StopExit::Stopped
            }
        );
        let (exit, _) = worker.finish();
        assert_eq!(exit.code, 3);
        let outcome: Value =
            serde_json::from_slice(&fs::read(folder.join("outcome.json")).unwrap()).unwrap();
        assert_eq!(outcome["outcome"], "aborted");
        assert_eq!(
            outcome["reason"],
            if unconfirmed {
                "stopped-unconfirmed"
            } else {
                "stopped"
            }
        );
        assert!(
            records(&folder)
                .iter()
                .any(|r| r["t"] == "run_finished" && r["outcome"] == "aborted")
        );
        if !unconfirmed {
            assert_eq!(
                records(&folder)
                    .iter()
                    .filter(|r| r["t"] == "turn_settled")
                    .count(),
                2
            );
        }
        let frames = h.wire.frames();
        assert_eq!(
            frames
                .iter()
                .filter(|f| f.method == "session.interrupt")
                .count(),
            1
        );
        assert_eq!(
            frames
                .iter()
                .filter(|f| f.method == "prompt.cancel")
                .count(),
            1
        );
        assert!(!folder.join("control/stop.json").exists());
        // The runner noted each stop before sending it. A settlement it confirmed used its
        // note up; one it could not confirm keeps the note naming the stopped turn, so the
        // next execution still knows the stop was the run's.
        let dir = RunDir::open(&folder).unwrap();
        for admitted in records(&folder)
            .iter()
            .filter(|r| r["t"] == "turn_admitted")
        {
            let key = admitted["key"].as_str().unwrap();
            let admission = admitted["admission_id"].as_str().unwrap();
            if unconfirmed {
                assert_eq!(
                    dir.agent_stop_for(key, 0, 0, admission),
                    Some(talaria_workflow::journal::StopSource::RunStop),
                    "{key}"
                );
            } else {
                assert_eq!(dir.agent_stop(key), None, "{key}");
            }
        }
    }
    scenario(false);
    scenario(true);
}

#[test]
fn pause_blocks_new_admissions() {
    let h = Harness::new(
        "return await parallel([() => agent('hold first', {label:'first'}), () => agent('second', {label:'second'})]);",
    );
    let worker = Worker::start(Start::New(h.request()), services(&h.wire, true));
    let folder = run_folder(&worker);
    wait_for(|| records(&folder).iter().any(|r| r["t"] == "turn_admitted"));
    write_pause(&folder);
    thread::sleep(Duration::from_millis(350));
    h.wire.release("hold first");
    wait_for(|| records(&folder).iter().any(|r| r["t"] == "call_finished"));
    thread::sleep(Duration::from_millis(350));
    assert_eq!(
        h.wire
            .frames()
            .iter()
            .filter(|f| f.method == "prompt.submit")
            .count(),
        1
    );
    assert!(
        records(&folder)
            .iter()
            .any(|r| r["t"] == "call_intent" && r["label"] == "second")
    );
    fs::remove_file(folder.join("control/pause.json")).unwrap();
    assert_eq!(worker.finish().0.code, 0);
    assert_eq!(
        h.wire
            .frames()
            .iter()
            .filter(|f| f.method == "prompt.submit")
            .count(),
        2
    );
}

#[test]
fn resolve_answers_parked_call() {
    let h = Harness::new("return await agent('unknown', {label:'uncertain'});");
    let worker = Worker::start(Start::New(h.request()), services(&h.wire, true));
    let folder = run_folder(&worker);
    wait_for(|| worker.events().iter().any(|e| e["event"] == "parked"));
    let parked = worker
        .events()
        .into_iter()
        .find(|e| e["event"] == "parked")
        .unwrap();
    let id = parked["run_id"].as_str().unwrap();
    let key = parked["key"].as_str().unwrap();
    let attempt = parked["attempt"].as_u64().unwrap() as u32;
    let episode = parked["episode"].as_u64().unwrap() as u32;
    answer_parked(&h.runs, id, key, attempt, episode, Resolution::Fail).unwrap();
    assert!(answer_parked(&h.runs, id, key, attempt, episode, Resolution::Fail).is_err());
    let (exit, events) = worker.finish();
    assert_eq!(exit.code, 5);
    let finish = events
        .iter()
        .find(|e| e["event"] == "run_finished")
        .unwrap();
    assert_eq!(finish["result"], Value::Null);
    assert_eq!(
        h.wire
            .frames()
            .iter()
            .filter(|f| f.method == "prompt.submit")
            .count(),
        1
    );
    assert!(
        records(&folder)
            .iter()
            .any(|r| r["t"] == "call_resolved" && r["resolution"] == "fail")
    );

    let h = Harness::new("return await agent('unknown', {label:'uncertain'});");
    let mut req = h.request();
    req.on_unknown = OnUnknown::Fail;
    let worker = Worker::start(Start::New(req), services(&h.wire, true));
    let folder = run_folder(&worker);
    let (exit, events) = worker.finish();
    assert_eq!(exit.code, 5);
    assert_eq!(
        events
            .iter()
            .find(|e| e["event"] == "run_finished")
            .unwrap()["result"],
        Value::Null
    );
    assert_eq!(
        h.wire
            .frames()
            .iter()
            .filter(|f| f.method == "prompt.submit")
            .count(),
        1
    );
    assert_eq!(fs::read_dir(folder.join("resolutions")).unwrap().count(), 0);
}

#[test]
fn budgets_and_manifest_fields() {
    use sha2::{Digest, Sha256};
    let h = Harness::new("return await agent('review', {label:'review'});");
    let mut req = h.request();
    req.budget_tokens = Some(1234);
    req.budget_usd = Some(2.5);
    let worker = Worker::start(Start::New(req), services(&h.wire, true));
    let folder = run_folder(&worker);
    let (exit, _) = worker.finish();
    assert_eq!(exit.code, 0);
    let bytes = fs::read(folder.join("manifest.json")).unwrap();
    let manifest: Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(manifest["workflow"]["name"], "fixture");
    assert_eq!(manifest["talaria_version"], "test-version");
    assert_eq!(manifest["roster"]["budget"]["tokens"], 1234);
    assert_eq!(manifest["roster"]["budget"]["money_usd"], 2.5);
    let hash = hex::encode(Sha256::digest(fs::read(&h.source).unwrap()));
    assert_eq!(manifest["workflow"]["script_sha256"], hash);
    let mut seed_hash = Sha256::new();
    seed_hash.update(manifest["started_at_ms"].as_i64().unwrap().to_string());
    seed_hash.update(hash.as_bytes());
    let digest = seed_hash.finalize();
    let seed = u64::from_be_bytes(digest[..8].try_into().unwrap());
    assert_eq!(manifest["seed"], seed);
    let before = h.wire.frames().len();
    let target = folder.file_name().unwrap().to_string_lossy().into_owned();
    let (resumed, _) =
        Worker::start(Start::Resume(h.resume(&target)), services(&h.wire, true)).finish();
    assert_eq!(resumed.code, 0);
    assert_eq!(h.wire.frames().len(), before);
    assert_eq!(fs::read(folder.join("manifest.json")).unwrap(), bytes);
}

#[test]
fn token_budget_stops_running_agents_and_refuses_new_ones() {
    // Worker one streams 1,000 tokens of live usage before its turn settles; the budget is 500.
    // Worker two waits at the admission gate (one agent at a time) and never reaches Hermes.
    let h = Harness::new(
        "const results = await parallel([1, 2].map(i => () => agent(`hold usage:1000 worker ${i}`, {label:`worker-${i}`}).catch(e => `${e.name}: ${e.message}`)));
         let late; try { await agent('late work', {label:'late'}); late = 'ran'; } catch (e) { late = `${e.name}: ${e.message}`; }
         return {results, late, spent: budget.spent()};",
    );
    let mut req = h.request();
    req.budget_tokens = Some(500);
    let worker = Worker::start(Start::New(req), services(&h.wire, true));
    let folder = run_folder(&worker);
    let (exit, events) = worker.finish();
    assert_eq!(exit.code, 4, "{:?}", exit.problems);
    let exhausted = "BudgetExhaustedError: token budget exhausted (spent 1000 of 500)";
    let finished = events
        .iter()
        .find(|e| e["event"] == "run_finished")
        .unwrap();
    assert_eq!(finished["outcome"], "budget_exhausted");
    assert_eq!(
        finished["result"],
        json!({"results":[exhausted, exhausted],"late":exhausted,"spent":1000})
    );
    assert_eq!(finished["spend"]["tokens"], 1000);
    for (label, state) in [
        ("worker-1", "stopped"),
        ("worker-2", "stopped"),
        ("late", "refused"),
    ] {
        assert!(
            events.iter().any(|e| e["event"] == "agent_finished"
                && e["label"] == label
                && e["state"] == state),
            "{label} {state}: {events:?}"
        );
    }

    // Only the first worker reached Hermes, and the runner interrupted its running turn.
    let frames = h.wire.frames();
    assert_eq!(
        frames
            .iter()
            .filter(|f| f.method == "prompt.submit")
            .count(),
        1
    );
    assert_eq!(
        frames
            .iter()
            .filter(|f| f.method == "session.interrupt")
            .count(),
        1
    );

    // Each refusal and each stop is its own journal record with the reason.
    let journal = records(&folder);
    let refused: Vec<_> = journal
        .iter()
        .filter(|r| r["t"] == "engine" && r["event"]["event"] == "agent_refused")
        .collect();
    assert_eq!(refused.len(), 1);
    assert_eq!(refused[0]["event"]["label"], "late");
    assert_eq!(
        refused[0]["event"]["reason"],
        "token budget exhausted (spent 1000 of 500)"
    );
    let stopped: Vec<_> = journal
        .iter()
        .filter(|r| r["t"] == "call_finished" && r["outcome"]["kind"] == "stopped")
        .collect();
    assert_eq!(stopped.len(), 2);
    assert!(
        stopped
            .iter()
            .all(|r| r["outcome"]["reason"] == "token budget exhausted (spent 1000 of 500)")
    );
    assert!(journal.iter().any(|r| {
        r["t"] == "engine"
            && r["event"]["event"] == "log"
            && r["event"]["message"]
                .as_str()
                .unwrap()
                .contains("no new agents start")
    }));
    let finish = journal.iter().find(|r| r["t"] == "run_finished").unwrap();
    assert_eq!(finish["outcome"], "budget_exhausted");
    assert_eq!(finish["summary"]["refused"], 1);
    assert_eq!(finish["summary"]["stopped"], 2);
    // The runner noted the budget stop before sending it, so the settled turn says who sent it
    // even had it been read only by a later execution, and the used note is gone.
    let settled: Vec<_> = journal
        .iter()
        .filter(|r| r["t"] == "turn_settled")
        .collect();
    assert_eq!(settled.len(), 1);
    assert_eq!(
        settled[0]["result_meta"]["stopped_by"], "budget",
        "{}",
        settled[0]
    );
    let worker_one = settled[0]["key"].as_str().unwrap();
    assert_eq!(RunDir::open(&folder).unwrap().agent_stop(worker_one), None);

    // The stored outcome keeps the script's result and says why the run did not complete.
    let outcome: Value =
        serde_json::from_slice(&fs::read(folder.join("outcome.json")).unwrap()).unwrap();
    assert_eq!(outcome["outcome"], "budget_exhausted");
    assert_eq!(
        outcome["reason"],
        "token budget exhausted (spent 1000 of 500)"
    );
    assert_eq!(outcome["result"]["late"], exhausted);
    assert_eq!(outcome["summary"]["refused"], 1);
    assert_eq!(outcome["summary"]["stopped"], 2);

    // The workflow view's header says the budget ended the run.
    let (_, view, _) = progress_view(&folder);
    let header = talaria_workflow::progress::text::run_line(&view);
    for text in ["budget exhausted", "1 refused", "2 stopped"] {
        assert!(header.contains(text), "{header}");
    }
    assert!(!header.contains("failed"), "{header}");
    let summary = talaria_workflow::runner::runs::discover_runs(&h.runs);
    assert_eq!(summary[0].status.as_str(), "budget_exhausted");
}

#[test]
fn a_script_that_lets_the_budget_error_end_it_ends_on_its_budget() {
    // The script does not catch the budget error, so it has no result; the run still ended
    // because of its budget, not because the script was wrong.
    let h = Harness::new(
        "await agent('hold usage:1000 worker', {label:'worker'}); return 'unreached';",
    );
    let mut req = h.request();
    req.budget_tokens = Some(500);
    let worker = Worker::start(Start::New(req), services(&h.wire, true));
    let folder = run_folder(&worker);
    let (exit, events) = worker.finish();
    assert_eq!(exit.code, 4, "{:?}", exit.problems);
    let finished = events
        .iter()
        .find(|e| e["event"] == "run_finished")
        .unwrap();
    assert_eq!(finished["outcome"], "budget_exhausted");
    let outcome: Value =
        serde_json::from_slice(&fs::read(folder.join("outcome.json")).unwrap()).unwrap();
    assert_eq!(outcome["outcome"], "budget_exhausted");
    assert_eq!(outcome["error"]["name"], "BudgetExhaustedError");
    assert_eq!(
        outcome["reason"],
        "token budget exhausted (spent 1000 of 500)"
    );
    assert!(outcome["result"].is_null(), "{outcome}");
    let summary = talaria_workflow::runner::runs::discover_runs(&h.runs);
    assert_eq!(summary[0].status.as_str(), "budget_exhausted");
}

#[test]
fn token_budget_sends_a_failed_stop_again() {
    // The first interrupt times out. Without a second one the held agent would run on past the
    // budget and the run would never finish.
    let h = Harness::new(
        "return await agent('hold usage:1000 worker', {label:'worker'}).catch(e => e.name);",
    );
    h.wire.fail_interrupts(1);
    let mut req = h.request();
    req.budget_tokens = Some(500);
    let worker = Worker::start(Start::New(req), services(&h.wire, true));
    let folder = run_folder(&worker);
    let (exit, events) = worker.finish();
    assert_eq!(exit.code, 4, "{:?}", exit.problems);
    assert!(
        events.iter().any(|e| e["event"] == "agent_finished"
            && e["label"] == "worker"
            && e["state"] == "stopped"),
        "{events:?}"
    );
    assert_eq!(
        h.wire
            .frames()
            .iter()
            .filter(|f| f.method == "session.interrupt")
            .count(),
        2
    );
    assert!(
        records(&folder)
            .iter()
            .any(|r| r["t"] == "call_finished" && r["outcome"]["kind"] == "stopped")
    );
}

#[test]
fn stop_reports_a_run_the_budget_ended_while_waiting() {
    use crate::support::harness::{line, new_dir, records_path, start_record, write_records};
    use std::io::Write;
    use talaria_workflow::{engine::RunOutcomeKind, journal::Record};
    let (_temp, dir) = new_dir();
    write_records(&dir, &[start_record(&dir)]);
    let path = records_path(&dir);
    // The runner finishes for the budget after the stop request is written.
    let finisher = thread::spawn(move || {
        thread::sleep(Duration::from_millis(300));
        let finished = line(
            2,
            Record::RunFinished {
                outcome: RunOutcomeKind::BudgetExhausted,
                summary: Default::default(),
            },
        );
        let mut file = fs::OpenOptions::new().append(true).open(path).unwrap();
        writeln!(file, "{}", serde_json::to_string(&finished).unwrap()).unwrap();
    });
    let stop = request_stop(
        dir.path(),
        dir.path().to_str().unwrap(),
        Duration::from_secs(5),
    );
    finisher.join().unwrap();
    assert_eq!(
        stop,
        StopExit::AlreadyFinished {
            outcome: "budget_exhausted".into()
        }
    );
}

#[test]
fn budget_exhausted_run_resumes_with_a_larger_budget() {
    let h = Harness::new(
        "const first = await agent('first', {label:'first'}); let second; try { second = await agent('second', {label:'second'}); } catch (e) { second = e.name; } return [first, second];",
    );
    let mut req = h.request();
    req.budget_tokens = Some(10);
    let worker = Worker::start(Start::New(req), services(&h.wire, true));
    let folder = run_folder(&worker);
    let (exit, events) = worker.finish();
    assert_eq!(exit.code, 4, "{:?}", exit.problems);
    assert_eq!(
        events
            .iter()
            .find(|e| e["event"] == "run_finished")
            .unwrap()["result"],
        json!(["READY", "BudgetExhaustedError"])
    );
    let target = folder.file_name().unwrap().to_string_lossy().into_owned();
    let mut resume = h.resume(&target);
    resume.budget_tokens = Some(100);
    let (exit, events) = Worker::start(Start::Resume(resume), services(&h.wire, true)).finish();
    assert_eq!(exit.code, 0, "{:?}", exit.problems);
    let finished = events
        .iter()
        .find(|e| e["event"] == "run_finished")
        .unwrap();
    assert_eq!(finished["outcome"], "completed");
    assert_eq!(finished["result"], json!(["READY", "READY"]));
    // The completed agent is reused; only the refused one reaches Hermes on resume.
    assert_eq!(
        h.wire
            .frames()
            .iter()
            .filter(|f| f.method == "prompt.submit")
            .count(),
        2
    );
}

/// Every file under a run folder with its bytes and modification time.
fn folder_state(dir: &Path) -> BTreeMap<PathBuf, (Vec<u8>, std::time::SystemTime)> {
    let mut files = BTreeMap::new();
    let mut pending = vec![dir.to_path_buf()];
    while let Some(next) = pending.pop() {
        for entry in fs::read_dir(&next).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                pending.push(path);
            } else {
                let modified = fs::metadata(&path).unwrap().modified().unwrap();
                files.insert(
                    path.strip_prefix(dir).unwrap().to_path_buf(),
                    (fs::read(&path).unwrap(), modified),
                );
            }
        }
    }
    files
}

#[test]
fn resume_without_a_budget_flag_keeps_the_run_budget() {
    // The script lets the refusal end the run on its budget, so a resume runs it again.
    let h = Harness::new(
        "const first = await agent('first', {label:'first'}); const second = await agent('second', {label:'second'}); return [first, second];",
    );
    let mut req = h.request();
    req.budget_tokens = Some(10);
    let worker = Worker::start(Start::New(req), services(&h.wire, true));
    let folder = run_folder(&worker);
    let (exit, _) = worker.finish();
    assert_eq!(exit.code, 4, "{:?}", exit.problems);
    let target = folder.file_name().unwrap().to_string_lossy().into_owned();
    let (exit, events) =
        Worker::start(Start::Resume(h.resume(&target)), services(&h.wire, true)).finish();
    let finished = events
        .iter()
        .find(|e| e["event"] == "run_finished")
        .unwrap();
    assert_eq!(finished["outcome"], "budget_exhausted", "{events:?}");
    assert_eq!(exit.code, 4, "{:?}", exit.problems);
    // The refused agent is refused again: nothing new reaches Hermes.
    assert_eq!(
        h.wire
            .frames()
            .iter()
            .filter(|f| f.method == "prompt.submit")
            .count(),
        1
    );
}

#[test]
fn reading_a_finished_run_with_resume_changes_nothing_in_its_folder() {
    let h = Harness::new("return await agent('only', {label:'only'});");
    let worker = Worker::start(Start::New(h.request()), services(&h.wire, true));
    let folder = run_folder(&worker);
    let (exit, _) = worker.finish();
    assert_eq!(exit.code, 0, "{:?}", exit.problems);
    let before = folder_state(&folder);
    let target = folder.file_name().unwrap().to_string_lossy().into_owned();
    let (exit, events) =
        Worker::start(Start::Resume(h.resume(&target)), services(&h.wire, true)).finish();
    assert_eq!(exit.code, 0, "{:?}", exit.problems);
    let finished = events
        .iter()
        .find(|e| e["event"] == "run_finished")
        .unwrap();
    assert_eq!(finished["result"], "READY");
    assert!(
        folder_state(&folder) == before,
        "a completed run's folder changed"
    );

    // A run the budget ended keeps its budget, so reading it starts nothing either.
    let h = Harness::new(
        "const first = await agent('first', {label:'first'}); let second; try { second = await agent('second', {label:'second'}); } catch (e) { second = e.name; } return [first, second];",
    );
    let mut req = h.request();
    req.budget_tokens = Some(10);
    let worker = Worker::start(Start::New(req), services(&h.wire, true));
    let folder = run_folder(&worker);
    let (exit, _) = worker.finish();
    assert_eq!(exit.code, 4, "{:?}", exit.problems);
    let before = folder_state(&folder);
    let frames = h.wire.frames().len();
    let target = folder.file_name().unwrap().to_string_lossy().into_owned();
    let (exit, events) =
        Worker::start(Start::Resume(h.resume(&target)), services(&h.wire, true)).finish();
    assert_eq!(exit.code, 4, "{:?}", exit.problems);
    let finished = events
        .iter()
        .find(|e| e["event"] == "run_finished")
        .unwrap();
    assert_eq!(finished["outcome"], "budget_exhausted");
    assert_eq!(finished["result"], json!(["READY", "BudgetExhaustedError"]));
    assert_eq!(
        h.wire.frames().len(),
        frames,
        "reading the result called Hermes"
    );
    assert!(
        folder_state(&folder) == before,
        "a budget-exhausted run's folder changed"
    );
}

#[test]
fn agent_without_live_usage_is_noted_once() {
    let h = Harness::new(
        "await agent('first quiet', {label:'quiet'}); return await agent('second quiet', {label:'quiet-too'});",
    );
    let mut req = h.request();
    req.budget_tokens = Some(100_000);
    let worker = Worker::start(Start::New(req), services(&h.wire, true));
    let folder = run_folder(&worker);
    let (exit, _) = worker.finish();
    assert_eq!(exit.code, 0, "{:?}", exit.problems);
    let notes: Vec<String> = records(&folder)
        .iter()
        .filter(|r| r["t"] == "engine" && r["event"]["event"] == "log")
        .filter_map(|r| r["event"]["message"].as_str().map(str::to_owned))
        .filter(|m| m.contains("streamed no token usage"))
        .collect();
    assert_eq!(notes.len(), 2, "{notes:?}");
    assert!(notes[0].contains("agent quiet "), "{notes:?}");
    assert!(notes[1].contains("agent quiet-too "), "{notes:?}");

    // Without a token budget the note would only be noise.
    let h = Harness::new("return await agent('quiet', {label:'quiet'});");
    let worker = Worker::start(Start::New(h.request()), services(&h.wire, true));
    let folder = run_folder(&worker);
    assert_eq!(worker.finish().0.code, 0);
    assert!(!records(&folder).iter().any(|r| {
        r["event"]["message"]
            .as_str()
            .is_some_and(|m| m.contains("streamed no token usage"))
    }));
}

#[test]
fn resume_after_script_edit_keeps_latest() {
    let h = Harness::new("return await agent('hold original', {label:'original'});");
    fs::write(
        h.root.join("child.js"),
        script("return 'child from script folder';"),
    )
    .unwrap();
    let worker = Worker::start(Start::New(h.request()), services(&h.wire, true));
    let folder = run_folder(&worker);
    wait_for(|| records(&folder).iter().any(|r| r["t"] == "turn_admitted"));
    worker.kill();
    let edited = h.root.join("edited.js");
    fs::write(&edited, script("await agent('hold edited', {label:'edited'}); return await workflow({scriptPath:'child.js'});")).unwrap();
    let target = folder.file_name().unwrap().to_string_lossy().into_owned();
    let mut resume = h.resume(&target);
    resume.script = Some(edited);
    let worker = Worker::start(Start::Resume(resume), services(&h.wire, true));
    wait_for(|| {
        records(&folder)
            .iter()
            .any(|r| r["t"] == "call_intent" && r["label"] == "edited")
    });
    worker.kill();
    h.wire.release("hold edited");
    let other = h.root.join("other");
    fs::create_dir(&other).unwrap();
    let mut resume = h.resume(&target);
    resume.cwd = Some(other);
    let (exit, events) = Worker::start(Start::Resume(resume), services(&h.wire, true)).finish();
    assert_eq!(exit.code, 0);
    assert_eq!(
        events
            .iter()
            .find(|e| e["event"] == "run_finished")
            .unwrap()["result"],
        "child from script folder"
    );
    let resumes: Vec<_> = records(&folder)
        .into_iter()
        .filter(|r| r["t"] == "run_resumed")
        .collect();
    assert_eq!(resumes.len(), 2);
    assert_eq!(resumes[0]["script_sha256"], resumes[1]["script_sha256"]);
}

fn progress_view(folder: &Path) -> (RunDir, ProgressState, u64) {
    let dir = RunDir::open(folder).unwrap();
    let (lines, offset) = JournalReader::read_from(folder, 0).unwrap();
    let mut view = ProgressState::new(RunProgress::from_records(dir.manifest(), &lines));
    update(&mut view, ProgressInput::Connected(true));
    (dir, view, offset)
}

fn poll_view(dir: &RunDir, view: &mut ProgressState, offset: &mut u64) {
    // The app probes the run lock before it reads the journal, so a runner that finishes
    // between the two reads is never reported as gone.
    let alive = talaria_workflow::runner::runs::runner_alive(dir.path()).unwrap();
    update(view, ProgressInput::Runner { alive });
    let (lines, next) = JournalReader::read_from(dir.path(), *offset).unwrap();
    *offset = next;
    update(view, ProgressInput::Journal(lines));
    update(view, ProgressInput::Controls(RunControl::pending(dir)));
}

fn view_write(effects: Vec<ProgressEffect>, expected: &ControlRequest) -> ControlId {
    match effects.as_slice() {
        [ProgressEffect::WriteControl(id, request)] if request == expected => *id,
        other => panic!("expected {expected:?} from the viewer, got {other:?}"),
    }
}

#[test]
fn progress_restart_waits_for_resume() {
    // An out-of-credit report from the kept agent skips the queued call. A budget would now
    // also stop the running agent that this test stops by hand.
    let h = Harness::new(
        "const [a,b,c]=await parallel([() => agent('hold kept billing',{label:'kept'}), () => agent('hold restart',{label:'retry'}), () => agent('skipped',{label:'skipped'})]); if(b===null) throw Error('retry required'); return [a,b,c];",
    );
    let staffing = fs::read_to_string(&h.staffing)
        .unwrap()
        .replace("max_in_flight = 1", "max_in_flight = 2");
    fs::write(&h.staffing, staffing).unwrap();
    let worker = Worker::start(Start::New(h.request()), services(&h.wire, true));
    let folder = run_folder(&worker);
    wait_for(|| {
        records(&folder)
            .iter()
            .filter(|r| r["t"] == "turn_admitted")
            .count()
            == 2
    });
    let kept = key_for(&folder, "kept");
    let retry = key_for(&folder, "retry");
    h.wire.release("hold kept billing");
    wait_for(|| {
        let entries = records(&folder);
        entries.iter().any(|r| {
            r["t"] == "call_finished" && r["key"] == kept && r["outcome"]["kind"] == "completed"
        }) && entries
            .iter()
            .any(|r| r["t"] == "call_finished" && r["outcome"]["kind"] == "skipped")
    });
    let skipped = records(&folder)
        .into_iter()
        .find(|r| r["t"] == "call_finished" && r["outcome"]["kind"] == "skipped")
        .unwrap()["key"]
        .as_str()
        .unwrap()
        .to_owned();

    // Restart on the running agent: confirmed, then the view notes that it is stopping the
    // agent to restart it, then it sends the stop.
    let (dir, mut view, _) = progress_view(&folder);
    view.select_agent(&retry);
    assert!(update(&mut view, ProgressInput::Key(ProgressKey::RestartAgent)).is_empty());
    let note = view_write(
        update(&mut view, ProgressInput::Key(ProgressKey::Confirm)),
        &ControlRequest::AgentStop {
            key: retry.clone(),
            restart: true,
        },
    );
    let noted = RunControl::note_agent_stop(&dir, &retry, true, 1).unwrap();
    let stop = update(&mut view, ProgressInput::ControlDone(note, noted));
    let (session, generation) = match stop.as_slice() {
        [
            ProgressEffect::Interrupt {
                session_id,
                generation,
            },
        ] => (session_id.clone(), *generation),
        other => panic!("expected the viewer's interrupt, got {other:?}"),
    };
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let reply = runtime.block_on(async {
        let wire = h.wire.open("fixture").await.unwrap();
        let params = json!({"session_id":session,"execution_generation":generation})
            .as_object()
            .unwrap()
            .clone();
        wire.call("session.interrupt", params, Duration::from_secs(1))
            .await
            .unwrap()
    });
    // The view would now ask for the restart; this test asks for it below, after the run.
    view_write(
        update(
            &mut view,
            ProgressInput::GatewayDone {
                key: retry.clone(),
                result: Ok(reply),
            },
        ),
        &ControlRequest::Restart { key: retry.clone() },
    );
    // A stop sent to restart the agent fails its call, as before: it does not wait for an
    // operator, and the journal says the view stopped it to restart it.
    wait_for(|| {
        records(&folder).iter().any(|r| {
            r["t"] == "call_finished" && r["key"] == retry && r["outcome"]["kind"] == "failed"
        })
    });
    let (exit, _) = worker.finish();
    assert_eq!(exit.code, 1);
    assert!(records(&folder).iter().any(|r| r["t"] == "turn_settled"
        && r["key"] == retry
        && r["result_meta"]["stopped_by"] == "workflow_view_restart"));

    let (_, mut view, _) = progress_view(&folder);
    assert_eq!(
        view.progress().agent(&retry).unwrap().state,
        AgentState::Failed
    );
    for key in [&kept, &skipped] {
        view.select_agent(key);
        assert!(update(&mut view, ProgressInput::Key(ProgressKey::RestartAgent)).is_empty());
        assert_eq!(
            view.footer.as_deref(),
            Some("finished agents are kept — restart would pay for the work twice")
        );
    }
    view.select_agent(&retry);
    assert!(update(&mut view, ProgressInput::Key(ProgressKey::RestartAgent)).is_empty());
    let id = view_write(
        update(&mut view, ProgressInput::Key(ProgressKey::Confirm)),
        &ControlRequest::Restart { key: retry.clone() },
    );
    let at = i64::try_from(now_ms()).unwrap();
    let outcome = RunControl::request_restart(&dir, &retry, at).unwrap();
    assert_eq!(outcome, ControlOutcome::Published);
    update(&mut view, ProgressInput::ControlDone(id, outcome));
    let restart_file = folder.join("control").join(format!("restart-{retry}.json"));
    let expected = format!(r#"{{"v":1,"kind":"restart","key":"{retry}","at_ms":{at}}}"#);
    assert_eq!(fs::read(&restart_file).unwrap(), expected.as_bytes());
    let before = h.wire.frames();
    let journal_before = fs::read(folder.join("journal.jsonl")).unwrap();
    thread::sleep(Duration::from_millis(350));
    assert!(restart_file.exists());
    assert_eq!(
        fs::read(folder.join("journal.jsonl")).unwrap(),
        journal_before
    );
    assert_eq!(h.wire.frames().len(), before.len());

    h.wire.release("hold restart");
    let target = folder.file_name().unwrap().to_string_lossy().into_owned();
    let (exit, _) =
        Worker::start(Start::Resume(h.resume(&target)), services(&h.wire, true)).finish();
    assert_eq!(exit.code, 0);
    assert!(!restart_file.exists());
    let after = h.wire.frames();
    for method in ["session.create", "prompt.submit"] {
        assert_eq!(
            count_for(&after, method, &kept),
            count_for(&before, method, &kept)
        );
        assert_eq!(count_for(&after, method, &skipped), 0);
        assert_eq!(
            count_for(&after, method, &retry),
            count_for(&before, method, &retry) + 1
        );
    }
    let attempts: Vec<_> = records(&folder)
        .into_iter()
        .filter(|r| r["t"] == "call_intent" && r["key"] == retry)
        .map(|r| r["attempt"].as_u64().unwrap())
        .collect();
    assert_eq!(attempts, [0, 1]);
}

#[test]
fn a_stop_from_the_view_parks_the_agent_until_the_operator_answers() {
    let h = Harness::new("return await agent('hold stopped',{label:'stopped'});");
    let worker = Worker::start(Start::New(h.request()), services(&h.wire, true));
    let folder = run_folder(&worker);
    wait_for(|| records(&folder).iter().any(|r| r["t"] == "turn_admitted"));
    let key = key_for(&folder, "stopped");
    let (dir, mut view, _) = progress_view(&folder);
    view.select_agent(&key);
    // The stop-agent key asks first, naming the agent.
    assert!(update(&mut view, ProgressInput::Key(ProgressKey::StopAgent)).is_empty());
    assert_eq!(
        view.confirmation_text(),
        Some("stop agent stopped? x or y confirms · esc cancels")
    );
    let note = view_write(
        update(&mut view, ProgressInput::Key(ProgressKey::StopAgent)),
        &ControlRequest::AgentStop {
            key: key.clone(),
            restart: false,
        },
    );
    let noted = RunControl::note_agent_stop(&dir, &key, false, 1).unwrap();
    let (session, generation) =
        match update(&mut view, ProgressInput::ControlDone(note, noted)).as_slice() {
            [
                ProgressEffect::Interrupt {
                    session_id,
                    generation,
                },
            ] => (session_id.clone(), *generation),
            other => panic!("expected the viewer's interrupt, got {other:?}"),
        };
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    runtime.block_on(async {
        let wire = h.wire.open("fixture").await.unwrap();
        let params = json!({"session_id":session,"execution_generation":generation})
            .as_object()
            .unwrap()
            .clone();
        wire.call("session.interrupt", params, Duration::from_secs(1))
            .await
            .unwrap()
    });
    wait_for(|| worker.events().iter().any(|e| e["event"] == "parked"));
    let parked = worker
        .events()
        .into_iter()
        .find(|e| e["event"] == "parked")
        .unwrap();
    assert_eq!(parked["cause"], "stopped");
    assert_eq!(
        parked["choices"],
        json!(["fail", "rerun", "abort", "use-reply"])
    );
    assert!(
        parked["reason"].as_str().unwrap().starts_with(
            "agent stopped: it was stopped with the stop-agent key in the Talaria workflow view"
        ),
        "{parked}"
    );
    // Parked means waiting: nothing ends the run until it is answered.
    thread::sleep(Duration::from_millis(400));
    assert!(!worker.events().iter().any(|e| e["event"] == "run_finished"));
    let run_id = folder.file_name().unwrap().to_string_lossy().into_owned();
    answer_parked(
        &h.runs,
        &run_id,
        &key,
        parked["attempt"].as_u64().unwrap() as u32,
        parked["episode"].as_u64().unwrap() as u32,
        Resolution::Fail,
    )
    .unwrap();
    let (exit, _) = worker.finish();
    assert_eq!(exit.code, 5, "the script returned and the agent failed");
}

#[test]
fn progress_control_files_match_view() {
    for unconfirmed in [false, true] {
        let h = Harness::new(
            "return await parallel([() => agent('hold first',{label:'first'}), () => agent('hold second',{label:'second'})]);",
        );
        if unconfirmed {
            h.wire.unconfirmed();
        }
        let worker = Worker::start(Start::New(h.request()), services(&h.wire, true));
        let folder = run_folder(&worker);
        wait_for(|| records(&folder).iter().any(|r| r["t"] == "turn_admitted"));
        let (dir, mut view, mut offset) = progress_view(&folder);
        let at = i64::try_from(now_ms()).unwrap();
        update(&mut view, ProgressInput::Clock { now_ms: at });
        let pause = view_write(
            update(&mut view, ProgressInput::Key(ProgressKey::PauseRun)),
            &ControlRequest::Pause,
        );
        let result = RunControl::request_pause(&dir, at).unwrap();
        assert_eq!(result, ControlOutcome::Published);
        update(&mut view, ProgressInput::ControlDone(pause, result));
        assert_eq!(
            fs::read(folder.join("control/pause.json")).unwrap(),
            format!(r#"{{"v":1,"kind":"pause","at_ms":{at}}}"#).as_bytes()
        );
        assert_eq!(view.run_suffix.as_deref(), Some("pausing"));
        thread::sleep(Duration::from_millis(350));
        h.wire.release("hold first");
        wait_for(|| records(&folder).iter().any(|r| r["t"] == "call_finished"));
        thread::sleep(Duration::from_millis(350));
        assert!(
            records(&folder)
                .iter()
                .any(|r| r["t"] == "call_intent" && r["label"] == "second")
        );
        assert_eq!(
            h.wire
                .frames()
                .iter()
                .filter(|f| f.method == "prompt.submit")
                .count(),
            1
        );
        update(&mut view, ProgressInput::Clock { now_ms: at + 500 });
        poll_view(&dir, &mut view, &mut offset);
        update(&mut view, ProgressInput::Clock { now_ms: at + 1_100 });
        update(&mut view, ProgressInput::Tick);
        assert_eq!(view.run_suffix.as_deref(), Some("paused"));
        let resume = view_write(
            update(&mut view, ProgressInput::Key(ProgressKey::PauseRun)),
            &ControlRequest::Resume,
        );
        let result = RunControl::request_resume(&dir).unwrap();
        assert_eq!(result, ControlOutcome::Removed);
        update(&mut view, ProgressInput::ControlDone(resume, result));
        assert!(!folder.join("control/pause.json").exists());
        wait_for(|| {
            records(&folder)
                .iter()
                .any(|r| r["t"] == "call_intent" && r["label"] == "second")
        });
        wait_for(|| {
            records(&folder)
                .iter()
                .filter(|r| r["t"] == "turn_admitted")
                .count()
                == 2
        });

        assert!(!records(&folder).iter().any(|r| r["t"] == "run_finished"));
        h.wire.hold_stops();
        assert!(update(&mut view, ProgressInput::Key(ProgressKey::StopRun)).is_empty());
        let stop = view_write(
            update(&mut view, ProgressInput::Key(ProgressKey::Confirm)),
            &ControlRequest::Stop,
        );
        let stop_at = at + 1_100;
        let result = RunControl::request_stop(&dir, stop_at).unwrap();
        assert_eq!(result, ControlOutcome::Published);
        assert_eq!(
            fs::read(folder.join("control/stop.json")).unwrap(),
            format!(r#"{{"v":1,"kind":"stop","at_ms":{stop_at}}}"#).as_bytes()
        );
        update(&mut view, ProgressInput::ControlDone(stop, result));
        assert_eq!(view.run_suffix.as_deref(), Some("stopping"));
        assert_eq!(view.progress().run().state, RunState::Running);
        h.wire.release_stops();
        let (exit, _) = worker.finish();
        assert_eq!(exit.code, 3);
        let outcome: Value =
            serde_json::from_slice(&fs::read(folder.join("outcome.json")).unwrap()).unwrap();
        assert_eq!(outcome["outcome"], "aborted");
        assert_eq!(
            outcome["reason"],
            if unconfirmed {
                "stopped-unconfirmed"
            } else {
                "stopped"
            }
        );
        assert!(
            records(&folder)
                .iter()
                .any(|r| r["t"] == "run_finished" && r["outcome"] == "aborted")
        );
        if !unconfirmed {
            assert_eq!(
                records(&folder)
                    .iter()
                    .filter(|r| r["t"] == "turn_settled")
                    .count(),
                2
            );
        }
        assert!(!folder.join("control/stop.json").exists());
        assert!(
            h.wire
                .frames()
                .iter()
                .any(|f| f.method == "session.interrupt")
        );

        let stale_at = stop_at + 5_000;
        assert_eq!(
            RunControl::request_pause(&dir, stale_at).unwrap(),
            ControlOutcome::Published
        );
        assert_eq!(
            RunControl::request_stop(&dir, stale_at).unwrap(),
            ControlOutcome::Published
        );
        let key = key_for(&folder, "second");
        assert_eq!(
            RunControl::request_restart(&dir, &key, stale_at).unwrap(),
            ControlOutcome::Published
        );
        update(
            &mut view,
            ProgressInput::Controls(RunControl::pending(&dir)),
        );
        // The app probes the lock on every poll; the runner has exited and released it.
        update(
            &mut view,
            ProgressInput::Runner {
                alive: talaria_workflow::runner::runs::runner_alive(dir.path()).unwrap(),
            },
        );
        update(
            &mut view,
            ProgressInput::Clock {
                now_ms: stale_at + 10_000,
            },
        );
        update(&mut view, ProgressInput::Tick);
        assert!(
            view.footer
                .as_deref()
                .is_some_and(|s| s.contains("no runner answered"))
        );
        assert_ne!(view.run_suffix.as_deref(), Some("paused"));
        assert_eq!(view.progress().run().state, RunState::Running);
        assert!(
            view.pending_controls
                .restarts
                .iter()
                .any(|r| r.key.as_deref() == Some(&key))
        );
        assert!(
            view.pending_controls
                .warnings
                .iter()
                .all(|s| !s.contains(&key))
        );
    }
}

#[test]
fn a_replay_says_its_money_was_not_recorded_when_a_turn_had_no_hermes_cost() {
    // The live run can price a turn from the price list, but only Hermes's own costs are journaled,
    // so the replay cannot know that money and must not claim there was no price.
    let h = Harness::new("return await agent('unpriced review', {label:'review'});");
    let (first, events) = Worker::start(Start::New(h.request()), services(&h.wire, true)).finish();
    assert_eq!(first.code, 0);
    let live: Vec<_> = events
        .iter()
        .filter(|e| e["event"] == "run_finished")
        .collect();
    assert_eq!(live[0]["spend"]["money_source"], "none");
    let target = first.run_id.unwrap();
    let (exit, events) =
        Worker::start(Start::Resume(h.resume(&target)), services(&h.wire, false)).finish();
    assert_eq!(exit.code, 0);
    let replay: Vec<_> = events
        .iter()
        .filter(|e| e["event"] == "run_finished")
        .collect();
    assert_eq!(replay[0]["spend"]["money_usd"], Value::Null);
    assert_eq!(replay[0]["spend"]["money_source"], "unrecorded");
}

#[test]
fn completed_replay_initializes_recording_without_gateway_frames() {
    let h = Harness::new("return await agent('review', {label:'review'});");
    let (first, _) = Worker::start(Start::New(h.request()), services(&h.wire, true)).finish();
    assert_eq!(first.code, 0);
    let frames = h.wire.frames().len();
    let target = first.run_id.unwrap();
    let record = h.root.join("replay.jsonl");
    let mut req = h.resume(&target);
    req.record = Some(record.clone());
    let (exit, events) = Worker::start(Start::Resume(req), services(&h.wire, false)).finish();
    assert_eq!(exit.code, 0);
    assert!(events.iter().any(|e| e["event"] == "run_finished"));
    let bytes = fs::read(&record).expect("completed replay must create its recording");
    let rows: Vec<Value> = String::from_utf8(bytes.clone())
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0]["kind"], "header");
    assert_eq!(h.wire.frames().len(), frames);
    let mut req = h.resume(&target);
    req.record = Some(record.clone());
    let (exit, _) = Worker::start(Start::Resume(req), services(&h.wire, false)).finish();
    assert_eq!(exit.code, 1, "replay must refuse to overwrite a recording");
    assert_eq!(fs::read(record).unwrap(), bytes);
}

#[test]
fn parallel_waiters_are_queued_and_cancelled_by_stop() {
    let h = Harness::new(
        "return await parallel([() => agent('hold first',{label:'first'}), () => agent('second',{label:'second'}), () => agent('third',{label:'third'}), () => agent('fourth',{label:'fourth'})]);",
    );
    let worker = Worker::start(Start::New(h.request()), services(&h.wire, true));
    let folder = run_folder(&worker);
    wait_for(|| {
        records(&folder)
            .iter()
            .filter(|r| r["t"] == "call_intent")
            .count()
            == 4
    });
    wait_for(|| records(&folder).iter().any(|r| r["t"] == "turn_admitted"));
    let (_, view, _) = progress_view(&folder);
    assert_eq!(
        view.progress
            .agents
            .iter()
            .filter(|a| a.state == AgentState::Queued)
            .count(),
        3
    );
    assert_eq!(
        view.progress
            .agents
            .iter()
            .filter(|a| a.state == AgentState::Running)
            .count(),
        1
    );
    let queued: Vec<_> = view
        .progress
        .agents
        .iter()
        .filter(|a| a.state == AgentState::Queued)
        .map(|a| a.key.clone())
        .collect();
    let stop = request_stop(&h.runs, folder.to_str().unwrap(), Duration::from_secs(5));
    assert_eq!(stop, StopExit::Stopped);
    let (exit, _) = worker.finish();
    assert_eq!(exit.code, 3);
    let rows = records(&folder);
    for key in queued {
        assert_eq!(
            rows.iter()
                .filter(|r| r["t"] == "call_intent" && r["key"] == key)
                .count(),
            1
        );
        assert!(rows.iter().any(|r| r["t"] == "call_finished"
            && r["key"] == key
            && r["outcome"]["kind"] == "failed"
            && r["outcome"]["reason"] == "cancelled before admission"));
        assert_eq!(count_for(&h.wire.frames(), "session.create", &key), 0);
        assert_eq!(count_for(&h.wire.frames(), "prompt.submit", &key), 0);
    }
    let saved: Value =
        serde_json::from_slice(&fs::read(folder.join("outcome.json")).unwrap()).unwrap();
    assert_eq!(saved["reason"], "stopped");
}

#[test]
fn paused_runner_journals_the_gate_once_and_its_lock_keeps_the_view_answered() {
    let h = Harness::new(
        "await agent('hold first',{label:'first'}); return await parallel([() => agent('second',{label:'second'}), () => agent('third',{label:'third'}), () => agent('fourth',{label:'fourth'})]);",
    );
    let worker = Worker::start(Start::New(h.request()), services(&h.wire, true));
    let folder = run_folder(&worker);
    wait_for(|| records(&folder).iter().any(|r| r["t"] == "turn_admitted"));
    let (dir, mut view, mut offset) = progress_view(&folder);
    write_pause(&folder);
    let start = Instant::now();
    let until = start + Duration::from_secs(12);
    let mut released = false;
    while Instant::now() < until {
        if !released && start.elapsed() >= Duration::from_millis(400) {
            h.wire.release("hold first");
            released = true;
            wait_for(|| {
                records(&folder)
                    .iter()
                    .filter(|r| r["t"] == "call_intent")
                    .count()
                    == 4
            });
        }
        update(
            &mut view,
            ProgressInput::Clock {
                now_ms: now_ms() as i64,
            },
        );
        poll_view(&dir, &mut view, &mut offset);
        update(&mut view, ProgressInput::Tick);
        assert!(
            !view
                .footer
                .as_deref()
                .is_some_and(|f| f.contains("no runner answered"))
        );
        thread::sleep(Duration::from_millis(250));
    }
    assert_eq!(view.run_suffix.as_deref(), Some("paused"));
    assert_eq!(
        view.progress
            .agents
            .iter()
            .filter(|a| a.state == AgentState::Queued)
            .count(),
        3
    );
    // The gate is journaled when it changes, not every second, so script logs stay visible.
    let gate_lines: Vec<_> = records(&folder)
        .into_iter()
        .filter(|r| {
            r["t"] == "engine"
                && r["event"]["message"]
                    .as_str()
                    .is_some_and(|m| m.starts_with("runner: admission gate"))
        })
        .collect();
    assert_eq!(gate_lines.len(), 1, "{gate_lines:?}");
    assert_eq!(
        gate_lines[0]["event"]["message"],
        "runner: admission gate paused"
    );
    assert_eq!(
        h.wire
            .frames()
            .iter()
            .filter(|f| f.method == "prompt.submit")
            .count(),
        1
    );
    fs::remove_file(folder.join("control/pause.json")).unwrap();
    h.wire.release("hold first");
    assert_eq!(worker.finish().0.code, 0);
    let rows = records(&folder);
    assert_eq!(rows.iter().filter(|r| r["t"] == "call_intent").count(), 4);
}

fn init_repository(path: &Path) {
    fs::create_dir_all(path).unwrap();
    let run = |args: &[&str]| {
        let out = std::process::Command::new("git")
            .arg("-C")
            .arg(path)
            .args(args)
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "git fixture setup failed: {}",
            String::from_utf8_lossy(&out.stderr)
        );
    };
    run(&["init", "-b", "main"]);
    fs::write(path.join("tracked"), "initial").unwrap();
    run(&["add", "tracked"]);
    run(&[
        "-c",
        "user.name=fixture",
        "-c",
        "user.email=fixture",
        "commit",
        "-m",
        "fixture",
    ]);
}
fn common_dir(path: &Path) -> PathBuf {
    let out = std::process::Command::new("git")
        .arg("-C")
        .arg(path)
        .args(["rev-parse", "--path-format=absolute", "--git-common-dir"])
        .output()
        .unwrap();
    assert!(out.status.success());
    PathBuf::from(String::from_utf8(out.stdout).unwrap().trim())
        .canonicalize()
        .unwrap()
}
#[test]
fn runner_installs_worktree_provider() {
    let h = Harness::new(
        "const a=await agent('first',{label:'first',tier:'first',toolsets:['file'],isolation:'worktree'}); const b=await agent('hold second',{label:'second',tier:'second',toolsets:['file'],isolation:'worktree'}); const c=await agent('third',{label:'third',tier:'first',toolsets:['file'],isolation:'worktree'}); return [a,b,c];",
    );
    let repo = h.root.join("source-repo");
    let other = h.root.join("other-repo");
    init_repository(&repo);
    init_repository(&other);
    let mut request = h.request();
    request.cwd = repo.clone().canonicalize().unwrap();
    let worker = Worker::start(Start::New(request), services(&h.wire, true));
    let folder = run_folder(&worker);
    wait_for(|| {
        h.wire.frames().iter().any(|f| f.method == "session.create")
            || worker.events().iter().any(|e| e["event"] == "run_finished")
    });
    let initial = h.wire.frames();
    let first = initial
        .iter()
        .find(|f| f.method == "session.create")
        .expect("new run did not create isolated session; worktree provider missing");
    let first_cwd = PathBuf::from(first.params["cwd"].as_str().expect("session cwd"));
    assert!(first_cwd.starts_with(folder.join("worktrees").canonicalize().unwrap()));
    assert_eq!(common_dir(&first_cwd), common_dir(&repo));
    assert_ne!(first_cwd, repo);
    wait_for(|| {
        let rows = records(&folder);
        rows.iter()
            .find(|r| r["t"] == "call_intent" && r["label"] == "second")
            .is_some_and(|intent| {
                rows.iter()
                    .any(|r| r["t"] == "turn_admitted" && r["key"] == intent["key"])
            })
    });
    worker.kill();
    h.wire.release("hold second");
    let run_id = folder.file_name().unwrap().to_string_lossy().into_owned();
    let mut resume = h.resume(&run_id);
    resume.cwd = Some(other.clone().canonicalize().unwrap());
    let before = h.wire.frames().len();
    let (exit, _) = Worker::start(Start::Resume(resume), services(&h.wire, true)).finish();
    assert_eq!(exit.code, 0, "resume failed: {exit:?}");
    let after = h.wire.frames();
    let third = after[before..]
        .iter()
        .find(|f| f.method == "session.create")
        .expect("resume did not create third isolated session");
    let resumed_cwd = PathBuf::from(third.params["cwd"].as_str().expect("resumed session cwd"));
    assert!(resumed_cwd.starts_with(folder.join("worktrees").canonicalize().unwrap()));
    assert_eq!(common_dir(&resumed_cwd), common_dir(&repo));
    assert_ne!(common_dir(&resumed_cwd), common_dir(&other));
}

#[test]
fn a_parked_isolated_agent_has_its_uncommitted_work_saved_and_named_when_it_parks() {
    let h = Harness::new(
        "return await agent('hold stopped', {label:'stopped', tier:'first', toolsets:['file'], isolation:'worktree'});",
    );
    let repo = h.root.join("source-repo");
    init_repository(&repo);
    let mut request = h.request();
    request.cwd = repo.canonicalize().unwrap();
    let worker = Worker::start(Start::New(request), services(&h.wire, true));
    let folder = run_folder(&worker);
    wait_for(|| records(&folder).iter().any(|r| r["t"] == "turn_admitted"));
    let worktree = PathBuf::from(
        h.wire
            .frames()
            .iter()
            .find(|f| f.method == "session.create")
            .unwrap()
            .params["cwd"]
            .as_str()
            .unwrap(),
    );
    fs::write(worktree.join("half.txt"), "half done").unwrap();
    // The agent is stopped from the workflow view, so it parks for the operator.
    let key = key_for(&folder, "stopped");
    let (dir, mut view, _) = progress_view(&folder);
    view.select_agent(&key);
    assert!(update(&mut view, ProgressInput::Key(ProgressKey::StopAgent)).is_empty());
    let note = view_write(
        update(&mut view, ProgressInput::Key(ProgressKey::StopAgent)),
        &ControlRequest::AgentStop {
            key: key.clone(),
            restart: false,
        },
    );
    let noted = RunControl::note_agent_stop(&dir, &key, false, 1).unwrap();
    let (session, generation) =
        match update(&mut view, ProgressInput::ControlDone(note, noted)).as_slice() {
            [
                ProgressEffect::Interrupt {
                    session_id,
                    generation,
                },
            ] => (session_id.clone(), *generation),
            other => panic!("expected the viewer's interrupt, got {other:?}"),
        };
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    runtime.block_on(async {
        let wire = h.wire.open("fixture").await.unwrap();
        let params = json!({"session_id":session,"execution_generation":generation})
            .as_object()
            .unwrap()
            .clone();
        wire.call("session.interrupt", params, Duration::from_secs(1))
            .await
            .unwrap()
    });
    wait_for(|| worker.events().iter().any(|e| e["event"] == "parked"));
    let parked = worker
        .events()
        .into_iter()
        .find(|e| e["event"] == "parked")
        .unwrap();
    // A rerun would start the next attempt in this worktree, so the work is saved first and
    // the parked line names the branch.
    let branch = parked["saved_branch"]
        .as_str()
        .unwrap_or_else(|| panic!("{parked}"))
        .to_owned();
    assert!(
        branch.starts_with("talaria/wf/") && branch.ends_with("-0-saved"),
        "{branch}"
    );
    assert!(
        parked["reason"]
            .as_str()
            .unwrap()
            .ends_with(&format!("(uncommitted work saved to branch {branch})")),
        "{parked}"
    );
    let shown = std::process::Command::new("git")
        .arg("-C")
        .arg(&repo)
        .args(["show", &format!("{branch}:half.txt")])
        .output()
        .unwrap();
    assert_eq!(String::from_utf8_lossy(&shown.stdout), "half done");
    let run_id = folder.file_name().unwrap().to_string_lossy().into_owned();
    answer_parked(
        &h.runs,
        &run_id,
        &key,
        parked["attempt"].as_u64().unwrap() as u32,
        parked["episode"].as_u64().unwrap() as u32,
        Resolution::Fail,
    )
    .unwrap();
    let (exit, _) = worker.finish();
    assert_eq!(exit.code, 5);
    // The failed call names the same branch once: the second save found the same work.
    let finish = records(&folder)
        .into_iter()
        .rfind(|r| r["t"] == "call_finished")
        .unwrap();
    let reason = finish["outcome"]["reason"].as_str().unwrap();
    assert_eq!(reason.matches(&branch).count(), 1, "{reason}");
}
