//! Live steer, stop, and open against a gateway.
//!
//! Ignored unless a caller asks for ignored tests. A missing gateway, release,
//! profile, provider sign-in, or model prints SKIPPED and returns. Required
//! mode panics instead of treating that skip as a pass.

use hermes_client::{
    CallError, CallOptions, Client, DiscoverOptions, Endpoint, EnsurePolicy, Inbound, InboundEvent,
    InboundStream, Purpose, Scope, connect, default_home, discover_with, handshake, mint_ticket,
};
use hermes_contract::schemas::ModelOptionsParams;
use serde_json::{Value, json};
use std::{
    path::Path,
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use talaria_app::app::AppMessage;
use talaria_app::workflow_progress::ProgressService;
use talaria_workflow::engine::CallId;
use talaria_workflow::journal::testing::Writer as JournalWriter;
use talaria_workflow::journal::{
    AdmissionState, CallOutcome, NewManifest, OnUnknown, Record, ResolvedLaunch, RunDir,
};
use talaria_workflow::progress::AgentState;
use talaria_workflow::progress::live::{
    self, ProgressEffect, ProgressInput, ProgressKey, ProgressState,
};
use tokio::sync::mpsc::{UnboundedReceiver, unbounded_channel};

const MODEL: &str = "minimax-m3.1-flash";
const PROVIDER: &str = "ollama-cloud";
const PROFILE: &str = "fixture";
const RELEASE: &str = "0.22.4";
const KEY_FINISHED: &str = "11111111111111111111111111111111";
const KEY_RUNNING: &str = "88888888888888888888888888888888";
const MARKER: &str = "live-steer-marker";
const PROMPT: &str = "Use the search tool once for the word workflow. Then count from 1 to 200, one number per line, and do not finish early.";

struct Gateway {
    client: Client,
    inbound: InboundStream,
}

fn required() -> bool {
    std::env::var("TALARIA_LIVE_REQUIRE").ok().as_deref() == Some("1")
}

fn skip(reason: &str) -> Option<Gateway> {
    if required() {
        panic!("live gateway required: {reason}");
    }
    eprintln!("SKIPPED: {reason}");
    None
}

fn unique(label: &str) -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    format!("fixture-{label}-{nanos}")
}

fn launch() -> ResolvedLaunch {
    ResolvedLaunch {
        profile: PROFILE.into(),
        model: MODEL.into(),
        provider: Some(PROVIDER.into()),
        effort: None,
        toolsets: vec!["search".into()],
        skills: vec![],
        max_turns: None,
        isolation: None,
        cwd: None,
    }
}

fn release_ok(version: &str) -> bool {
    version == RELEASE || version.strip_prefix('v') == Some(RELEASE)
}

async fn connect_gateway(
    endpoint: &Endpoint,
    home: &Path,
    profile: &hermes_client::ProfileId,
) -> Result<Gateway, CallError> {
    let ticket = mint_ticket(
        home,
        endpoint,
        profile,
        Purpose::Interactive,
        Scope::profile(),
    )
    .await
    .map_err(|_| CallError::NotConnected)?;
    let mut connection = connect(endpoint, ticket)
        .await
        .map_err(|_| CallError::NotConnected)?;
    handshake(&mut connection)
        .await
        .map_err(|_| CallError::NotConnected)?;
    let (client, inbound) = Client::start(connection.into_parts());
    Ok(Gateway { client, inbound })
}

fn model_blocker(options: &hermes_contract::schemas::ModelOptionsResult) -> Option<&'static str> {
    let Some(row) = options
        .providers
        .iter()
        .find(|row| row.slug == PROVIDER || row.name == PROVIDER)
    else {
        return Some("provider ollama-cloud is not signed in");
    };
    if row.authenticated != Some(true) {
        return Some("provider ollama-cloud is not signed in");
    }
    let unavailable = row.unavailable_models.as_deref().unwrap_or(&[]);
    if unavailable.iter().any(|model| model == MODEL) {
        return Some("model minimax-m3.1-flash is unavailable");
    }
    let listed = row.models.iter().any(|model| model == MODEL)
        || row
            .featured_models
            .as_ref()
            .is_some_and(|models| models.iter().any(|model| model == MODEL));
    if !listed {
        return Some("model minimax-m3.1-flash is unavailable");
    }
    None
}

async fn prepare() -> Option<Gateway> {
    let home = default_home();
    let options = DiscoverOptions {
        control_timeout: Duration::from_secs(5),
        ensure: EnsurePolicy::Never,
    };
    let mut endpoint = match discover_with(&home, &options).await {
        Ok(endpoint) => endpoint,
        Err(_) => return skip("no gateway is listening on the default local address"),
    };
    if let Ok(url) = std::env::var("TALARIA_HERMES_URL") {
        let url = url.trim();
        if !url.is_empty() {
            endpoint = match endpoint.with_api_origin(url) {
                Ok(endpoint) => endpoint,
                Err(_) => return skip("TALARIA_HERMES_URL is not a local gateway origin"),
            };
        }
    }
    if !endpoint.code_version().is_some_and(release_ok) {
        return skip("gateway release is not 0.22.4");
    }
    let profile = match endpoint.resolve_profile(PROFILE) {
        Ok(profile) => profile,
        Err(_) => return skip("profile fixture is unavailable"),
    };
    let gateway = match connect_gateway(&endpoint, &home, &profile).await {
        Ok(gateway) => gateway,
        Err(_) => return skip("gateway connection failed"),
    };
    let options = match gateway
        .client
        .call::<hermes_contract::methods::legacy::model_options::Marker>(ModelOptionsParams {
            profile: Some(PROFILE.into()),
            refresh: true,
            ..ModelOptionsParams::default()
        })
        .await
    {
        Ok(options) => options,
        Err(_) => {
            gateway.client.close().await;
            return skip("provider ollama-cloud is not signed in");
        }
    };
    if let Some(reason) = model_blocker(&options) {
        gateway.client.close().await;
        return skip(reason);
    }
    Some(gateway)
}

async fn gateway_call(client: &Client, method: &str, params: Value) -> Result<Value, CallError> {
    client
        .call_raw(
            method,
            params,
            CallOptions {
                timeout: Some(Duration::from_secs(30)),
            },
        )
        .await
}

fn for_session(event: &InboundEvent, session: &str) -> bool {
    event.frame.session_id.as_deref() == Some(session)
        || event.raw.get("session_id").and_then(Value::as_str) == Some(session)
}

fn output_event(event: &InboundEvent) -> bool {
    matches!(
        event.frame.type_name(),
        Some(
            "message.start"
                | "message.delta"
                | "message.complete"
                | "tool.start"
                | "tool.complete"
                | "tool.progress"
                | "agent.print"
        )
    )
}

async fn wait_until_active(gateway: &mut Gateway, session: &str, admission: &str) {
    let started = tokio::time::Instant::now();
    let mut next_receipt = tokio::time::Instant::now();
    loop {
        if started.elapsed() > Duration::from_secs(90) {
            panic!("turn did not become active");
        }
        match tokio::time::timeout(Duration::from_millis(200), gateway.inbound.next()).await {
            Ok(Some(Inbound::Event(event)))
                if for_session(&event, session) && output_event(&event) =>
            {
                return;
            }
            Ok(Some(_)) => {}
            Ok(None) => panic!("gateway stream closed before the turn became active"),
            Err(_) => {}
        }
        if tokio::time::Instant::now() < next_receipt {
            continue;
        }
        next_receipt = tokio::time::Instant::now() + Duration::from_secs(1);
        let receipt = gateway_call(
            &gateway.client,
            "prompt.receipt",
            json!({
                "profile": PROFILE,
                "session_id": session,
                "admission_id": admission,
                "include_result": true
            }),
        )
        .await
        .unwrap_or_else(|error| panic!("prompt receipt failed: {error}"));
        if receipt["status"] == "terminal" {
            panic!("turn finished before it could be steered");
        }
        if receipt["status"] == "started" {
            return;
        }
    }
}

async fn wait_for_delivery(gateway: &mut Gateway, session: &str, admission: &str) {
    let started = tokio::time::Instant::now();
    let mut next_receipt = tokio::time::Instant::now();
    loop {
        if started.elapsed() > Duration::from_secs(150) {
            panic!("steer was acknowledged but the agent output never included it");
        }
        match tokio::time::timeout(Duration::from_millis(200), gateway.inbound.next()).await {
            Ok(Some(Inbound::Event(event)))
                if for_session(&event, session) && event.raw.to_string().contains(MARKER) =>
            {
                return;
            }
            Ok(Some(_)) => {}
            Ok(None) => panic!("gateway stream closed before steer delivery"),
            Err(_) => {}
        }
        if tokio::time::Instant::now() < next_receipt {
            continue;
        }
        next_receipt = tokio::time::Instant::now() + Duration::from_secs(1);
        let receipt = gateway_call(
            &gateway.client,
            "prompt.receipt",
            json!({
                "profile": PROFILE,
                "session_id": session,
                "admission_id": admission,
                "include_result": true
            }),
        )
        .await
        .unwrap_or_else(|error| panic!("prompt receipt failed: {error}"));
        if receipt.to_string().contains(MARKER) {
            return;
        }
        if receipt["status"] == "terminal" {
            panic!("turn settled before the steer appeared in the output");
        }
    }
}

async fn wait_for_interrupt(client: &Client, session: &str, admission: &str) -> Value {
    let started = tokio::time::Instant::now();
    loop {
        if started.elapsed() > Duration::from_secs(90) {
            panic!("interrupted turn did not settle");
        }
        let receipt = gateway_call(
            client,
            "prompt.receipt",
            json!({
                "profile": PROFILE,
                "session_id": session,
                "admission_id": admission,
                "include_result": true
            }),
        )
        .await
        .unwrap_or_else(|error| panic!("prompt receipt failed: {error}"));
        if receipt["status"] == "terminal" {
            let outcome = receipt["outcome"].as_str().unwrap_or_default();
            if outcome != "interrupted" && outcome != "failed" {
                panic!("stop left the turn {outcome} instead of interrupted");
            }
            return receipt;
        }
        tokio::time::sleep(Duration::from_secs(1)).await;
    }
}

async fn settle(
    service: &mut ProgressService,
    state: &mut ProgressState,
    rx: &mut UnboundedReceiver<AppMessage>,
    effects: Vec<ProgressEffect>,
) -> bool {
    service
        .execute(state, effects)
        .await
        .expect("execute progress effects");
    let started = tokio::time::Instant::now();
    let mut waiting = true;
    let mut opened = false;
    loop {
        if started.elapsed() > Duration::from_secs(45) {
            panic!("progress effects did not settle");
        }
        let slice = if waiting {
            Duration::from_secs(20)
        } else {
            Duration::from_millis(400)
        };
        match tokio::time::timeout(slice, rx.recv()).await {
            Ok(Some(AppMessage::WorkflowProgress { input, .. })) => {
                waiting = false;
                opened |= matches!(input, ProgressInput::Opened(_));
                let more = live::update(state, input);
                if !more.is_empty() {
                    service
                        .execute(state, more)
                        .await
                        .expect("execute progress effects");
                    waiting = true;
                }
            }
            Ok(Some(_)) => panic!("unexpected application message"),
            Ok(None) => panic!("progress channel closed"),
            Err(_) if !waiting => return opened,
            Err(_) => panic!("timed out waiting for a progress message"),
        }
    }
}

fn turn_intent_count(path: &Path) -> usize {
    let text = std::fs::read_to_string(path).unwrap_or_default();
    text.lines()
        .filter(|line| {
            serde_json::from_str::<Value>(line)
                .ok()
                .and_then(|value| {
                    value
                        .get("t")
                        .and_then(Value::as_str)
                        .map(|tag| tag == "turn_intent")
                })
                .unwrap_or(false)
        })
        .count()
}

async fn append(writer: &JournalWriter, record: Record) {
    writer.append(record).await.expect("append journal record");
}

async fn write_finished(writer: &JournalWriter) {
    let admission = unique("finished-admission");
    append(
        writer,
        Record::CallIntent {
            key: KEY_FINISHED.into(),
            attempt: 0,
            call_id: CallId {
                digest: "b".repeat(64),
                occurrence: 1,
            },
            label: "finished".into(),
            phase: Some("work".into()),
            workflow_path: vec![],
            launch: launch(),
            request_id: unique("finished"),
        },
    )
    .await;
    append(
        writer,
        Record::SessionBound {
            key: KEY_FINISHED.into(),
            attempt: 0,
            session_id: "finished-session".into(),
            stored_session_id: "finished-session".into(),
        },
    )
    .await;
    append(
        writer,
        Record::TurnIntent {
            key: KEY_FINISHED.into(),
            attempt: 0,
            turn: 1,
            submission_id: unique("finished-turn"),
            text: Some("finished earlier".into()),
            text_ref: None,
        },
    )
    .await;
    append(
        writer,
        Record::TurnAdmitted {
            key: KEY_FINISHED.into(),
            attempt: 0,
            turn: 1,
            admission_id: admission.clone(),
            status: AdmissionState::Terminal,
            outcome: Some("completed".into()),
            execution_generation: Some(1),
            authority_epoch: 1,
        },
    )
    .await;
    append(
        writer,
        Record::TurnSettled {
            key: KEY_FINISHED.into(),
            attempt: 0,
            turn: 1,
            admission_id: admission,
            outcome: "completed".into(),
            final_text: Some("ok".into()),
            final_text_ref: None,
            result_meta: json!({}),
            usage: Some(json!({"total_tokens": 1})),
        },
    )
    .await;
    append(
        writer,
        Record::CallFinished {
            key: KEY_FINISHED.into(),
            attempt: 0,
            outcome: CallOutcome::Completed {
                value: Some(json!("ok")),
                value_ref: None,
            },
            turns: 1,
        },
    )
    .await;
}

async fn write_running(
    writer: &JournalWriter,
    session: &str,
    admission: &str,
    generation: Option<i64>,
    epoch: i64,
) {
    append(
        writer,
        Record::CallIntent {
            key: KEY_RUNNING.into(),
            attempt: 0,
            call_id: CallId {
                digest: "c".repeat(64),
                occurrence: 1,
            },
            label: "running".into(),
            phase: Some("work".into()),
            workflow_path: vec![],
            launch: launch(),
            request_id: unique("running"),
        },
    )
    .await;
    append(
        writer,
        Record::SessionBound {
            key: KEY_RUNNING.into(),
            attempt: 0,
            session_id: session.into(),
            stored_session_id: session.into(),
        },
    )
    .await;
    append(
        writer,
        Record::TurnIntent {
            key: KEY_RUNNING.into(),
            attempt: 0,
            turn: 1,
            submission_id: unique("running-turn"),
            text: Some(PROMPT.into()),
            text_ref: None,
        },
    )
    .await;
    append(
        writer,
        Record::TurnAdmitted {
            key: KEY_RUNNING.into(),
            attempt: 0,
            turn: 1,
            admission_id: admission.into(),
            status: AdmissionState::Started,
            outcome: None,
            execution_generation: generation,
            authority_epoch: epoch,
        },
    )
    .await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "live gateway"]
async fn steer_stop_and_open_against_gateway() {
    let Some(mut gateway) = prepare().await else {
        return;
    };
    let created = gateway_call(
        &gateway.client,
        "session.create",
        json!({
            "profile": PROFILE,
            "request_id": unique("create"),
            "source": "tool",
            "hidden": true,
            "model": MODEL,
            "provider": PROVIDER,
            "toolsets": ["search"]
        }),
    )
    .await
    .unwrap_or_else(|error| panic!("session create failed: {error}"));
    let session = created["session_id"]
        .as_str()
        .filter(|id| !id.is_empty())
        .unwrap_or_else(|| panic!("session create returned no session"))
        .to_owned();
    let admitted = gateway_call(
        &gateway.client,
        "prompt.submit",
        json!({
            "profile": PROFILE,
            "session_id": session,
            "submission_id": unique("submit"),
            "text": PROMPT,
            "finite": true
        }),
    )
    .await
    .unwrap_or_else(|error| panic!("prompt submit failed: {error}"));
    if admitted["voice_stopped"] == true {
        panic!("prompt was treated as a voice stop");
    }
    let admission = admitted["admission_id"]
        .as_str()
        .filter(|id| !id.is_empty())
        .unwrap_or_else(|| panic!("prompt submit returned no admission"))
        .to_owned();
    let generation = admitted["execution_generation"].as_i64();
    let epoch = admitted["authority_epoch"].as_i64().unwrap_or(0);
    wait_until_active(&mut gateway, &session, &admission).await;

    let temp = tempfile::tempdir().expect("temporary run directory");
    let run = RunDir::create(
        temp.path(),
        NewManifest {
            workflow_name: "live-progress".into(),
            args: None,
            started_at_ms: 1,
            seed: 1,
            profile: PROFILE.into(),
            on_unknown: OnUnknown::Wait,
            talaria_version: "dev".into(),
            hermes: json!({}),
            roster: json!({}),
            approval_policy: json!({}),
        },
        "agent running\nagent finished\n",
    )
    .expect("create run");
    let journal_path = run.path().join("journal.jsonl");
    let file = std::fs::OpenOptions::new()
        .append(true)
        .open(&journal_path)
        .expect("open journal");
    let writer = JournalWriter::spawn(file, 1);
    append(
        &writer,
        Record::RunStarted {
            run_id: run.manifest().run_id.as_str().to_owned(),
            script_sha256: "a".repeat(64),
        },
    )
    .await;
    write_finished(&writer).await;
    write_running(&writer, &session, &admission, generation, epoch).await;

    let mut service = ProgressService::new(run.clone());
    let (tx, mut rx) = unbounded_channel();
    service.set_connection(Some(gateway.client.clone()), tx);
    let mut state = ProgressState::blank(run.manifest());
    service
        .poll_once(&mut state)
        .await
        .expect("read the journal");
    assert_eq!(
        state.progress.agents.len(),
        2,
        "expected one running and one finished agent"
    );
    state.select_agent(KEY_RUNNING);
    assert_eq!(
        state.progress.agent(KEY_RUNNING).map(|agent| agent.state),
        Some(AgentState::Running)
    );
    let attach = live::update(&mut state, ProgressInput::Connected(true));
    assert!(
        attach
            .iter()
            .any(|effect| matches!(effect, ProgressEffect::Attach { .. })),
        "connecting did not attach the running session"
    );
    settle(&mut service, &mut state, &mut rx, attach).await;
    assert!(state.connected);
    assert_eq!(
        state.progress.agent(KEY_RUNNING).map(|agent| agent.state),
        Some(AgentState::Running),
        "attach left the row {}",
        state
            .progress
            .agent(KEY_RUNNING)
            .map(|agent| agent.state.as_str())
            .unwrap_or("missing")
    );

    let steer_text = format!("On your next line print exactly {MARKER} and then continue.");
    assert!(live::update(&mut state, ProgressInput::Key(ProgressKey::Steer)).is_empty());
    assert_eq!(state.steer_input.as_deref(), Some(""));
    for ch in steer_text.chars() {
        assert!(live::update(&mut state, ProgressInput::Key(ProgressKey::Char(ch))).is_empty());
    }
    let steer = live::update(&mut state, ProgressInput::Key(ProgressKey::Enter));
    match steer.as_slice() {
        [
            ProgressEffect::Steer {
                session_id, text, ..
            },
        ] => {
            assert_eq!(session_id, &session);
            assert!(text.contains(MARKER));
        }
        _ => panic!("steer did not emit a steer effect"),
    }
    settle(&mut service, &mut state, &mut rx, steer).await;
    assert_eq!(
        state.footer.as_deref(),
        Some("steer queued for its next step"),
        "queued acknowledgement was {}",
        state.footer.as_deref().unwrap_or("absent")
    );
    // The queued sentence is only the acknowledgement. Delivery is later output.
    wait_for_delivery(&mut gateway, &session, &admission).await;

    let stop = live::update(&mut state, ProgressInput::Key(ProgressKey::StopAgent));
    assert!(
        matches!(stop.as_slice(), [ProgressEffect::Interrupt { .. }]),
        "stop did not emit an interrupt"
    );
    assert_eq!(
        state.progress.agent(KEY_RUNNING).map(|agent| agent.state),
        Some(AgentState::Stopping)
    );
    settle(&mut service, &mut state, &mut rx, stop).await;
    assert_eq!(state.footer.as_deref(), Some("stopping"));
    let receipt = wait_for_interrupt(&gateway.client, &session, &admission).await;
    let outcome = receipt["outcome"].as_str().unwrap_or("interrupted");
    append(
        &writer,
        Record::TurnSettled {
            key: KEY_RUNNING.into(),
            attempt: 0,
            turn: 1,
            admission_id: admission.clone(),
            outcome: if outcome == "failed" {
                "failed".into()
            } else {
                "interrupted".into()
            },
            final_text: None,
            final_text_ref: None,
            result_meta: receipt.get("result").cloned().unwrap_or_else(|| json!({})),
            usage: Some(receipt.get("usage").cloned().unwrap_or(Value::Null)),
        },
    )
    .await;
    append(
        &writer,
        Record::CallFinished {
            key: KEY_RUNNING.into(),
            attempt: 0,
            outcome: CallOutcome::Failed {
                reason: "interrupted".into(),
            },
            turns: 1,
        },
    )
    .await;
    service
        .poll_once(&mut state)
        .await
        .expect("read the settled journal");
    assert_eq!(
        state.footer.as_deref(),
        Some("stopped — restart (r) runs it again")
    );
    assert_eq!(
        state.progress.agent(KEY_RUNNING).map(|agent| agent.state),
        Some(AgentState::Failed)
    );
    assert_eq!(
        state.progress.agent(KEY_FINISHED).map(|agent| agent.state),
        Some(AgentState::Done)
    );

    let before = turn_intent_count(&journal_path);
    let open = live::update(&mut state, ProgressInput::Key(ProgressKey::OpenAgent));
    assert!(
        matches!(open.as_slice(), [ProgressEffect::OpenSession { .. }]),
        "open did not resume the session"
    );
    let opened = settle(&mut service, &mut state, &mut rx, open).await;
    assert!(opened, "open did not produce an opened session");
    assert!(state.opened_session.is_some());
    assert_eq!(
        turn_intent_count(&journal_path),
        before,
        "opening wrote a new turn intent"
    );
    gateway.client.close().await;
    println!("PASSED");
}
