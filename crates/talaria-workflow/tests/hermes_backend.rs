#![cfg(feature = "mock")]
#[path = "hermes_common/mod.rs"]
mod common;

use serde_json::{Value, json};
use std::{collections::BTreeSet, path::PathBuf, sync::Arc, time::Duration};
use talaria_workflow::{
    engine::{self, AgentBackend, AgentOutcome, EngineEvent, MapResolver, RunOutcome},
    hermes::{
        ExplicitLaunch, HermesPort, JournalReader, NoGate, PollConfig, PortConfig, Record,
        RetryPolicy, RpcTimeouts, RunDir, RunJournal, mock_wire::ScriptedWire,
    },
};

const META: &str = "export const meta = {name: 'fixture', description: 'fixture'};\n";

fn cfg() -> PortConfig {
    PortConfig {
        home: PathBuf::from("/work/proj"),
        poll: PollConfig::default(),
        timeouts: RpcTimeouts::default(),
    }
}

#[tokio::test]
async fn text_and_schema_calls_end_to_end() {
    let dir = tempfile::tempdir().unwrap();
    let run_dir = RunDir::create(dir.path(), common::manifest(), META).unwrap();
    let (journal, _summary, _warnings) = RunJournal::open(run_dir, META).await.unwrap();
    let journal = Arc::new(journal);
    let mut steps = common::read_json("scripts/text_ok.json")["steps"]
        .as_array()
        .unwrap()
        .clone();
    steps.pop();
    let mut schema_steps = common::read_json("scripts/schema_correction.json")["steps"]
        .as_array()
        .unwrap()
        .clone();
    schema_steps.remove(0);
    schema_steps.pop();
    steps.extend(schema_steps);
    let wire =
        Arc::new(ScriptedWire::from_value(&common::data(""), json!({"steps": steps})).unwrap());
    let (port, _feed) = HermesPort::with_wires(cfg(), wire.clone(), "fixture")
        .await
        .unwrap();
    let service = talaria_workflow::hermes::backend(
        journal.clone(),
        port,
        common::defaults(dir.path().to_path_buf()),
        NoGate,
        RetryPolicy::default(),
    );
    let text_request = common::request("Say ready");
    journal
        .record_engine_event(&EngineEvent::AgentStarted {
            request: text_request.clone(),
            replayed: false,
        })
        .await
        .unwrap();
    let text = service.run_agent(text_request).await;
    let mut schema_request = common::request("Report one finding.");
    schema_request.call_id.occurrence = 1;
    schema_request.schema = Some(common::read_json("schemas/finding.json"));
    journal
        .record_engine_event(&EngineEvent::AgentStarted {
            request: schema_request.clone(),
            replayed: false,
        })
        .await
        .unwrap();
    let schema = service.run_agent(schema_request).await;
    match text {
        AgentOutcome::Completed(Value::String(text)) => assert!(!text.is_empty()),
        other => panic!("text call should complete, got {other:?}"),
    }
    match schema {
        AgentOutcome::Completed(value) => {
            assert!(value.get("severity").is_some(), "{value}");
        }
        other => panic!("schema call should complete, got {other:?}"),
    }
    let submits: Vec<_> = wire
        .outbound()
        .into_iter()
        .filter(|frame| frame.method == "prompt.submit")
        .collect();
    assert_eq!(submits.len(), 3);
    let ids: Vec<_> = submits
        .iter()
        .filter_map(|frame| frame.params.get("submission_id").and_then(|v| v.as_str()))
        .collect();
    assert!(ids.iter().any(|id| id.ends_with(":0")), "{ids:?}");
    assert!(ids.iter().any(|id| id.ends_with(":1")), "{ids:?}");
}

#[tokio::test]
async fn resume_after_kill_reads_receipt_only() {
    let dir = tempfile::tempdir().unwrap();
    let run_dir = RunDir::create(dir.path(), common::manifest(), META).unwrap();
    let run_path = run_dir.path().to_path_buf();
    let (journal, _summary, _warnings) = RunJournal::open(run_dir, META).await.unwrap();
    let journal = Arc::new(journal);
    let request = common::request("Count");
    journal
        .record_engine_event(&EngineEvent::AgentStarted {
            request: request.clone(),
            replayed: false,
        })
        .await
        .unwrap();
    let wire = Arc::new(
        ScriptedWire::from_value(
            &common::data(""),
            json!({"steps":[
                {"expect":"runtime.describe","reply":"describe_ok.json"},
                {"expect":"session.create","reply":"create_ok.json"},
                {"expect":"prompt.submit","reply":"submit_ok.json"}
            ]}),
        )
        .unwrap(),
    );
    let (port, _feed) = HermesPort::with_wires(cfg(), wire, "fixture")
        .await
        .unwrap();
    let first = talaria_workflow::hermes::backend(
        journal.clone(),
        port,
        common::defaults(dir.path().to_path_buf()),
        NoGate,
        RetryPolicy::default(),
    );
    let mut pending = first.run_agent(request.clone());
    tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            tokio::select! {
                outcome = &mut pending => panic!("call finished before detach: {outcome:?}"),
                _ = tokio::time::sleep(Duration::from_millis(10)) => {
                    let admitted = JournalReader::read_from(&run_path, 0)
                        .unwrap()
                        .0
                        .iter()
                        .any(|line| matches!(line.record, Record::TurnAdmitted { .. }));
                    if admitted {
                        break;
                    }
                }
            }
        }
    })
    .await
    .unwrap();
    drop(pending);
    drop(first);
    drop(journal);
    let resumed = RunDir::open(&run_path).unwrap();
    let (journal, summary, _warnings) = RunJournal::open(resumed, META).await.unwrap();
    assert_eq!(summary.in_flight, 1);
    journal
        .record_engine_event(&EngineEvent::AgentStarted {
            request: request.clone(),
            replayed: false,
        })
        .await
        .unwrap();
    let wire = Arc::new(ScriptedWire::load(&common::data("scripts/resume_admitted.json")).unwrap());
    let (port, _feed) = HermesPort::with_wires(cfg(), wire.clone(), "fixture")
        .await
        .unwrap();
    let second = talaria_workflow::hermes::backend(
        Arc::new(journal),
        port,
        common::defaults(dir.path().to_path_buf()),
        NoGate,
        RetryPolicy::default(),
    );
    let outcome = second.run_agent(request).await;
    assert!(matches!(outcome, AgentOutcome::Completed(_)));
    let frames = wire.outbound();
    assert_eq!(
        frames
            .iter()
            .filter(|frame| frame.method == "session.create")
            .count(),
        0,
        "resume must not create"
    );
    assert_eq!(
        frames
            .iter()
            .filter(|frame| frame.method == "prompt.submit")
            .count(),
        0,
        "resume must not submit"
    );
    assert!(
        frames.iter().any(|frame| frame.method == "prompt.receipt"),
        "resume must read the receipt"
    );
}

#[tokio::test]
async fn ten_parallel_calls_through_engine() {
    let dir = tempfile::tempdir().unwrap();
    let source = format!(
        "{META}return await parallel([0,1,2,3,4,5,6,7,8,9].map(i => () => agent('agent ' + i, {{model:'scripted-model',provider:'scripted',toolsets:['file']}})));"
    );
    let run_dir = RunDir::create(dir.path(), common::manifest(), &source).unwrap();
    let (journal, _summary, _warnings) = RunJournal::open(run_dir, &source).await.unwrap();
    let journal = Arc::new(journal);
    let wire = Arc::new(ScriptedWire::load(&common::data("scripts/ten_parallel.json")).unwrap());
    let (port, _feed) = HermesPort::with_wires(cfg(), wire.clone(), "fixture")
        .await
        .unwrap();
    let defaults = common::defaults(dir.path().to_path_buf());
    let resolver = ExplicitLaunch::new(defaults.clone());
    let config = journal.engine_config(&resolver).unwrap();
    let service = talaria_workflow::hermes::backend(
        journal.clone(),
        port,
        defaults,
        NoGate,
        RetryPolicy::default(),
    );
    let backend: Arc<dyn AgentBackend> = Arc::new(service);
    let handle = engine::spawn(
        engine::parse_script(&source).unwrap(),
        config,
        backend,
        Arc::new(MapResolver::new()),
    );
    let events = journal.forward_events(handle.events);
    let outcome = tokio::time::timeout(Duration::from_secs(30), handle.outcome)
        .await
        .expect("spawn outcome")
        .expect("worker finished");
    events.await.unwrap().unwrap();
    match outcome {
        RunOutcome::Completed { result } => {
            assert_eq!(
                result,
                json!(["0", "1", "2", "3", "4", "5", "6", "7", "8", "9"])
            );
        }
        other => panic!("expected ten results, got {other:?}"),
    }
    let creates: Vec<_> = wire
        .outbound()
        .into_iter()
        .filter(|frame| frame.method == "session.create")
        .collect();
    assert_eq!(creates.len(), 10);
    let mut ids = BTreeSet::new();
    for frame in &creates {
        assert!(!frame.params.contains_key("profile"));
        ids.insert(frame.params["request_id"].as_str().unwrap().to_owned());
    }
    assert_eq!(ids.len(), 10);
}
