use super::engine::*;
use super::support::{LiveEnv, live_preconditions, report_skip};
use futures_util::future::join_all;
use hermes_client::{
    CallError, CallOptions, Client, Inbound, Purpose, Scope, connect, handshake, mint_ticket,
};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;
use std::time::{Duration, Instant};
use tokio::sync::mpsc;

pub fn results_path() -> PathBuf {
    PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("gateway-concurrency/results.json")
}
pub fn execution_mode() -> String {
    let mode = std::env::var("TALARIA_PROBE_EXECUTION_MODE").unwrap_or_else(|_| "unknown".into());
    assert!(
        ["in-process", "managed-worker", "unknown"].contains(&mode.as_str()),
        "invalid execution mode"
    );
    mode
}
fn run_identity() -> Value {
    let output = std::process::Command::new("git")
        .args(["rev-parse", "--short=12", "HEAD"])
        .output()
        .expect("commit identity");
    let commit = String::from_utf8(output.stdout)
        .expect("commit encoding")
        .trim()
        .to_owned();
    let date = time::OffsetDateTime::now_utc().date().to_string();
    json!({"run_id":format!("{:08x}",fastrand::u32(..)),"talaria_commit":commit,"date":date})
}
enum Signal {
    Event(Value, Instant),
    Sample(Option<ProcessTree>, Instant),
    Foreground(Result<u64, ()>),
}
struct Link {
    client: Client,
    reader: tokio::task::JoinHandle<()>,
}
impl Drop for Link {
    fn drop(&mut self) {
        self.reader.abort();
    }
}
async fn attach(env: &LiveEnv, tx: mpsc::UnboundedSender<Signal>) -> Result<Link, &'static str> {
    let ticket = mint_ticket(
        &env.home,
        &env.endpoint,
        &env.fixture,
        Purpose::Interactive,
        Scope::profile(),
    )
    .await
    .map_err(|_| "ticket failed")?;
    let mut connection = connect(&env.endpoint, ticket)
        .await
        .map_err(|_| "connect failed")?;
    handshake(&mut connection)
        .await
        .map_err(|_| "handshake failed")?;
    let (client, mut inbound) = Client::start(connection.into_parts());
    let reader = tokio::spawn(async move {
        while let Some(frame) = inbound.next().await {
            let at = Instant::now();
            match frame {
                Inbound::Event(event) => {
                    let _ = tx.send(Signal::Event(event.raw, at));
                }
                Inbound::ServerRequest(request) => {
                    let _ = request
                        .responder
                        .fail(-32601, "probe cannot answer server requests")
                        .await;
                }
                Inbound::Closed(_) => break,
                _ => {}
            }
        }
    });
    Ok(Link { client, reader })
}
async fn rpc(client: &Client, method: &str, params: Value) -> (Result<Value, CallError>, u64) {
    let start = Instant::now();
    let result = client
        .call_raw(
            method,
            params,
            CallOptions {
                timeout: Some(Duration::from_secs(30)),
            },
        )
        .await;
    (result, start.elapsed().as_millis() as u64)
}
fn code(error: &CallError) -> String {
    match error {
        CallError::Rpc(e) => e
            .reason
            .as_deref()
            .map(sanitise)
            .unwrap_or_else(|| "none".into()),
        CallError::TimedOut { .. } => "timeout".into(),
        CallError::Unknown { .. } => "unknown".into(),
        _ => "transport".into(),
    }
}
fn sanitise(s: &str) -> String {
    let out: String = s
        .chars()
        .filter(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || *c == '_' || *c == '.')
        .take(40)
        .collect();
    if out.is_empty() {
        "unknown".into()
    } else {
        out
    }
}
fn count_error(record: &mut Value, method: &str, error: &CallError) {
    if matches!(error, CallError::TimedOut { .. }) {
        increment(record, "timeouts");
    }
    let key = format!("{method}:{}", code(error));
    let n = record["rpc_errors"][&key].as_u64().unwrap_or(0);
    record["rpc_errors"][key] = json!(n + 1);
}
fn increment(record: &mut Value, key: &str) {
    record[key] = json!(record[key].as_u64().unwrap_or(0) + 1);
}
#[derive(Clone)]
struct Session {
    id: String,
    subscription: String,
    connection: usize,
    admission: String,
}
struct Measure {
    origin: Instant,
    tracker: EventTracker,
    clock: TurnClock,
    foreground: Vec<u64>,
    foreground_errors: u64,
    samples: Vec<(Instant, ProcessTree)>,
    sampler_failed: bool,
    completed: u64,
    failed: u64,
    tools: BTreeMap<String, (u64, u64)>,
    rate_keys: BTreeSet<String>,
    failures: Vec<Value>,
}
impl Measure {
    fn new() -> Self {
        Self {
            origin: Instant::now(),
            tracker: EventTracker::default(),
            clock: TurnClock::default(),
            foreground: Vec::new(),
            foreground_errors: 0,
            samples: Vec::new(),
            sampler_failed: false,
            completed: 0,
            failed: 0,
            tools: BTreeMap::new(),
            rate_keys: BTreeSet::new(),
            failures: Vec::new(),
        }
    }
    fn ms(&self, at: Instant) -> u64 {
        at.saturating_duration_since(self.origin).as_millis() as u64
    }
    fn take(&mut self, signal: Signal) {
        match signal {
            Signal::Foreground(Ok(ms)) => self.foreground.push(ms),
            Signal::Foreground(Err(())) => self.foreground_errors += 1,
            Signal::Sample(Some(p), at) => self.samples.push((at, p)),
            Signal::Sample(None, _) => self.sampler_failed = true,
            Signal::Event(p, at) => {
                self.tracker.observe(&p, self.ms(at));
                self.clock.event(&p, self.ms(at));
                let session = p["session_id"].as_str().unwrap_or("");
                let payload = &p["payload"];
                match p["type"].as_str().unwrap_or("") {
                    "message.complete" => {
                        if payload["status"] == "complete" || payload["status"] == "completed" {
                            self.completed += 1;
                        } else {
                            self.failed += 1;
                            if self.failures.len() < 3 {
                                let kind = payload["failure_reason"]
                                    .as_str()
                                    .or_else(|| payload["error"].as_str())
                                    .unwrap_or("unknown");
                                self.failures.push(json!({"status":sanitise(payload["status"].as_str().unwrap_or("unknown")),"kind":sanitise(kind)}));
                            }
                        }
                    }
                    "tool.start" | "tool.complete" => {
                        if payload["name"] == "todo_list"
                            || payload["tool_name"] == "todo_list"
                            || payload["tool"] == "todo_list"
                        {
                            let c = self.tools.entry(session.into()).or_default();
                            if p["type"] == "tool.start" {
                                c.0 += 1;
                            } else {
                                c.1 += 1;
                            }
                        }
                    }
                    "session.usage" => {
                        if let Some(o) = payload["rate_limit"].as_object() {
                            self.rate_keys.extend(o.keys().map(|k| sanitise(k)));
                        }
                    }
                    _ => {}
                }
            }
        }
    }
    fn drain(&mut self, rx: &mut mpsc::UnboundedReceiver<Signal>) {
        while let Ok(signal) = rx.try_recv() {
            self.take(signal);
        }
    }
    async fn wait(&mut self, rx: &mut mpsc::UnboundedReceiver<Signal>, duration: Duration) {
        let deadline = tokio::time::Instant::now() + duration;
        while let Ok(Some(signal)) = tokio::time::timeout_at(deadline, rx.recv()).await {
            self.take(signal);
        }
    }
    async fn terminal(
        &mut self,
        rx: &mut mpsc::UnboundedReceiver<Signal>,
        sessions: &[Session],
        deadline: Instant,
        started: bool,
    ) -> bool {
        loop {
            self.drain(rx);
            if sessions.iter().all(|s| {
                if started {
                    self.clock.started(&s.id)
                } else {
                    self.clock.completed(&s.id)
                }
            }) {
                return true;
            }
            let left = deadline.saturating_duration_since(Instant::now());
            if left.is_zero() {
                return false;
            }
            match tokio::time::timeout(left, rx.recv()).await {
                Ok(Some(s)) => self.take(s),
                _ => return false,
            }
        }
    }
}
fn sampler(pid: u32, tx: mpsc::UnboundedSender<Signal>) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_millis(500));
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            interval.tick().await;
            let sample = tokio::process::Command::new("ps")
                .args(["-A", "-o", "pid=,ppid=,rss=,time="])
                .output()
                .await
                .ok()
                .filter(|o| o.status.success())
                .and_then(|o| String::from_utf8(o.stdout).ok())
                .and_then(|s| parse_ps(&s, pid));
            if tx.send(Signal::Sample(sample, Instant::now())).is_err() {
                break;
            }
        }
    })
}
fn foreground(client: Client, tx: mpsc::UnboundedSender<Signal>) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_millis(500));
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            interval.tick().await;
            let (result, ms) = rpc(&client, "runtime.describe", json!({})).await;
            if tx
                .send(Signal::Foreground(result.map(|_| ms).map_err(|_| ())))
                .is_err()
            {
                break;
            }
        }
    })
}
fn median(values: &[f64]) -> Option<f64> {
    if values.is_empty() {
        return None;
    }
    let mut values = values.to_vec();
    values.sort_by(f64::total_cmp);
    let n = values.len();
    Some(if n.is_multiple_of(2) {
        (values[n / 2 - 1] + values[n / 2]) / 2.0
    } else {
        values[n / 2]
    })
}
fn rounded(n: f64) -> f64 {
    (n * 10.0).round() / 10.0
}
fn resources(m: &Measure, create_at: Instant, cleanup_at: Instant) -> Value {
    if m.sampler_failed {
        return Value::Null;
    }
    let before = median(
        &m.samples
            .iter()
            .filter(|(at, _)| *at < create_at)
            .map(|(_, p)| p.rss_mb)
            .collect::<Vec<_>>(),
    );
    let Some((last_at, last)) = m.samples.last() else {
        return Value::Null;
    };
    let after = median(
        &m.samples
            .iter()
            .filter(|(at, _)| last_at.saturating_duration_since(*at) <= Duration::from_secs(5))
            .map(|(_, p)| p.rss_mb)
            .collect::<Vec<_>>(),
    );
    let peak = m
        .samples
        .iter()
        .filter(|(at, _)| *at >= create_at && *at <= cleanup_at)
        .map(|(_, p)| p.rss_mb)
        .max_by(f64::total_cmp);
    let Some(((before, after), peak)) = before.zip(after).zip(peak) else {
        return Value::Null;
    };
    let batch_last = m
        .samples
        .iter()
        .rev()
        .find(|(at, _)| *at <= cleanup_at)
        .map(|(_, p)| p)
        .unwrap_or(last);
    let first = m
        .samples
        .iter()
        .find(|(at, _)| *at >= create_at)
        .map(|(_, p)| p)
        .unwrap_or(last);
    let cpu: f64 = first
        .processes
        .iter()
        .filter_map(|p| {
            batch_last
                .processes
                .iter()
                .find(|l| l.pid == p.pid)
                .map(|l| (l.cpu_s - p.cpu_s).max(0.0))
        })
        .sum();
    json!({"rss_mb":{"before":rounded(before),"peak":rounded(peak),"after":rounded(after)},"cpu_s_estimate":rounded(cpu),"children_peak":m.samples.iter().map(|(_,p)|p.children).max().unwrap_or(0),"sampler_bad_lines":m.samples.iter().map(|(_,p)|p.bad_lines).sum::<u64>()})
}
fn unavailable(record: &mut Value, field: &str, reason: &str) {
    record["unavailable"]
        .as_array_mut()
        .unwrap()
        .push(json!({"field":field,"reason":reason}));
}
fn fill_unavailable(record: &mut Value) {
    let keys: Vec<String> = record
        .as_object()
        .unwrap()
        .iter()
        .filter(|(_, v)| v.is_null())
        .map(|(k, _)| k.clone())
        .collect();
    for k in keys {
        if !record["unavailable"]
            .as_array()
            .unwrap()
            .iter()
            .any(|u| u["field"] == k)
        {
            let reason = match k.as_str() {
                "resources" => "sampler failed",
                "provider" => "external provider",
                "recovery" => "not applicable",
                _ => "no samples",
            };
            unavailable(record, &k, reason);
        }
    }
}
async fn replay_gaps(m: &mut Measure, record: &mut Value, sessions: &[Session], links: &[Link]) {
    for request in m.tracker.replay_requests() {
        let Some(session) = sessions.iter().find(|s| request["session_id"] == s.id) else {
            continue;
        };
        let (result, _) = rpc(
            &links[session.connection].client,
            "session.events.since",
            request.clone(),
        )
        .await;
        match result {
            Ok(value) => m.tracker.resolve(
                &session.id,
                request["replay_epoch"].as_str().unwrap_or(""),
                &value,
            ),
            Err(e) => count_error(record, "session.events.since", &e),
        }
    }
}
async fn cleanup(sessions: &[Session], links: &[Link], record: &mut Value) {
    let calls = sessions
        .iter()
        .filter(|s| !s.subscription.is_empty())
        .map(|s| {
            rpc(
                &links[s.connection].client,
                "session.detach",
                json!({"session_id":s.id,"subscription_id":s.subscription}),
            )
        });
    match tokio::time::timeout(Duration::from_secs(30), join_all(calls)).await {
        Ok(replies) => {
            for (result, _) in replies {
                if let Err(e) = result {
                    count_error(record, "session.detach", &e);
                    record["notes"] = json!("cleanup incomplete");
                }
            }
        }
        Err(_) => {
            increment(record, "timeouts");
            record["notes"] = json!("cleanup incomplete");
        }
    }
    for link in links {
        link.client.close().await;
    }
}
async fn receipts(
    m: &mut Measure,
    rx: &mut mpsc::UnboundedReceiver<Signal>,
    record: &mut Value,
    sessions: &[Session],
    client: &Client,
    start: Instant,
) -> Value {
    let mut pending: BTreeSet<_> = (0..sessions.len()).collect();
    let mut samples = Vec::new();
    let mut with = 0;
    let mut without = 0;
    let mut unknown = 0;
    while !pending.is_empty() && start.elapsed() < Duration::from_secs(120) {
        let replies = join_all(pending.iter().map(|i| {
            let s = &sessions[*i];
            async move {
                let remaining = Duration::from_secs(120).saturating_sub(start.elapsed());
                let result = tokio::time::timeout(
                    remaining,
                    rpc(
                        client,
                        "prompt.receipt",
                        json!({"session_id":s.id,"admission_id":s.admission,"include_result":true}),
                    ),
                )
                .await
                .map(|r| r.0)
                .unwrap_or_else(|_| {
                    Err(CallError::TimedOut {
                        method: "prompt.receipt".into(),
                        after: Duration::from_secs(120),
                    })
                });
                (*i, result, start.elapsed().as_millis() as u64)
            }
        }))
        .await;
        for (i, result, arrival_ms) in replies {
            match result {
                Ok(v) => match v["status"].as_str().unwrap_or("") {
                    "terminal" => {
                        samples.push(arrival_ms);
                        println!(
                            "receipt terminal saved_result={}",
                            v.get("result").is_some_and(|r| !r.is_null())
                        );
                        if v.get("result").is_some_and(|r| !r.is_null()) {
                            with += 1;
                        } else {
                            without += 1;
                        }
                        pending.remove(&i);
                    }
                    "unknown" => {
                        unknown += 1;
                        println!("receipt unknown saved_result=false");
                        pending.remove(&i);
                    }
                    _ => {}
                },
                Err(e) => {
                    count_error(record, "prompt.receipt", &e);
                    pending.remove(&i);
                }
            }
        }
        if !pending.is_empty() {
            m.wait(rx, Duration::from_secs(1)).await;
        }
    }
    json!({"terminal_with_result":with,"terminal_without_result":without,"unknown":unknown,"unresolved":sessions.len() as u64-with-without-unknown,
        "receipt_ms":percentile(&samples),"resume_ms":null,"next_turn_ms":null,"replay_truncated":0,"submit_after_detach_refused":null,"eviction_evidence":null})
}
async fn recover_drop(
    env: &LiveEnv,
    m: &mut Measure,
    rx: &mut mpsc::UnboundedReceiver<Signal>,
    tx: &mpsc::UnboundedSender<Signal>,
    record: &mut Value,
    sessions: &mut [Session],
    links: &mut Vec<Link>,
) {
    let positions: Vec<_> = sessions.iter().map(|s| m.tracker.position(&s.id)).collect();
    for link in links.iter() {
        link.client.close().await;
    }
    println!(
        "dropped connection with {} admissions started",
        sessions.len()
    );
    links.clear();
    m.wait(rx, Duration::from_secs(5)).await;
    let Ok(link) = attach(env, tx.clone()).await else {
        record["recovery"] = json!({"terminal_with_result":0,"terminal_without_result":0,"unknown":0,"unresolved":sessions.len(),"receipt_ms":null,"resume_ms":null,"next_turn_ms":null,"replay_truncated":0,"submit_after_detach_refused":null,"eviction_evidence":null});
        return;
    };
    links.push(link);
    println!("reconnected");
    let client = &links[0].client;
    let mut recovery = receipts(m, rx, record, sessions, client, Instant::now()).await;
    let mut resume_ms = Vec::new();
    for (s, (seq, epoch)) in sessions.iter_mut().zip(positions) {
        s.connection = 0;
        s.subscription.clear();
        let (result, ms) = rpc(client, "session.resume", json!({"session_id":s.id})).await;
        match result {
            Ok(v) => {
                resume_ms.push(ms);
                s.subscription = v["subscription_id"].as_str().unwrap_or("").into();
            }
            Err(e) => count_error(record, "session.resume", &e),
        }
        let (result, _) = rpc(
            client,
            "session.events.since",
            json!({"session_id":s.id,"last_sequence":seq,"replay_epoch":epoch}),
        )
        .await;
        match result {
            Ok(v) => {
                if v["truncated"] == true {
                    increment(&mut recovery, "replay_truncated");
                }
                m.tracker.resolve(&s.id, &epoch, &v);
            }
            Err(e) => count_error(record, "session.events.since", &e),
        }
    }
    recovery["resume_ms"] = json!(percentile(&resume_ms));
    record["recovery"] = recovery;
}
async fn recover_detached(
    m: &mut Measure,
    rx: &mut mpsc::UnboundedReceiver<Signal>,
    record: &mut Value,
    sessions: &mut [Session],
    links: &[Link],
    run: &str,
    id: &str,
) {
    for s in sessions.iter_mut() {
        let (result, _) = rpc(
            &links[s.connection].client,
            "session.detach",
            json!({"session_id":s.id,"subscription_id":s.subscription}),
        )
        .await;
        if let Err(e) = result {
            count_error(record, "session.detach", &e);
        } else {
            s.subscription.clear();
        }
    }
    m.wait(rx, Duration::from_secs(60)).await;
    let receipt_origin = Instant::now();
    let first = &sessions[0];
    let client = &links[first.connection].client;
    let (result, _) = rpc(
        client,
        "prompt.submit",
        submit_params(run, id, 0, 1, &first.id),
    )
    .await;
    let refused = matches!(result,Err(CallError::Rpc(ref e)) if e.reason.as_deref()==Some("permission_denied"));
    let mut recovery = receipts(m, rx, record, sessions, client, receipt_origin).await;
    recovery["submit_after_detach_refused"] = json!(refused);
    let (result, ms) = rpc(client, "session.resume", json!({"session_id":first.id})).await;
    match result {
        Ok(v) => {
            sessions[0].subscription = v["subscription_id"].as_str().unwrap_or("").into();
            recovery["resume_ms"] = json!(percentile(&[ms]));
        }
        Err(e) => count_error(record, "session.resume", &e),
    }
    let failures_before = m.failed;
    let start = Instant::now();
    m.clock.sent(&sessions[0].id, m.ms(start));
    let (result, _) = rpc(
        client,
        "prompt.submit",
        submit_params(run, id, 0, 2, &sessions[0].id),
    )
    .await;
    match result {
        Ok(v) => {
            m.clock
                .admitted(&sessions[0].id, v["admission_id"].as_str().unwrap_or(""));
            if m.terminal(rx, &sessions[..1], start + Duration::from_secs(60), false)
                .await
                && m.failed == failures_before
            {
                recovery["next_turn_ms"] = json!(start.elapsed().as_millis() as u64);
            }
        }
        Err(e) => count_error(record, "prompt.submit", &e),
    }
    record["recovery"] = recovery;
}

async fn measure(env: &LiveEnv, series: &str, topology: &str, width: u32, run: &Value) -> Value {
    let mut record = empty_record(series, topology, width, run.clone());
    let id = record["id"].as_str().unwrap().to_owned();
    let run_id = run["run_id"].as_str().unwrap();
    let (tx, mut rx) = mpsc::unbounded_channel();
    let mut m = Measure::new();
    let foreground_link = attach(env, tx.clone())
        .await
        .expect("foreground connection failed");
    let (desc, _) = rpc(&foreground_link.client, "runtime.describe", json!({})).await;
    if let Err(reason) = desc
        .as_ref()
        .map_err(|_| "descriptor call failed".to_owned())
        .and_then(descriptor_gate)
    {
        record["notes"] = json!(format!("stopped: {reason}"));
        return record;
    }
    let provider = if series == "B" {
        None
    } else {
        Some(
            LoopbackProvider::start(match series {
                "C" => ProviderScript::Todo,
                "D" => ProviderScript::Slow,
                _ => ProviderScript::Ready,
            })
            .await
            .expect("loopback provider failed"),
        )
    };
    let pid = hermes_client::discovery::control_query(
        &env.home,
        hermes_client::discovery::ReadVerb::Identify,
        &hermes_client::DiscoverOptions {
            ensure: hermes_client::EnsurePolicy::Never,
            ..Default::default()
        },
    )
    .await
    .ok()
    .and_then(|v| v["pid"].as_u64())
    .map(|v| v as u32);
    let sampler_task = pid.map(|pid| sampler(pid, tx.clone()));
    if pid.is_none() {
        m.sampler_failed = true;
    }
    let foreground_task = foreground(foreground_link.client.clone(), tx.clone());
    m.wait(&mut rx, Duration::from_secs(5)).await;
    record["evidence"] = json!("live");
    record["notes"] = json!("");
    record["rpc_errors"] = json!({});
    for k in [
        "turns_completed",
        "turns_failed",
        "admissions_unknown",
        "create_refused",
        "timeouts",
        "foreground_errors",
        "distinct_sessions",
    ] {
        record[k] = json!(0);
    }
    let mut links = Vec::new();
    for _ in 0..if topology == "shared" { 1 } else { width } {
        links.push(
            attach(env, tx.clone())
                .await
                .expect("measurement connection failed"),
        );
    }
    let create_at = Instant::now();
    let base_url = provider.as_ref().map_or("", |p| p.base_url());
    let creates = join_all((0..width).map(|n| {
        let link = &links[if topology == "shared" { 0 } else { n as usize }];
        let params = create_params(run_id, &id, n, base_url);
        async move { (n, rpc(&link.client, "session.create", params).await) }
    }));
    let replies = tokio::time::timeout(Duration::from_secs(60), creates).await;
    let mut sessions = Vec::new();
    let mut create_ms = Vec::new();
    let mut tool_names = Vec::new();
    let mut refusal = None;
    if let Ok(replies) = replies {
        for (n, (result, ms)) in replies {
            match result {
                Ok(v) => {
                    let snapshot = v.get("snapshot").unwrap_or(&v);
                    if n == 0
                        && let Some(tools) = snapshot["tools"].as_array()
                    {
                        tool_names = tools
                            .iter()
                            .filter_map(|v| v.as_str().or_else(|| v["name"].as_str()))
                            .map(sanitise)
                            .collect();
                    }
                    if let (Some(session), Some(subscription)) = (
                        snapshot["session_id"].as_str(),
                        snapshot["subscription_id"].as_str(),
                    ) {
                        create_ms.push(ms);
                        sessions.push(Session {
                            id: session.into(),
                            subscription: subscription.into(),
                            connection: if topology == "shared" { 0 } else { n as usize },
                            admission: String::new(),
                        });
                    } else {
                        increment(&mut record, "create_refused");
                    }
                }
                Err(e) => {
                    increment(&mut record, "create_refused");
                    count_error(&mut record, "session.create", &e);
                    refusal.get_or_insert_with(|| code(&e));
                }
            }
        }
    } else {
        increment(&mut record, "timeouts");
    }
    record["distinct_sessions"] = json!(
        sessions
            .iter()
            .map(|s| &s.id)
            .collect::<BTreeSet<_>>()
            .len()
    );
    let mut submits = Vec::new();
    let mut statuses = json!({"queued":0,"started":0,"unknown":0,"terminal":0});
    let mut peaks = Vec::new();
    let batch_start = Instant::now();
    let repetitions = if series == "A" { 3 } else { 1 };
    if record["distinct_sessions"] == width {
        for rep in 0..repetitions {
            if let Some(p) = &provider {
                p.reset_peak().await;
            }
            let calls = sessions.iter().enumerate().map(|(n, s)| {
                let client = links[s.connection].client.clone();
                let params = submit_params(run_id, &id, n as u32, rep, &s.id);
                let session = s.id.clone();
                async move {
                    let start = Instant::now();
                    let (result, ms) = rpc(&client, "prompt.submit", params).await;
                    (session, start, result, ms)
                }
            });
            let replies = join_all(calls).await;
            let mut deadline = Instant::now();
            for (session, start, result, ms) in replies {
                m.clock.sent(&session, m.ms(start));
                deadline = deadline.min(start);
                match result {
                    Ok(v) => {
                        submits.push(ms);
                        if let Some(status) = v["status"].as_str() {
                            if statuses.get(status).is_some() {
                                increment(&mut statuses, status);
                            }
                            if status == "unknown" {
                                increment(&mut record, "admissions_unknown");
                            }
                        }
                        if let Some(s) = sessions.iter_mut().find(|s| s.id == session) {
                            s.admission = v["admission_id"].as_str().unwrap_or("").into();
                            m.clock.admitted(&session, &s.admission);
                        }
                    }
                    Err(e) => {
                        increment(&mut record, "admissions_unknown");
                        count_error(&mut record, "prompt.submit", &e);
                    }
                }
            }
            m.drain(&mut rx);
            if series == "D" {
                let all_admitted = statuses["started"] == width;
                if !all_admitted
                    && !m
                        .terminal(&mut rx, &sessions, deadline + Duration::from_secs(30), true)
                        .await
                {
                    increment(&mut record, "timeouts");
                }
                recover_drop(
                    env,
                    &mut m,
                    &mut rx,
                    &tx,
                    &mut record,
                    &mut sessions,
                    &mut links,
                )
                .await;
            } else if !m
                .terminal(
                    &mut rx,
                    &sessions,
                    deadline + Duration::from_secs(if series == "B" { 300 } else { 60 }),
                    false,
                )
                .await
            {
                let missing = sessions
                    .iter()
                    .filter(|s| !m.clock.completed(&s.id))
                    .count() as u64;
                record["timeouts"] = json!(record["timeouts"].as_u64().unwrap_or(0) + missing);
            }
            if let Some(p) = &provider {
                peaks.push(p.stats().await.turn_requests_peak_min);
            }
        }
    }
    let batch_wall = batch_start.elapsed().as_millis() as u64;
    let main_completed = m.completed;
    let main_failed = m.failed;
    let mut main_events = m.tracker.report();
    let main_turn_ms = m.clock.turn_ms.clone();
    let main_start_ms = m.clock.start_delay_ms.clone();
    let main_delta_ms = m.clock.first_delta_ms.clone();
    if series == "E" && !sessions.is_empty() {
        recover_detached(
            &mut m,
            &mut rx,
            &mut record,
            &mut sessions,
            &links,
            run_id,
            &id,
        )
        .await;
    }
    replay_gaps(&mut m, &mut record, &sessions, &links).await;
    main_events["unresolved_gaps"] = m.tracker.report()["unresolved_gaps"].clone();
    cleanup(&sessions, &links, &mut record).await;
    let cleanup_at = Instant::now();
    m.wait(&mut rx, Duration::from_secs(60)).await;
    foreground_task.abort();
    foreground_link.client.close().await;
    if let Some(task) = sampler_task {
        task.abort();
    }
    m.drain(&mut rx);
    record["create_ms"] = json!(percentile(&create_ms));
    record["submit_ms"] = json!(percentile(&submits));
    record["start_delay_ms"] = json!(percentile(&main_start_ms));
    record["first_delta_ms"] = json!(percentile(&main_delta_ms));
    record["turn_ms"] = json!(percentile(&main_turn_ms));
    record["batch_wall_ms"] = json!(batch_wall);
    record["turns_completed"] = json!(main_completed);
    record["turns_failed"] = json!(main_failed);
    record["foreground_errors"] = json!(m.foreground_errors);
    record["foreground_rtt_ms"] = json!(percentile(&m.foreground));
    record["events"] = main_events;
    record["resources"] = resources(&m, create_at, cleanup_at);
    if let Some(p) = &provider {
        let mut stats = p.stats().await;
        stats.turn_requests_peak_min = peaks.into_iter().min().unwrap_or(0);
        if series == "A" && width == 1 && stats.turn_requests != u64::from(repetitions) {
            record["notes"] = json!("stopped: turn request classification failed");
            increment(&mut record, "admissions_unknown");
        }
        record["provider"] = serde_json::to_value(stats).unwrap();
    }
    record["observations"] = json!({"tool_names":if width==1||series!="A"&&series!="B" {Some(tool_names)}else{None},"tool_executions":if series=="C"{Some(m.tools.values().filter(|c|**c==(1,1)).count())}else{None},"rate_limit_keys":if series=="B"{Some(m.rate_keys)}else{None},"failure_examples":m.failures,"admission_statuses":statuses});
    if series == "D" {
        for key in [
            "turn_ms",
            "batch_wall_ms",
            "turns_completed",
            "turns_failed",
            "complete_per_admission",
        ] {
            if key == "complete_per_admission" {
                record["events"][key] = Value::Null;
            } else {
                record[key] = Value::Null;
            }
            unavailable(&mut record, key, "connection dropped by design");
        }
    }
    fill_unavailable(&mut record);
    if ["D", "E"].contains(&series) {
        for key in ["receipt_ms", "resume_ms", "next_turn_ms"] {
            if record["recovery"][key].is_null() && !(series == "D" && key == "next_turn_ms") {
                unavailable(&mut record, &format!("recovery.{key}"), "no samples");
            }
        }
    }
    if let Some(reason) = refusal {
        let notes = if series == "B" && reason == "unsupported_managed_policy" {
            Some(reason)
        } else if series == "C" {
            Some(format!("toolset refused: {reason}"))
        } else if series == "A" && width == 1 {
            Some(format!("loopback provider refused: {reason}"))
        } else {
            None
        };
        if let Some(notes) = notes {
            let mut unrun = empty_record(series, topology, width, run.clone());
            unrun["notes"] = json!(notes);
            return unrun;
        }
    }
    record
}

pub async fn series(series: &str) -> Vec<Value> {
    let prefix = "GATEWAY_CONCURRENCY_RESULT";
    let env = match live_preconditions().await {
        Ok(env) => env,
        Err(reason) => {
            report_skip(prefix, &reason);
            return Vec::new();
        }
    };
    let mode = execution_mode();
    let identity = run_identity();
    let cap = std::env::var("TALARIA_PROBE_MAX_WIDTH")
        .ok()
        .map(|s| s.parse::<u32>().expect("invalid width cap"))
        .unwrap_or(20);
    assert!(cap >= 1, "width cap must include the control");
    let mut stopped = BTreeSet::new();
    let mut stop_note = None;
    let mut control_failed = false;
    let mut records = Vec::new();
    for (s, topology, width) in matrix().into_iter().filter(|(s, _, _)| s == series) {
        let mut record = empty_record(&s, &topology, width, identity.clone());
        if let Some(note) = &stop_note {
            record["notes"] = json!(note);
        } else if width > cap {
            record["notes"] = json!("above the operator's width cap");
        } else if stopped.contains(&topology) {
            record["notes"] = json!("stopped after an unhealthy width");
        } else {
            record = match tokio::time::timeout(
                Duration::from_secs(900),
                measure(&env, &s, &topology, width, &identity),
            )
            .await
            {
                Ok(record) => record,
                Err(_) => panic!("record deadline exceeded; verify fixture cleanup"),
            };
            if record["evidence"] == "live" {
                let previous = std::fs::read(results_path())
                    .ok()
                    .and_then(|b| serde_json::from_slice::<Value>(&b).ok());
                let baseline_id = if s == "B" {
                    "B-shared-1".to_owned()
                } else if s == "A" {
                    format!("A-{topology}-1")
                } else {
                    "A-shared-1".into()
                };
                let baseline = if record["id"] == baseline_id {
                    record.clone()
                } else {
                    previous
                        .as_ref()
                        .and_then(|v| v["runs"].as_array())
                        .and_then(|a| a.iter().find(|r| r["id"] == baseline_id))
                        .cloned()
                        .unwrap_or(Value::Null)
                };
                if baseline["evidence"] != "live" {
                    unavailable(&mut record, "baseline", "no baseline");
                }
                let (ok, criteria) = healthy(&record, &baseline);
                record["healthy"] = json!(ok);
                record["failed_criteria"] = json!(criteria);
                if !ok {
                    println!("UNHEALTHY {}", record["failed_criteria"]);
                    if width == 1 && topology == "shared" {
                        control_failed = true;
                        stop_note = Some("stopped after an unhealthy width".to_owned());
                    }
                    if record["failed_criteria"]
                        .as_array()
                        .unwrap()
                        .contains(&json!(1))
                    {
                        stopped.insert(topology.clone());
                    }
                }
            } else {
                stopped.insert(topology.clone());
                stop_note = record["notes"].as_str().map(str::to_owned);
                if width == 1
                    && topology == "shared"
                    && record["notes"] != "unsupported_managed_policy"
                    || ["D", "E"].contains(&s.as_str())
                {
                    control_failed = true;
                }
            }
        }
        merge_record(&results_path(), RELEASE, &mode, &record).expect("record write rejected");
        records.push(record);
    }
    assert!(!control_failed, "width one control failed");
    records
}
