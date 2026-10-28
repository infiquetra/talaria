#![cfg(feature = "fake-gateway")]
mod support;
use serde_json::json;
use support::harness::*;
use talaria_workflow::{
    engine::{CallId, EngineEvent},
    journal::{AdmissionState, CallOutcome, Record, RunJournal},
};

#[tokio::test(flavor = "current_thread")]
#[ignore]
async fn open_10k_records_under_budget() {
    let (_temp, dir) = new_dir();
    let mut records = vec![start_record(&dir)];
    for i in 0..1000 {
        let key = format!("{i:032x}");
        let call_id = CallId {
            digest: format!("{i:064x}"),
            occurrence: 0,
        };
        records.extend([
            Record::CallIntent {
                key: key.clone(),
                attempt: 0,
                call_id,
                label: "review".into(),
                phase: None,
                workflow_path: vec![],
                launch: launch(),
                request_id: format!("request-{i}"),
            },
            Record::SessionBound {
                key: key.clone(),
                attempt: 0,
                session_id: format!("session-{i}"),
                stored_session_id: format!("session-{i}"),
            },
            Record::TurnIntent {
                key: key.clone(),
                attempt: 0,
                turn: 0,
                submission_id: format!("submission-{i}"),
                text: Some("x".repeat(2048)),
                text_ref: None,
            },
            Record::TurnAdmitted {
                key: key.clone(),
                attempt: 0,
                turn: 0,
                admission_id: format!("admission-{i}"),
                status: AdmissionState::Terminal,
                outcome: Some("completed".into()),
                execution_generation: Some(1),
                authority_epoch: 0,
            },
            Record::TurnSettled {
                key: key.clone(),
                attempt: 0,
                turn: 0,
                admission_id: format!("admission-{i}"),
                outcome: "completed".into(),
                final_text: Some("x".repeat(4096)),
                final_text_ref: None,
                result_meta: json!({"completed":true}),
                usage: Some(json!({})),
            },
            Record::CallFinished {
                key,
                attempt: 0,
                outcome: CallOutcome::Completed {
                    value: Some(json!("x".repeat(4096))),
                    value_ref: None,
                },
                turns: 1,
            },
        ]);
        for _ in 0..4 {
            records.push(Record::Engine {
                event: Some(EngineEvent::Log {
                    message: "sample".into(),
                    workflow_path: vec![],
                }),
                event_ref: None,
            });
        }
    }
    write_records(&dir, &records);
    let (warm, _, _) = RunJournal::open(dir.clone(), SCRIPT).await.unwrap();
    drop(warm);
    let start = std::time::Instant::now();
    let (journal, _, _) = RunJournal::open(dir, SCRIPT).await.unwrap();
    let elapsed = start.elapsed();
    drop(journal);
    println!("open_10k_ms={}", elapsed.as_millis());
    assert!(elapsed.as_millis() < 250);
}
