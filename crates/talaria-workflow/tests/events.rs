#![cfg(feature = "mock")]
mod common;
use common::*;
use serde_json::json;
use std::sync::Arc;
use talaria_workflow::engine::{
    self, CallId, EngineEvent, MapResolver, OutcomeKind, RunOutcome, RunOutcomeKind,
};
use tokio_util::sync::CancellationToken;

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn events_arrive_in_order() {
    let backend = default_backend();
    let (outcome, events) = run(
        "phase('Review'); log('hello'); await agent('first'); await agent('second'); return 42;",
        backend.clone(),
    )
    .await;
    assert_eq!(result(outcome), json!(42));
    assert_eq!(events.len(), 8);
    assert!(matches!(&events[0], EngineEvent::Started { .. }));
    assert!(
        matches!(&events[1], EngineEvent::Phase {title, workflow_path} if title == "Review" && workflow_path.is_empty())
    );
    assert!(
        matches!(&events[2], EngineEvent::Log {message, workflow_path} if message == "hello" && workflow_path.is_empty())
    );
    for (start, finish, prompt, seq) in [(3, 4, "first", 0), (5, 6, "second", 1)] {
        let request = match &events[start] {
            EngineEvent::AgentStarted {
                request,
                replayed: false,
            } => request,
            other => panic!("{other:?}"),
        };
        assert_eq!(request.prompt, prompt);
        assert!(
            matches!(&events[finish], EngineEvent::AgentFinished {call_id,completion_seq:Some(n),outcome:OutcomeKind::Completed} if *call_id == request.call_id && *n == seq)
        );
    }
    assert!(matches!(
        events.last(),
        Some(EngineEvent::Finished {
            outcome: RunOutcomeKind::Completed
        })
    ));
    assert_eq!(
        events
            .iter()
            .filter(|e| matches!(e, EngineEvent::Finished { .. }))
            .count(),
        1
    );
    let parsed = engine::parse_script(&source("return await agent('still runs');")).unwrap();
    let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
    drop(rx);
    let outcome = engine::run(
        parsed,
        config(),
        backend,
        Arc::new(MapResolver::new()),
        tx,
        CancellationToken::new(),
    )
    .await;
    assert_eq!(
        outcome,
        RunOutcome::Completed {
            result: json!("ok")
        }
    );
}

#[test]
fn event_json_goldens() {
    assert_eq!(config().run_id, "fixture");
    let id = |occurrence| CallId {
        digest: "D".into(),
        occurrence,
    };
    let events = [
        EngineEvent::AgentFinished {
            call_id: id(0),
            completion_seq: Some(3),
            outcome: OutcomeKind::Completed,
        },
        EngineEvent::AgentFinished {
            call_id: id(1),
            completion_seq: Some(4),
            outcome: OutcomeKind::Failed {
                reason: "provider error".into(),
            },
        },
        EngineEvent::AgentFinished {
            call_id: id(2),
            completion_seq: Some(5),
            outcome: OutcomeKind::Skipped,
        },
        EngineEvent::AgentFinished {
            call_id: id(3),
            completion_seq: None,
            outcome: OutcomeKind::Aborted,
        },
        EngineEvent::AgentFinished {
            call_id: id(4),
            completion_seq: None,
            outcome: OutcomeKind::Abandoned,
        },
        EngineEvent::ChildFinished {
            workflow_path: vec!["name:child@0123456789ab".into()],
            ok: true,
        },
        EngineEvent::ReplayDiverged { at_seq: 7 },
        EngineEvent::Finished {
            outcome: RunOutcomeKind::Completed,
        },
    ];
    let expected = [
        r#"{"event":"agent_finished","call_id":{"digest":"D","occurrence":0},"completion_seq":3,"outcome":{"kind":"completed"}}"#,
        r#"{"event":"agent_finished","call_id":{"digest":"D","occurrence":1},"completion_seq":4,"outcome":{"kind":"failed","reason":"provider error"}}"#,
        r#"{"event":"agent_finished","call_id":{"digest":"D","occurrence":2},"completion_seq":5,"outcome":{"kind":"skipped"}}"#,
        r#"{"event":"agent_finished","call_id":{"digest":"D","occurrence":3},"completion_seq":null,"outcome":{"kind":"aborted"}}"#,
        r#"{"event":"agent_finished","call_id":{"digest":"D","occurrence":4},"completion_seq":null,"outcome":{"kind":"abandoned"}}"#,
        r#"{"event":"child_finished","workflow_path":["name:child@0123456789ab"],"ok":true}"#,
        r#"{"event":"replay_diverged","at_seq":7}"#,
        r#"{"event":"finished","outcome":"completed"}"#,
    ];
    for (event, line) in events.iter().zip(expected) {
        assert_eq!(serde_json::to_string(event).unwrap(), line);
    }
}
