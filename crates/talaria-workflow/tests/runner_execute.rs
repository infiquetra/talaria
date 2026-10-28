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
    hermes::{PortConfig, Wire, WireError, WireEvent, WireFactory, wire::Cursor},
    journal::{JournalReader, OnUnknown, Resolution, RunDir},
    progress::{
        AgentState, ControlId, ControlOutcome, ControlRequest, ProgressEffect, RunControl,
        RunProgress, RunState,
        live::{ProgressInput, ProgressKey, ProgressState, update},
    },
    runner::{
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
fn snapshot(session_id: &str, session: &Session) -> Value {
    json!({"session_id":session_id,"stored_session_id":session_id,"messages":[],"message_count":0,
        "running":false,"authority_epoch":1,"replay_epoch":"epoch","last_sequence":0,
        "subscription_id":"subscription","revision":1,"execution_generation":1,"pending":[],"prompts":[],
        "info":{"desktop_protocol":"hermes-gateway-v1","source":"tool","model":session.model,"provider":session.provider}})
}
fn receipt(session_id: &str, session: &Session, status: &str, include_result: bool) -> Value {
    let terminal = status == "terminal";
    let outcome = if session.unknown_resolved {
        "interrupted"
    } else if session.stopped {
        "cancelled"
    } else {
        "completed"
    };
    let mut value = json!({"admission_id":session.admission,"ref":{"profile_id":"fixture","session_id":session_id},
        "sequence":1,"status":status,"outcome":if terminal {Some(outcome)} else {None},"authority_epoch":1,"execution_generation":1});
    if terminal && include_result && !session.unknown_resolved {
        value["result"] = json!({"final_response":"READY","completed":!session.stopped,"failed":session.stopped,
            "turn_exit_reason":outcome,"estimated_cost_usd":0.2,"cost_status":"estimated","cost_source":"fixture"});
        value["usage"] = json!({"input_tokens":8,"output_tokens":2,"total_tokens":10});
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
        Box::pin(async move {
            while matches!(method, "session.interrupt" | "prompt.cancel")
                && hold_stop.load(Ordering::SeqCst)
            {
                tokio::time::sleep(Duration::from_millis(10)).await;
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
                    snapshot(&session_id, &state.sessions[&session_id])
                }
                "session.resume" => {
                    let id = params["session_id"].as_str().unwrap();
                    snapshot(id, &state.sessions[id])
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
                    } else if session.prompt.contains("hold")
                        && !state.released.contains(&session.prompt)
                    {
                        &session.status
                    } else {
                        "terminal"
                    };
                    receipt(id, session, status, params["include_result"] == true)
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
                _ => return Err(WireError::Other(format!("unexpected method: {method}"))),
            };
            Ok(result)
        })
    }
    fn generation(&self) -> u64 {
        1
    }
    fn track(&self, _session_id: &str, _cursor: Cursor) {}
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
            staffing: Some(self.staffing.clone()),
            cwd: self.root.clone(),
            budget_tokens: None,
            budget_usd: None,
            on_unknown: OnUnknown::Wait,
            runs_dir: self.runs.clone(),
            workflows_dir: self.workflows.clone(),
            talaria_version: "test-version".into(),
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
        } => {
            json!({"event":"parked","run_id":run_id,"label":label,"key":key,"attempt":attempt,"episode":episode})
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
    assert_eq!(exit.code, 0);
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
    assert_eq!(exit.code, 0);
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
    let h = Harness::new(
        "const [a,b,c]=await parallel([() => agent('hold kept',{label:'kept'}), () => agent('hold restart',{label:'retry'}), () => agent('skipped',{label:'skipped'})]); if(b===null) throw Error('retry required'); return [a,b,c];",
    );
    let staffing = fs::read_to_string(&h.staffing)
        .unwrap()
        .replace("max_in_flight = 1", "max_in_flight = 2");
    fs::write(&h.staffing, staffing).unwrap();
    let mut request = h.request();
    request.budget_tokens = Some(10);
    let worker = Worker::start(Start::New(request), services(&h.wire, true));
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
    h.wire.release("hold kept");
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

    let (dir, mut view, _) = progress_view(&folder);
    view.select_agent(&retry);
    let stop = update(&mut view, ProgressInput::Key(ProgressKey::StopAgent));
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
    update(
        &mut view,
        ProgressInput::GatewayDone {
            key: retry.clone(),
            result: Ok(reply),
        },
    );
    wait_for(|| {
        records(&folder).iter().any(|r| {
            r["t"] == "call_finished" && r["key"] == retry && r["outcome"]["kind"] == "failed"
        })
    });
    let (exit, _) = worker.finish();
    assert_eq!(exit.code, 1);

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
    let mut request = h.resume(&target);
    request.budget_tokens = Some(100);
    let (exit, _) = Worker::start(Start::Resume(request), services(&h.wire, true)).finish();
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
fn paused_runner_heartbeats_keep_the_view_answered() {
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
    let heartbeats: Vec<_> = records(&folder)
        .into_iter()
        .filter(|r| r["t"] == "engine" && r["event"]["message"] == "runner: admission gate paused")
        .collect();
    assert!(heartbeats.len() >= 5, "{heartbeats:?}");
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
