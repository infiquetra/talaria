#![cfg(feature = "mock")]
mod common;
use common::*;
use serde_json::json;
use std::{sync::Arc, time::Duration};
use talaria_workflow::engine::{self, EngineEvent, MapResolver, OutcomeKind, RunOutcome};

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn return_values_and_script_errors() {
    for (body, expected) in [
        ("return {a:1};", json!({"a":1})),
        ("return undefined;", json!(null)),
        ("return new Date(0);", json!("1970-01-01T00:00:00.000Z")),
        ("return {a:undefined};", json!({})),
    ] {
        let (outcome, _) = run(body, default_backend()).await;
        assert_eq!(result(outcome), expected);
    }
    for body in [
        "let x={}; x.x=x; return x;",
        "return 1n;",
        "return NaN;",
        "return {x:()=>1};",
        "return [undefined];",
    ] {
        let (outcome, _) = run(body, default_backend()).await;
        assert!(
            failure(outcome).message.contains("not JSON-serialisable"),
            "{body}"
        );
    }
    let script = format!("{}\n\n\nthrow new Error('boom');", HEADER);
    let (outcome, _) = execute(
        &script,
        config(),
        default_backend(),
        Arc::new(MapResolver::new()),
    )
    .await;
    let error = failure(outcome);
    assert_eq!(
        (error.name.as_str(), error.message.as_str(), error.line),
        ("Error", "boom", Some(5))
    );
    let child = format!("{}\nthrow new Error('child boom');", HEADER);
    let resolver = Arc::new(MapResolver::new().with_name("child", &child));
    let (outcome, _) = execute(
        &source("return await workflow('child');"),
        config(),
        default_backend(),
        resolver,
    )
    .await;
    let error = failure(outcome);
    assert_eq!(error.line, Some(3));
    assert_eq!(error.workflow_path, vec!["name:child@74234e98afe7"]);
}

#[tokio::test(flavor = "current_thread")]
async fn cancel_aborts_promptly() {
    let backend = backend(
        r#"{"rules":[{"match":{"prompt_contains":"slow"},"outcome":{"text":"ok"},"delay_ms":30000}]}"#,
    );
    let parsed = engine::parse_script(&source("return await parallel([() => agent('slow 1'),() => agent('slow 2'),() => agent('slow 3')]);")).unwrap();
    let mut handle = engine::spawn(
        parsed,
        config(),
        backend.clone(),
        Arc::new(MapResolver::new()),
    );
    let mut events = Vec::new();
    while events
        .iter()
        .filter(|e| matches!(e, EngineEvent::AgentStarted { .. }))
        .count()
        < 3
    {
        events.push(
            tokio::time::timeout(Duration::from_secs(2), handle.events.recv())
                .await
                .unwrap()
                .unwrap(),
        );
    }
    tokio::time::sleep(Duration::from_millis(100)).await;
    let started = std::time::Instant::now();
    handle.cancel.cancel();
    let outcome = tokio::time::timeout(Duration::from_secs(1), handle.outcome)
        .await
        .unwrap()
        .unwrap();
    assert!(started.elapsed() < Duration::from_secs(1));
    assert_eq!(
        outcome,
        RunOutcome::Aborted {
            reason: "cancelled".into()
        }
    );
    let before = events.len();
    while let Some(event) = handle.events.recv().await {
        events.push(event);
    }
    assert!(
        !events[before..]
            .iter()
            .any(|e| matches!(e, EngineEvent::AgentStarted { .. }))
    );
    assert_eq!(
        events
            .iter()
            .filter(|e| matches!(
                e,
                EngineEvent::AgentFinished {
                    outcome: OutcomeKind::Aborted,
                    ..
                }
            ))
            .count(),
        3
    );
    assert_eq!(backend.requests().len(), 3);
    assert_eq!(backend.in_flight(), 0);
}

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn backend_abort_ends_run_uncatchably() {
    for body in [
        "agent('slow'); try {await agent('abort')} catch(e) {log('caught')} log('later');",
        "await parallel([() => agent('slow'), async () => {try {await agent('abort')} catch(e) {log('caught')}}]); log('later');",
    ] {
        let backend = backend(
            r#"{"rules":[{"match":{"prompt_equals":"abort"},"outcome":{"abort":"lost"},"delay_ms":100},{"match":{"prompt_equals":"slow"},"outcome":{"text":"ok"},"delay_ms":30000}]}"#,
        );
        let (outcome, events) = run(body, backend.clone()).await;
        assert_eq!(
            outcome,
            RunOutcome::Aborted {
                reason: "lost".into()
            }
        );
        assert!(!events.iter().any(|e| matches!(e, EngineEvent::Log {message,..} if message == "caught" || message == "later")));
        let sibling = backend
            .requests()
            .into_iter()
            .find(|r| r.prompt == "slow")
            .unwrap();
        assert!(
            events
                .iter()
                .any(|e| matches!(e, EngineEvent::AgentFinished {
            call_id, outcome: OutcomeKind::Aborted, ..
        } if *call_id == sibling.call_id))
        );
        assert_eq!(backend.in_flight(), 0);
    }
}

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn unawaited_work_is_abandoned() {
    for (body, abort, succeeds) in [
        ("agent('slow'); return 1;", false, true),
        ("agent('slow'); throw Error('boom');", false, false),
        ("agent('slow'); return 1;", true, true),
    ] {
        let outcome_rule = if abort {
            r#"{"abort":"late"}"#
        } else {
            r#"{"text":"ok"}"#
        };
        let rules = format!(
            r#"{{"rules":[{{"match":{{"prompt_equals":"slow"}},"outcome":{outcome_rule},"delay_ms":10000}}]}}"#
        );
        let backend = backend(&rules);
        let start = tokio::time::Instant::now();
        let (outcome, events) = run(body, backend.clone()).await;
        if succeeds {
            assert_eq!(result(outcome), json!(1));
        } else {
            assert_eq!(failure(outcome).message, "boom");
        }
        assert_eq!(start.elapsed(), Duration::ZERO);
        assert_eq!(
            events
                .iter()
                .filter(|e| matches!(
                    e,
                    EngineEvent::AgentFinished {
                        outcome: OutcomeKind::Abandoned,
                        completion_seq: None,
                        ..
                    }
                ))
                .count(),
            1
        );
        assert!(matches!(events.last(), Some(EngineEvent::Finished { .. })));
        assert_eq!(backend.in_flight(), 0);
    }
}
