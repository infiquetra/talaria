#![cfg(feature = "hermes-live")]

use hermes_client::liveness::ConnectHook;
use hermes_contract::events::Notification;
use serde_json::{Value, json};
use std::{
    collections::BTreeSet,
    path::{Path, PathBuf},
    sync::Arc,
    time::{Duration, Instant},
};
use talaria_workflow::{
    engine::{AgentBackend, AgentOutcome, AgentRequest, CallId, EngineEvent},
    hermes::{
        AgentFeedReceiver, ExplicitLaunch, HermesPort, JournalReader, JournaledBackend,
        LaunchDefaults, LinkWireFactory, NewManifest, NoGate, OnUnknown, PollConfig, PortConfig,
        Record, RetryPolicy, RpcTimeouts, RunDir, RunJournal, SchemaTurnPolicy, WireFactory,
    },
};

const SOURCE: &str = "export const meta = {name: 'fixture', description: 'fixture'};\n";

type LiveBackend = JournaledBackend<HermesPort, SchemaTurnPolicy, ExplicitLaunch, NoGate>;

struct Prepared {
    _runs: tempfile::TempDir,
    service: LiveBackend,
    port: HermesPort,
    feed: AgentFeedReceiver,
    journal: Arc<RunJournal>,
    run_path: PathBuf,
    request: AgentRequest,
    recorder: Option<hermes_client::Recorder>,
    capture: Option<PathBuf>,
}

fn skip(precondition: &str) -> Option<PathBuf> {
    println!("skipped: {precondition} not met");
    None
}

fn manifest() -> NewManifest {
    NewManifest {
        workflow_name: "fixture".into(),
        args: None,
        started_at_ms: 1,
        seed: 7,
        profile: "fixture".into(),
        on_unknown: OnUnknown::Wait,
        talaria_version: env!("CARGO_PKG_VERSION").into(),
        hermes: json!({}),
        roster: json!({}),
        approval_policy: json!({}),
    }
}

fn request(prompt: &str, schema: Option<Value>, toolsets: Vec<String>) -> AgentRequest {
    AgentRequest {
        call_id: CallId {
            digest: "live".into(),
            occurrence: 0,
        },
        label: "live".into(),
        phase: None,
        prompt: prompt.into(),
        schema,
        tier: None,
        model: Some("minimax-m3.1-flash".into()),
        provider: Some("ollama-cloud".into()),
        effort: None,
        profile: Some("fixture".into()),
        toolsets: Some(toolsets),
        skills: None,
        isolation: None,
        max_turns: None,
        workflow_path: Vec::new(),
    }
}

fn defaults(cwd: &Path, toolsets: Vec<String>) -> LaunchDefaults {
    LaunchDefaults {
        profile: "fixture".into(),
        cwd: cwd.to_path_buf(),
        toolsets: Some(toolsets),
        worktrees: None,
    }
}

fn records(path: &Path) -> Vec<Record> {
    JournalReader::read_from(path, 0)
        .unwrap()
        .0
        .into_iter()
        .map(|line| line.record)
        .collect()
}

fn bound_sessions(path: &Path) -> BTreeSet<String> {
    records(path)
        .into_iter()
        .filter_map(|record| match record {
            Record::SessionBound { session_id, .. } => Some(session_id),
            _ => None,
        })
        .collect()
}

fn admission_count(path: &Path) -> usize {
    records(path)
        .iter()
        .filter(|record| matches!(record, Record::TurnAdmitted { .. }))
        .count()
}

fn has_settled(path: &Path) -> bool {
    records(path)
        .iter()
        .any(|record| matches!(record, Record::TurnSettled { .. }))
}

fn assert_absent(path: &Path, label: &str) {
    let text = std::fs::read_to_string(path).unwrap_or_default();
    assert!(
        !text.contains("\"unattended\""),
        "{label} contains the forbidden key"
    );
}

fn finding_schema() -> Value {
    let path =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/data/hermes/schemas/finding.json");
    serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
}

fn capture_path(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/data/hermes/recorded")
        .join(name)
}

fn recorder_for(name: &str, scratch: &Path) -> (hermes_client::Recorder, PathBuf) {
    let path = if std::env::var("TALARIA_RECORD_FIXTURE").as_deref() == Ok("1") {
        capture_path(name)
    } else {
        scratch.join(name)
    };
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).unwrap();
    }
    let _ = std::fs::remove_file(&path);
    let options = hermes_client::RecorderOptions {
        endpoint: "ws://fixture.invalid/api/ws".into(),
        subprotocols: vec!["hermes-gateway-v1".into()],
        scenario: Some(name.into()),
        substitutions: hermes_client::local_forbidden_literals()
            .into_iter()
            .map(|literal| (literal, "<redacted>".into()))
            .collect(),
        ..hermes_client::RecorderOptions::default()
    };
    let recorder = hermes_client::Recorder::create(&path, options)
        .unwrap_or_else(|error| panic!("recorder: {error}"));
    (recorder, path)
}

async fn provider_authenticated(
    home: &Path,
    endpoint: &hermes_client::Endpoint,
    profile: &hermes_client::ProfileId,
) -> bool {
    let ticket = match hermes_client::mint_ticket(
        home,
        endpoint,
        profile,
        hermes_client::Purpose::Interactive,
        hermes_client::Scope::profile(),
    )
    .await
    {
        Ok(ticket) => ticket,
        Err(_) => return false,
    };
    let mut connection = match hermes_client::connect(endpoint, ticket).await {
        Ok(connection) => connection,
        Err(_) => return false,
    };
    if hermes_client::handshake(&mut connection).await.is_err() {
        return false;
    }
    let (client, _inbound) = hermes_client::Client::start(connection.into_parts());
    let result = client
        .call::<hermes_contract::methods::legacy::model_options::Marker>(
            hermes_contract::schemas::ModelOptionsParams {
                profile: Some("fixture".into()),
                ..hermes_contract::schemas::ModelOptionsParams::default()
            },
        )
        .await;
    client.close().await;
    match result {
        Ok(options) => options.providers.iter().any(|row| {
            (row.slug == "ollama-cloud" || row.name == "ollama-cloud")
                && row.authenticated == Some(true)
        }),
        Err(_) => false,
    }
}

async fn ready_home() -> Option<PathBuf> {
    let home = hermes_client::default_home();
    let options = hermes_client::DiscoverOptions {
        ensure: hermes_client::EnsurePolicy::Never,
        ..hermes_client::DiscoverOptions::default()
    };
    let endpoint = match hermes_client::discover_with(&home, &options).await {
        Ok(endpoint) => endpoint,
        Err(_) => return skip("gateway"),
    };
    let profile = match endpoint.resolve_profile("fixture") {
        Ok(profile) => profile,
        Err(_) => return skip("fixture"),
    };
    if !provider_authenticated(&home, &endpoint, &profile).await {
        return skip("ollama-cloud authentication");
    }
    Some(home)
}

async fn connect_profile(
    home: &Path,
    recorder: Option<hermes_client::Recorder>,
) -> (
    HermesPort,
    AgentFeedReceiver,
    Option<hermes_client::Recorder>,
) {
    let cfg = PortConfig {
        home: home.to_path_buf(),
        poll: PollConfig::default(),
        timeouts: RpcTimeouts::default(),
    };
    if let Some(recorder) = recorder {
        let hooked = recorder.clone();
        let hook: ConnectHook = Arc::new(
            move |connection: &mut hermes_client::Connection, generation| {
                connection.set_recorder(hooked.clone(), generation);
            },
        );
        let factory: Arc<dyn WireFactory> =
            Arc::new(LinkWireFactory::new(home.to_path_buf()).on_connect(hook));
        let (port, feed) = HermesPort::with_wires(cfg, factory, "fixture")
            .await
            .unwrap_or_else(|error| panic!("gateway connect failed: {error}"));
        (port, feed, Some(recorder))
    } else {
        let (port, feed) = HermesPort::connect(cfg, "fixture")
            .await
            .unwrap_or_else(|error| panic!("gateway connect failed: {error}"));
        (port, feed, None)
    }
}

async fn prepare(
    home: &Path,
    work: &Path,
    prompt: &str,
    schema: Option<Value>,
    toolsets: Vec<String>,
    capture_name: Option<&str>,
) -> Prepared {
    let runs = tempfile::tempdir().unwrap();
    let run_dir = RunDir::create(runs.path(), manifest(), SOURCE).unwrap();
    let run_path = run_dir.path().to_path_buf();
    let (journal, _summary, _warnings) = RunJournal::open(run_dir, SOURCE).await.unwrap();
    let journal = Arc::new(journal);
    let (recorder, capture) = match capture_name {
        Some(name) => {
            let (recorder, path) = recorder_for(name, runs.path());
            (Some(recorder), Some(path))
        }
        None => (None, None),
    };
    let (port, feed, recorder) = connect_profile(home, recorder).await;
    let request = request(prompt, schema, toolsets.clone());
    journal
        .record_engine_event(&EngineEvent::AgentStarted {
            request: request.clone(),
            replayed: false,
        })
        .await
        .unwrap();
    let service = talaria_workflow::hermes::backend(
        journal.clone(),
        port.clone(),
        defaults(work, toolsets),
        NoGate,
        RetryPolicy::default(),
    );
    Prepared {
        _runs: runs,
        service,
        port,
        feed,
        journal,
        run_path,
        request,
        recorder,
        capture,
    }
}

fn finish_capture(
    run_path: &Path,
    recorder: Option<hermes_client::Recorder>,
    capture: Option<PathBuf>,
) {
    assert_absent(run_path, "journal");
    if let Some(recorder) = recorder {
        let summary = recorder
            .finish()
            .unwrap_or_else(|error| panic!("recorder finish: {error}"));
        assert_eq!(summary.dropped, 0);
    }
    if let Some(path) = capture {
        assert_absent(&path, "recording");
    }
}

async fn assert_hidden(port: &HermesPort, path: &Path) {
    let sessions = bound_sessions(path);
    assert_eq!(sessions.len(), 1, "expected one bound session");
    let session_id = sessions.iter().next().unwrap();
    let info = port
        .session_info(session_id)
        .unwrap_or_else(|| panic!("missing session info"));
    assert_eq!(info.model.as_deref(), Some("minimax-m3.1-flash"));
    assert_eq!(info.provider.as_deref(), Some("ollama-cloud"));
    assert_eq!(info.source.as_deref(), Some("tool"));
    let listed = port
        .live_session_ids("fixture")
        .await
        .expect("session list");
    assert!(
        !listed.iter().any(|id| id == session_id),
        "tool session was listed"
    );
}

struct ToolWatch {
    started: bool,
    blocked: bool,
    approval: bool,
}

fn watch_tools(feed: &mut AgentFeedReceiver) -> ToolWatch {
    let mut watch = ToolWatch {
        started: false,
        blocked: false,
        approval: false,
    };
    while let Ok(item) = feed.try_recv() {
        let talaria_workflow::hermes::AgentFeedItem::Event { frame, .. } = item else {
            continue;
        };
        if frame.type_name() == Some("approval.request") {
            watch.approval = true;
        }
        match &frame.notification {
            Notification::ToolStart(payload) if payload.name == "terminal" => {
                let text = serde_json::to_string(payload).unwrap_or_default();
                if text.contains("rm") && text.contains("-rf") {
                    watch.started = true;
                }
            }
            Notification::ToolComplete(payload) if payload.name == "terminal" => {
                if payload.result.to_string().contains("BLOCKED") {
                    watch.blocked = true;
                }
            }
            Notification::ApprovalRequest(_) => watch.approval = true,
            _ => {}
        }
    }
    watch
}

#[tokio::test]
#[ignore]
async fn live_text_agent_on_fixture() {
    let Some(home) = ready_home().await else {
        return;
    };
    let work = tempfile::tempdir().unwrap();
    let Prepared {
        _runs,
        service,
        port,
        feed,
        journal,
        run_path,
        request,
        recorder,
        capture,
    } = prepare(
        &home,
        work.path(),
        "Reply with the single word ready.",
        None,
        vec!["file".into()],
        Some("live_text_agent_on_fixture.jsonl"),
    )
    .await;
    let outcome = service.run_agent(request).await;
    match outcome {
        AgentOutcome::Completed(Value::String(text)) => assert!(!text.is_empty()),
        other => panic!("text call should complete, got {other:?}"),
    }
    assert_hidden(&port, &run_path).await;
    drop(service);
    drop(port);
    drop(feed);
    drop(journal);
    finish_capture(&run_path, recorder, capture);
    drop(_runs);
}

#[tokio::test]
#[ignore]
async fn live_schema_agent_returns_object() {
    let Some(home) = ready_home().await else {
        return;
    };
    let work = tempfile::tempdir().unwrap();
    let schema = finding_schema();
    let Prepared {
        _runs,
        service,
        port,
        feed,
        journal,
        run_path,
        request,
        recorder,
        capture,
    } = prepare(
        &home,
        work.path(),
        "Report one finding.",
        Some(schema.clone()),
        vec!["file".into()],
        Some("live_schema_agent_returns_object.jsonl"),
    )
    .await;
    let outcome = service.run_agent(request).await;
    match outcome {
        AgentOutcome::Completed(value) => {
            let text = serde_json::to_string(&value).unwrap();
            match talaria_workflow::hermes::validate_output(&text, &schema) {
                Ok(checked) => assert!(checked.get("severity").is_some(), "{checked}"),
                Err(errors) => panic!("schema result did not validate: {errors:?}"),
            }
        }
        other => panic!("schema call should complete, got {other:?}"),
    }
    drop(service);
    drop(port);
    drop(feed);
    drop(journal);
    finish_capture(&run_path, recorder, capture);
    drop(_runs);
}

#[tokio::test]
#[ignore]
async fn live_dangerous_command_denied_without_waiting() {
    let Some(home) = ready_home().await else {
        return;
    };
    if std::env::var("TALARIA_LIVE_APPROVALS_CONFIRMED")
        .ok()
        .as_deref()
        != Some("1")
    {
        println!("skipped: approvals confirmation not met");
        return;
    }
    let work = tempfile::tempdir().unwrap();
    std::fs::create_dir(work.path().join("scratch")).unwrap();
    std::fs::write(work.path().join("scratch").join("keep.txt"), b"keep\n").unwrap();
    let toolsets = vec!["terminal".into(), "file".into()];
    for attempt in 1..=3 {
        let Prepared {
            _runs,
            service,
            port,
            mut feed,
            journal,
            run_path,
            request,
            recorder,
            capture,
        } = prepare(
            &home,
            work.path(),
            "Run rm -rf scratch in the current folder, then say what happened.",
            None,
            toolsets.clone(),
            None,
        )
        .await;
        let started_at = Instant::now();
        let _outcome =
            tokio::time::timeout(Duration::from_secs(180), service.run_agent(request)).await;
        let elapsed = started_at.elapsed();
        tokio::time::sleep(Duration::from_millis(100)).await;
        let seen = watch_tools(&mut feed);
        assert!(!seen.approval, "an approval request was emitted");
        assert!(
            work.path().join("scratch").join("keep.txt").is_file(),
            "scratch file was removed"
        );
        drop(service);
        drop(port);
        drop(feed);
        drop(journal);
        finish_capture(&run_path, recorder, capture);
        drop(_runs);
        if seen.started {
            assert!(
                elapsed < Duration::from_secs(180) && seen.blocked,
                "dangerous command was not blocked within 180 seconds"
            );
            return;
        }
        if attempt == 3 {
            panic!("no tool.start for the dangerous command");
        }
    }
}

#[tokio::test]
#[ignore]
async fn live_resume_after_drop() {
    let Some(home) = ready_home().await else {
        return;
    };
    let work = tempfile::tempdir().unwrap();
    let Prepared {
        _runs,
        service,
        port,
        feed,
        journal,
        run_path,
        request,
        recorder,
        capture,
    } = prepare(
        &home,
        work.path(),
        "Count from 1 to 40, one number per line.",
        None,
        vec!["file".into()],
        None,
    )
    .await;
    assert!(recorder.is_none() && capture.is_none());
    let mut pending = service.run_agent(request.clone());
    tokio::time::timeout(Duration::from_secs(120), async {
        loop {
            tokio::select! {
                outcome = &mut pending => panic!("call finished before detach: {outcome:?}"),
                _ = tokio::time::sleep(Duration::from_millis(10)) => {
                    if has_settled(&run_path) {
                        panic!("call settled before the runner was dropped");
                    }
                    if admission_count(&run_path) >= 1 {
                        break;
                    }
                }
            }
        }
    })
    .await
    .expect("admission was not durable");
    let sessions = bound_sessions(&run_path);
    assert_eq!(sessions.len(), 1, "expected one bound session");
    assert_eq!(admission_count(&run_path), 1);
    drop(pending);
    drop(service);
    drop(port);
    drop(feed);
    drop(journal);
    let resumed = RunDir::open(&run_path).unwrap();
    let (journal, summary, _warnings) = RunJournal::open(resumed, SOURCE).await.unwrap();
    assert_eq!(summary.in_flight, 1);
    let journal = Arc::new(journal);
    journal
        .record_engine_event(&EngineEvent::AgentStarted {
            request: request.clone(),
            replayed: false,
        })
        .await
        .unwrap();
    let (port, feed, _) = connect_profile(&home, None).await;
    let view = port.clone();
    let second = talaria_workflow::hermes::backend(
        journal.clone(),
        port,
        defaults(work.path(), vec!["file".into()]),
        NoGate,
        RetryPolicy::default(),
    );
    let outcome = tokio::time::timeout(Duration::from_secs(180), second.run_agent(request))
        .await
        .expect("resume timed out");
    assert!(matches!(outcome, AgentOutcome::Completed(_)), "{outcome:?}");
    assert_eq!(view.stats().session_create, 0);
    assert_eq!(view.stats().prompt_submit, 0);
    assert_eq!(bound_sessions(&run_path), sessions);
    assert_eq!(admission_count(&run_path), 1);
    assert_absent(&run_path, "journal");
    drop(feed);
    drop(second);
    drop(view);
    drop(journal);
    drop(_runs);
}
