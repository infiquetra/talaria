#![cfg(feature = "fake-gateway")]
mod support;
use serde_json::json;
use std::sync::Arc;
use support::harness::*;
use talaria_workflow::{
    engine::{AgentBackend, AgentOutcome, EngineEvent, RunOutcomeKind},
    journal::{fake_gateway::FakeGateway, *},
};

fn history(
    key: &str,
    request: &talaria_workflow::engine::AgentRequest,
    launch: ResolvedLaunch,
    stage: u8,
) -> Vec<Record> {
    let mut records = vec![
        Record::CallIntent {
            key: key.into(),
            attempt: 0,
            call_id: request.call_id.clone(),
            label: request.label.clone(),
            phase: None,
            workflow_path: request.workflow_path.clone(),
            launch,
            request_id: "old-request".into(),
        },
        Record::SessionBound {
            key: key.into(),
            attempt: 0,
            session_id: "old-session".into(),
            stored_session_id: "old-session".into(),
        },
        Record::TurnIntent {
            key: key.into(),
            attempt: 0,
            turn: 0,
            submission_id: "old-submission".into(),
            text: Some(request.prompt.clone()),
            text_ref: None,
        },
        Record::TurnAdmitted {
            key: key.into(),
            attempt: 0,
            turn: 0,
            admission_id: format!("old-admission-{key}"),
            status: AdmissionState::Terminal,
            outcome: Some("interrupted".into()),
            execution_generation: Some(1),
            authority_epoch: 0,
        },
    ];
    if stage >= 1 {
        records.push(Record::TurnSettled {
            key: key.into(),
            attempt: 0,
            turn: 0,
            admission_id: format!("old-admission-{key}"),
            outcome: "interrupted".into(),
            final_text: None,
            final_text_ref: None,
            result_meta: json!({}),
            usage: Some(json!({"input_tokens":7})),
        });
    }
    if stage >= 2 {
        records.push(Record::CallFinished {
            key: key.into(),
            attempt: 0,
            turns: 1,
            outcome: if stage == 2 {
                CallOutcome::Failed {
                    reason: "interrupted".into(),
                }
            } else {
                CallOutcome::Completed {
                    value: Some(json!("cached")),
                    value_ref: None,
                }
            },
        });
    }
    records
}
async fn exercise(stage: u8, crash: bool) {
    let (_temp, dir) = new_dir();
    let request = request();
    let resolver = TestResolver::default();
    let launch = resolver.resolve(&request).unwrap();
    let key = journal_key(&request.call_id, &launch);
    let completed = "c".repeat(32);
    let skipped = "d".repeat(32);
    let mut records = vec![start_record(&dir)];
    records.extend(history(&key, &request, launch.clone(), stage));
    records.extend(history(&completed, &request, launch, 3));
    records.push(Record::CallFinished {
        key: skipped.clone(),
        attempt: 0,
        outcome: CallOutcome::Skipped,
        turns: 0,
    });
    write_records(&dir, &records);
    let journal = RunJournal::open(dir.clone(), SCRIPT).await.unwrap().0;
    let keys = vec![
        key.clone(),
        completed.clone(),
        skipped.clone(),
        "e".repeat(32),
    ];
    journal.request_restart_keys(&keys).await.unwrap();
    journal.request_restart_keys(&keys).await.unwrap();
    let journal = if crash {
        drop(journal);
        RunJournal::open(dir.clone(), SCRIPT).await.unwrap().0
    } else {
        journal
    };
    let journal = Arc::new(journal);
    journal
        .record_engine_event(&EngineEvent::AgentStarted {
            request: request.clone(),
            replayed: false,
        })
        .await
        .unwrap();
    let fake = FakeGateway::new();
    let backend = JournaledBackend::new(
        journal.clone(),
        fake.clone(),
        TestPolicy,
        resolver,
        NoGate,
        RetryPolicy::default(),
    );
    assert_eq!(
        backend.run_agent(request).await,
        AgentOutcome::Completed(json!("ok"))
    );
    assert_eq!((fake.stats().sessions, fake.stats().admissions), (1, 1));
    let records = JournalReader::read_from(dir.path(), 0).unwrap().0;
    assert_eq!(records.iter().filter(|r|matches!(&r.record,Record::CallIntent{key:k,attempt:1,request_id,..}if k==&key&&request_id.ends_with(":a1"))).count(),1);
    for cached in [&completed, &skipped] {
        assert_eq!(
            records
                .iter()
                .filter(|r| r
                    .record
                    .identity()
                    .is_some_and(|(k, a)| k == cached && a > 0))
                .count(),
            0
        );
    }
    if stage >= 1 {
        assert_eq!(
            journal
                .summary()
                .settled_usage
                .iter()
                .filter(|u| u.key == key && u.attempt == 0)
                .count(),
            1
        );
    }
    journal.finish(RunOutcomeKind::Completed).await.unwrap();
    assert!(matches!(
        journal.request_restart_keys(&keys).await,
        Err(JournalError::Closed)
    ));
    drop(backend);
    drop(journal);
    RunJournal::open(dir, SCRIPT).await.unwrap();
}
#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn explicit_restarts_advance_unfinished_calls_once() {
    for stage in 0..=2 {
        exercise(stage, false).await;
    }
}
#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn restart_request_survives_a_crash_before_the_next_call() {
    exercise(0, true).await;
}
