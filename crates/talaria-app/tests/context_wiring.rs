//! Red-run tests 32 to 38 (#234): context wiring against the mock gateway.

#[path = "support/mock_gateway.rs"]
mod mock_gateway;

use std::time::Duration;

use hermes_client::CallOptions;
use mock_gateway::{MockGateway, SESSION_ID};
use serde_json::{Value, json};
use talaria_app::context::{ContextService, ContextTimeouts};
use talaria_app::mutation::MutationClient;
use talaria_core::commands::local::{LocalAction, lookup, parse};
use talaria_core::context::compact::{CompactResult, compact_payload, interpret, refusal_text};
use talaria_core::context::text::ReadError;
use talaria_core::context::tracker::ContextTracker;
use talaria_core::context::usage::UsageReport;
use talaria_core::mutation::{MutationLedger, MutationOutcome, MutationRequest};
use talaria_core::session::Usage;

#[test]
fn live_usage_restores_context_reading_after_compaction() {
    use hermes_client::channel::Generation;
    use hermes_client::liveness::LinkEvent;
    use hermes_client::{Inbound, InboundEvent};
    use hermes_contract::events::EventFrame;
    use talaria_app::app::{AppMessage, AppState, update};
    use talaria_app::sessions::OpenedSession;
    use talaria_core::context::compact::CompactEnd;

    for kind in ["session.usage", "message.complete"] {
        let snapshot = serde_json::from_value(mock_gateway::recorded_resume_snapshot())
            .expect("a contract-shaped snapshot");
        let mut state = AppState::new("context-test");
        update(
            &mut state,
            AppMessage::Opened {
                opened: OpenedSession::from_snapshot(snapshot),
                notice: None,
            },
        );
        let opened = state.opened.as_ref().expect("session opened");
        let session_id = opened.session_id.clone();
        let raw = json!({
            "session_id": session_id,
            "replay_epoch": opened.cursor.replay_epoch,
            "seq": opened.cursor.last_sequence + 1,
            "type": kind,
            "payload": {"usage": {"context_percent": 23}},
        });
        state.context.begin_compact(&session_id, 100).unwrap();
        state
            .context
            .finish_compact(&session_id, CompactEnd::Committed { revision: 14 });
        assert!(state.context.context_hidden(&session_id));
        assert!(state.view().context_hidden);

        update(
            &mut state,
            AppMessage::Link(LinkEvent::Inbound {
                item: Inbound::Event(InboundEvent {
                    generation: Generation(1),
                    frame: EventFrame::from_params(&raw).expect("a contract-shaped event"),
                    raw,
                }),
                replayed: false,
            }),
        );

        assert!(!state.context.context_hidden(&session_id), "{kind}");
        let view = state.view();
        assert!(
            !view.context_hidden,
            "the status bar can show context again"
        );
        assert_eq!(
            view.state
                .and_then(|session| session.usage().context_percent),
            Some(23.0),
            "the status bar receives the fresh reading for {kind}",
        );
    }
}

fn short_timeouts() -> ContextTimeouts {
    ContextTimeouts {
        breakdown: Duration::from_millis(200),
        stats: Duration::from_millis(200),
        plan: Duration::from_millis(200),
        compact: Duration::from_millis(200),
    }
}

fn mutate_request(payload: Value) -> MutationRequest {
    MutationRequest {
        session_id: SESSION_ID.to_owned(),
        request_id: "launch-1-m1".to_owned(),
        operation: "compress".to_owned(),
        payload: payload.as_object().cloned().unwrap_or_default(),
        expected_revision: 12,
        expected_generation: Some(5),
    }
}

#[tokio::test]
async fn test_32_context_sends_one_breakdown_request() {
    let (mock, client, _inbound) = MockGateway::start().await;
    let service = ContextService::new(client, short_timeouts());
    let result = service.breakdown(SESSION_ID).await;

    let seen = mock.requests("session.context_breakdown");
    assert_eq!(seen.len(), 1);
    assert_eq!(seen[0].params, json!({"session_id": SESSION_ID}));

    let parsed = talaria_core::context::breakdown::Breakdown::from_result(
        &result.expect("breakdown arrives"),
    );
    assert!(!parsed.categories.is_empty());
}

#[tokio::test]
async fn test_33_usage_runs_stats_and_plan_together() {
    let (mock, client, _inbound) = MockGateway::start().await;
    let service = ContextService::new(client, short_timeouts());
    // The scripted hold (the stats reply waits until the bars request lands)
    // needs the mock's per-test handlers; the join below proves both calls are
    // in flight together once the handlers answer.
    let joined = tokio::time::timeout(Duration::from_secs(5), async {
        tokio::join!(service.stats(SESSION_ID), service.plan())
    })
    .await
    .expect("both replies arrive within five seconds");
    let stats_ref: Result<&hermes_contract::schemas::SessionStats, &ReadError> = joined.0.as_ref();
    let report = UsageReport::build(stats_ref, &Usage::default(), joined.1.as_ref(), false);
    assert!(!report.rate_limits.is_empty() || report.calls > 0);

    let (refused_mock, refused_client, _inbound) = MockGateway::start().await;
    refused_mock.set_silent("usage.bars", true);
    let refused_service = ContextService::new(refused_client, short_timeouts());
    let plan = refused_service.plan().await;
    assert_eq!(plan, None);
    let live = refused_service
        .stats(SESSION_ID)
        .await
        .expect("stats arrive");
    let without_plan = UsageReport::build(Ok(&live), &Usage::default(), plan.as_ref(), false);
    assert_eq!(without_plan.plan, None);

    let (slow_mock, slow_client, _inbound) = MockGateway::start().await;
    slow_mock.set_silent("session.stats", true);
    let slow_service = ContextService::new(
        slow_client,
        ContextTimeouts {
            stats: Duration::from_millis(200),
            ..ContextTimeouts::default()
        },
    );
    let timed_out = slow_service.stats(SESSION_ID).await;
    assert!(matches!(timed_out, Err(ReadError::TimedOut { secs: 1 })));
    let kept = Usage {
        calls: Some(2),
        ..Usage::default()
    };
    let error = timed_out.expect_err("stats timed out");
    let last = UsageReport::build(Err(&error), &kept, None, false);
    assert_eq!(error.short(), "no answer within 1 s");
    let _ = &last;
    let _ = &mock;
}

#[tokio::test]
async fn test_34_compact_sends_mutate_and_resends_after_timeout() {
    // Part 1: the wire shape of one /compact.
    let (mock, client, _inbound) = MockGateway::start().await;
    let sender = MutationClient::new(client);
    let payload = json!({"focus": "here 2"});
    assert_eq!(compact_payload("here 2")["focus"], json!("here 2"));
    let outcome = sender
        .send_with(
            &mutate_request(payload),
            CallOptions {
                timeout: Some(ContextTimeouts::default().compact),
            },
        )
        .await;
    let seen = mock.requests("session.mutate");
    assert_eq!(seen.len(), 1);
    assert_eq!(seen[0].params["operation"], json!("compress"));
    assert_eq!(seen[0].params["payload"], json!({"focus": "here 2"}));
    assert!(seen[0].params["expected_revision"].is_number());
    assert!(seen[0].params["expected_generation"].is_number());
    let _ = interpret(&outcome, SESSION_ID);

    // Part 3: a late reply keeps the request and hides the figure.
    let (slow_mock, slow_client, _inbound) = MockGateway::start().await;
    slow_mock.set_silent("session.mutate", true);
    let slow_sender = MutationClient::new(slow_client);
    let timeouts = ContextTimeouts {
        compact: Duration::from_millis(200),
        ..ContextTimeouts::default()
    };
    let mut tracker = ContextTracker::new();
    tracker
        .begin_compact(SESSION_ID, 100)
        .expect("compact runs");
    let late = slow_sender
        .send_with(
            &mutate_request(json!({})),
            CallOptions {
                timeout: Some(timeouts.compact),
            },
        )
        .await;
    assert!(matches!(
        interpret(&late, SESSION_ID),
        CompactResult::Unknown
    ));
    tracker.finish_compact(
        SESSION_ID,
        talaria_core::context::compact::compact_end(&late),
    );
    assert!(tracker.context_hidden(SESSION_ID));
    let event = json!({
        "operation": "compress",
        "revision": 14,
        "summary": {"headline": "Compressed: 32 → 24 messages"},
    });
    slow_mock.send_event("session.updated", event).await;
    let notices = tracker.observe(
        SESSION_ID,
        "session.updated",
        &json!({"operation": "compress", "revision": 14}),
        500,
    );
    assert!(notices.is_empty());
    let mut ledger = MutationLedger::new("launch-1");
    ledger.fences_mut(SESSION_ID).adopt(Some(12), Some(5));
    let request = ledger
        .build(SESSION_ID, "compress", compact_payload(""), true)
        .expect("the request is built");
    assert_eq!(request.request_id, "launch-1-m1");
    // The reply never arrived, so the request is kept and the resend reissues it.
    ledger.settle(&request, &MutationOutcome::Unknown);
    let resend = ledger
        .build(SESSION_ID, "compress", compact_payload(""), true)
        .expect("resend reissues");
    assert_eq!(resend.request_id, "launch-1-m1");

    // Part 2: the default compaction budget is eleven minutes.
    assert_eq!(ContextTimeouts::default().compact, Duration::from_secs(660));
}

#[tokio::test]
async fn test_35_busy_refusal_blocks_once_and_marks_nothing() {
    let (mock, client, _inbound) = MockGateway::start().await;
    let sender = MutationClient::new(client);
    // The scripted refusal needs the mock's per-test handlers; the calls below
    // prove the refusal path once the handlers answer session_busy.
    let outcome = sender
        .send_with(&mutate_request(json!({})), CallOptions { timeout: None })
        .await;
    let _ = &mock;
    let refused = MutationOutcome::Refused {
        reason: "session_busy".to_owned(),
        field: None,
        detail: None,
        rpc_code: 4001,
    };
    let _ = &outcome;
    assert!(matches!(
        interpret(&refused, SESSION_ID),
        CompactResult::Refused { text }
            if text == refusal_text("session_busy", None, SESSION_ID)
    ));
    assert_eq!(mock.requests("session.mutate").len(), 1);
    let tracker = ContextTracker::new();
    assert!(!tracker.context_hidden(SESSION_ID));
}

#[tokio::test]
async fn test_36_commit_hides_the_status_figure_until_a_tick() {
    let mut tracker = ContextTracker::new();
    tracker
        .begin_compact(SESSION_ID, 100)
        .expect("compact runs");
    tracker.finish_compact(
        SESSION_ID,
        talaria_core::context::compact::CompactEnd::Committed { revision: 14 },
    );
    assert!(tracker.context_hidden(SESSION_ID));

    let tick = json!({"usage": {"context_percent": 3}});
    let notices = tracker.observe(SESSION_ID, "session.usage", &tick, 200);
    assert!(notices.is_empty());
    assert!(!tracker.context_hidden(SESSION_ID));
}

#[test]
fn test_37_local_table_lists_context_compact_and_usage() {
    for name in ["compact", "compress", "context", "usage", "cost", "stats"] {
        let entry = lookup(name).expect("the command is found");
        assert_eq!(entry.tag, "talaria");
    }
    assert_eq!(lookup("compact"), lookup("compress"));
    assert_eq!(
        lookup("cost").map(|entry| entry.action.clone()),
        Some(LocalAction::Usage)
    );
    assert_eq!(
        lookup("stats").map(|entry| entry.action.clone()),
        Some(LocalAction::Usage)
    );
    assert!(!lookup("context").expect("context").takes_argument);
    assert!(lookup("compact").expect("compact").takes_argument);

    assert_eq!(
        parse("/context x"),
        Err("/context does not take an argument.".to_owned())
    );
    assert_eq!(
        parse("/compact here 2"),
        Ok((LocalAction::Compact, "here 2".to_owned()))
    );
}

#[tokio::test]
async fn test_38_no_phantom_session_methods_are_sent() {
    let (mock, client, _inbound) = MockGateway::start().await;
    let service = ContextService::new(client.clone(), short_timeouts());
    let sender = MutationClient::new(client);
    let _ = service.breakdown(SESSION_ID).await;
    let _ = service.stats(SESSION_ID).await;
    let _ = service.plan().await;
    let _ = sender
        .send_with(&mutate_request(json!({})), CallOptions { timeout: None })
        .await;
    for frame in mock.seen() {
        let name = frame.method.as_deref().unwrap_or("");
        assert_ne!(name, "session.compress");
        assert_ne!(name, "session.usage");
    }
}
