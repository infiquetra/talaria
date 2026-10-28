#![cfg(feature = "fake-gateway")]
mod support;
use serde_json::json;
use support::harness::*;
use talaria_workflow::{
    engine::{EngineEvent, RunOutcomeKind},
    journal::{
        AdmissionState, AttentionCause, CallOutcome, FinishSummary, JournalError, Record,
        RecordLine, Resolution, ResolutionSource, RunId, RunJournal,
    },
};

#[test]
fn round_trip_every_record_type() {
    let id = RunId::parse("01JB2X9Q4M8Z7K3T5V6W8Y0A1C").unwrap();
    let k = key();
    let records = vec![
        Record::RunStarted {
            run_id: id.as_str().into(),
            script_sha256: "b".repeat(64),
        },
        Record::RunResumed {
            script_sha256: "b".repeat(64),
            resume_count: 1,
        },
        Record::Engine {
            event: Some(EngineEvent::Log {
                message: "hello".into(),
                workflow_path: vec![],
            }),
            event_ref: None,
        },
        Record::CallIntent {
            key: k.clone(),
            attempt: 0,
            call_id: call_id(),
            label: "review".into(),
            phase: Some("Review".into()),
            workflow_path: vec![],
            launch: launch(),
            request_id: "wf:fixture".into(),
        },
        Record::SessionBound {
            key: k.clone(),
            attempt: 0,
            session_id: "session".into(),
            stored_session_id: "session".into(),
        },
        Record::TurnIntent {
            key: k.clone(),
            attempt: 0,
            turn: 0,
            submission_id: "submission".into(),
            text: Some("hello".into()),
            text_ref: None,
        },
        Record::TurnAdmitted {
            key: k.clone(),
            attempt: 0,
            turn: 0,
            admission_id: "admission".into(),
            status: AdmissionState::Started,
            outcome: None,
            execution_generation: Some(1),
            authority_epoch: 1,
        },
        Record::TurnSettled {
            key: k.clone(),
            attempt: 0,
            turn: 0,
            admission_id: "admission".into(),
            outcome: "completed".into(),
            final_text: Some("ok".into()),
            final_text_ref: None,
            result_meta: json!({"completed":true}),
            usage: Some(json!({"total_tokens":3})),
        },
        Record::CallAttention {
            key: k.clone(),
            attempt: 0,
            episode: 0,
            cause: AttentionCause::Unknown,
            turn: Some(0),
            admission_id: Some("admission".into()),
            execution_generation: Some(1),
            detail: "unknown".into(),
        },
        Record::CallResolved {
            key: k.clone(),
            attempt: 0,
            episode: 0,
            resolution: Resolution::Fail,
            source: ResolutionSource::Operator,
        },
        Record::CallFinished {
            key: k,
            attempt: 0,
            outcome: CallOutcome::Completed {
                value: Some(json!("ok")),
                value_ref: None,
            },
            turns: 1,
        },
        Record::RunFinished {
            outcome: RunOutcomeKind::Completed,
            summary: FinishSummary {
                completed: 1,
                failed: 0,
                skipped: 0,
                parked: 0,
                unfinished: 0,
            },
        },
    ];
    for (i, record) in records.iter().enumerate() {
        let original = line(i as u64 + 1, record.clone());
        let encoded = serde_json::to_string(&original).unwrap();
        assert_eq!(
            serde_json::from_str::<RecordLine>(&encoded).unwrap(),
            original
        );
    }
    for (i, golden) in [
        (3, include_str!("journal_golden/call_intent.jsonl")),
        (7, include_str!("journal_golden/turn_settled.jsonl")),
        (11, include_str!("journal_golden/run_finished.jsonl")),
    ] {
        assert_eq!(
            serde_json::to_string(&line(i as u64 + 1, records[i].clone())).unwrap(),
            golden.trim_end()
        );
    }
}

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn unknown_type_is_corrupt() {
    let (_temp, dir) = new_dir();
    write_records(
        &dir,
        &[
            start_record(&dir),
            Record::Engine {
                event: Some(EngineEvent::Log {
                    message: "ok".into(),
                    workflow_path: vec![],
                }),
                event_ref: None,
            },
        ],
    );
    use std::io::Write;
    writeln!(
        std::fs::OpenOptions::new()
            .append(true)
            .open(records_path(&dir))
            .unwrap(),
        "{{\"v\":1,\"seq\":3,\"at_ms\":0,\"t\":\"mystery\"}}"
    )
    .unwrap();
    assert!(matches!(
        RunJournal::open(dir, SCRIPT).await,
        Err(JournalError::Corrupt { line: 3, .. })
    ));
}

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn every_allowed_transition_round_trips() {
    use futures_util::future::BoxFuture;
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };
    use talaria_workflow::{
        engine::{AgentRequest, BudgetSnapshot},
        journal::{AdmissionGate, GateDecision, GateReason, ResolvedLaunch},
    };
    #[derive(Clone)]
    struct SkipGate(Arc<AtomicUsize>);
    impl AdmissionGate for SkipGate {
        fn acquire(
            &self,
            _: &AgentRequest,
            _: &ResolvedLaunch,
            _: GateReason,
        ) -> BoxFuture<'static, GateDecision> {
            self.0.fetch_add(1, Ordering::SeqCst);
            Box::pin(async { GateDecision::Skip })
        }
        fn budget(&self) -> BudgetSnapshot {
            BudgetSnapshot::default()
        }
    }
    fn intent(attempt: u32) -> Record {
        Record::CallIntent {
            key: key(),
            attempt,
            call_id: call_id(),
            label: "review".into(),
            phase: None,
            workflow_path: vec![],
            launch: launch(),
            request_id: format!("request-{attempt}"),
        }
    }
    fn bound() -> Record {
        Record::SessionBound {
            key: key(),
            attempt: 0,
            session_id: "session".into(),
            stored_session_id: "session".into(),
        }
    }
    fn turn(t: u32) -> Record {
        Record::TurnIntent {
            key: key(),
            attempt: 0,
            turn: t,
            submission_id: format!("turn-{t}"),
            text: Some("prompt".into()),
            text_ref: None,
        }
    }
    fn admitted() -> Record {
        Record::TurnAdmitted {
            key: key(),
            attempt: 0,
            turn: 0,
            admission_id: "admission".into(),
            status: AdmissionState::Started,
            outcome: None,
            execution_generation: Some(1),
            authority_epoch: 0,
        }
    }
    fn settled() -> Record {
        Record::TurnSettled {
            key: key(),
            attempt: 0,
            turn: 0,
            admission_id: "admission".into(),
            outcome: "completed".into(),
            final_text: Some("ok".into()),
            final_text_ref: None,
            result_meta: json!({}),
            usage: Some(json!({})),
        }
    }
    fn attention(episode: u32) -> Record {
        Record::CallAttention {
            key: key(),
            attempt: 0,
            episode,
            cause: AttentionCause::Unknown,
            turn: Some(0),
            admission_id: Some("admission".into()),
            execution_generation: Some(1),
            detail: "unknown".into(),
        }
    }
    fn resolved(resolution: Resolution) -> Record {
        Record::CallResolved {
            key: key(),
            attempt: 0,
            episode: 0,
            resolution,
            source: ResolutionSource::Operator,
        }
    }
    fn failed() -> Record {
        Record::CallFinished {
            key: key(),
            attempt: 0,
            outcome: CallOutcome::Failed {
                reason: "refused".into(),
            },
            turns: 0,
        }
    }
    fn skipped() -> Record {
        Record::CallFinished {
            key: key(),
            attempt: 0,
            outcome: CallOutcome::Skipped,
            turns: 0,
        }
    }
    fn completed() -> Record {
        Record::CallFinished {
            key: key(),
            attempt: 0,
            outcome: CallOutcome::Completed {
                value: Some(json!("ok")),
                value_ref: None,
            },
            turns: 1,
        }
    }
    let valid = vec![
        vec![skipped()],
        vec![failed()],
        vec![intent(0), failed()],
        vec![intent(0), bound(), failed()],
        vec![intent(0), bound(), turn(0), failed()],
        vec![intent(0), bound(), turn(0), admitted(), failed()],
        vec![
            intent(0),
            bound(),
            turn(0),
            admitted(),
            attention(0),
            resolved(Resolution::Fail),
            settled(),
            completed(),
        ],
        vec![
            intent(0),
            bound(),
            turn(0),
            admitted(),
            attention(0),
            resolved(Resolution::Fail),
            attention(1),
        ],
        vec![
            intent(0),
            bound(),
            turn(0),
            admitted(),
            attention(0),
            resolved(Resolution::Abort),
            intent(1),
        ],
        vec![intent(0), bound(), turn(0), admitted(), settled(), turn(1)],
    ];
    for chain in valid {
        let (_temp, dir) = new_dir();
        let mut records = vec![start_record(&dir)];
        records.extend(chain);
        write_records(&dir, &records);
        assert!(RunJournal::open(dir, SCRIPT).await.is_ok());
    }
    let invalid = vec![
        vec![admitted()],
        vec![intent(0), admitted()],
        vec![intent(0), bound(), intent(0)],
        vec![intent(0), bound(), turn(0), turn(1)],
        vec![intent(0), bound(), turn(0), admitted(), turn(1)],
        vec![
            intent(0),
            bound(),
            turn(0),
            admitted(),
            settled(),
            admitted(),
        ],
        vec![
            intent(0),
            bound(),
            turn(0),
            admitted(),
            attention(0),
            completed(),
        ],
        vec![
            intent(0),
            bound(),
            turn(0),
            admitted(),
            attention(0),
            resolved(Resolution::Fail),
            bound(),
        ],
        vec![failed(), bound()],
        vec![
            intent(0),
            bound(),
            turn(0),
            admitted(),
            settled(),
            completed(),
            intent(1),
        ],
        vec![skipped(), intent(1)],
    ];
    for chain in invalid {
        let (_temp, dir) = new_dir();
        let mut records = vec![start_record(&dir)];
        records.extend(chain);
        let bad_line = records.len() as u64;
        write_records(&dir, &records);
        assert!(
            matches!(RunJournal::open(dir,SCRIPT).await,Err(JournalError::Corrupt{line,..}) if line==bad_line)
        );
    }

    let script = source("return await agent('skip');");
    let harness = Harness::new(&script);
    let count = Arc::new(AtomicUsize::new(0));
    let result = harness.run_with(&script, SkipGate(count.clone())).await;
    assert_eq!(support::harness::result(result.outcome), json!(null));
    let path = harness.path().join("journal.jsonl");
    let text = std::fs::read_to_string(&path).unwrap();
    let mut lines = Vec::new();
    for line in text.lines() {
        lines.push(line);
        let record: serde_json::Value = serde_json::from_str(line).unwrap();
        if record["t"] == "call_finished" {
            break;
        }
    }
    std::fs::write(&path, format!("{}\n", lines.join("\n"))).unwrap();
    count.store(0, Ordering::SeqCst);
    let result = harness.run_with(&script, SkipGate(count.clone())).await;
    assert_eq!(support::harness::result(result.outcome), json!(null));
    assert_eq!(count.load(Ordering::SeqCst), 0);
    assert_eq!(harness.gateway.stats().sessions, 0);
}
