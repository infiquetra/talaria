//! Red-run tests 32 and 33 (#242): the recorded subagent runs.
//!
//! Test 32 replays the committed `subagent-batch` recording through the hub
//! and the tree. Test 33 needs the `subagent-control` recording, which the
//! developer records against the operator's gateway; until it is committed,
//! its red is the absent file. Red: the stub returns no batch.

#[path = "support/mock_gateway.rs"]
mod mock_gateway;

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::time::Duration;

use hermes_client::ReplaySource;
use hermes_contract::events::{EventFrame, Notification};
use mock_gateway::{MockGateway, SESSION_ID};
use serde_json::{Value, json};
use talaria_app::app::AppMessage;
use talaria_app::subagents::{SubagentHub, SubagentMsg, SubagentsConfig};
use talaria_core::directory::rows::RowKey;
use talaria_core::subagents::{Clock, RowState, SubagentEvent, SubagentTree};
use tokio::sync::mpsc;

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/frames")
        .join(name)
}

fn key() -> RowKey {
    RowKey {
        profile: "fixture".to_owned(),
        session_id: SESSION_ID.to_owned(),
    }
}

fn clock_ms(ms: u64) -> Clock {
    Clock {
        mono_ms: ms,
        wall_s: ms as f64 / 1000.0,
    }
}

/// The recording's `subagent.*` notifications, in frame order.
fn subagent_notifications(source: &ReplaySource) -> Vec<Notification> {
    let mut out = Vec::new();
    for frame in source.inbound() {
        let Some(params) = frame.frame.get("params") else {
            continue;
        };
        if frame.frame.get("method") != Some(&json!("event")) {
            continue;
        }
        if params
            .get("type")
            .and_then(Value::as_str)
            .is_none_or(|name| !name.starts_with("subagent."))
        {
            continue;
        }
        let decoded = EventFrame::from_params(params).expect("the recording parses");
        out.push(decoded.notification);
    }
    out
}

#[tokio::test(start_paused = true)]
async fn recorded_batch_replays() {
    let source = ReplaySource::open(&fixture("subagent-batch.jsonl"))
        .expect("the committed recording opens");
    let notifications = subagent_notifications(&source);
    assert_eq!(
        notifications.len(),
        21,
        "the recording holds 21 subagent events: 2 spawns, 2 starts, 9 thinking, 6 tool, 1 progress, 1 complete"
    );

    let (gateway, client, _inbound) = MockGateway::start().await;
    let (tx, mut rx) = mpsc::unbounded_channel();
    let mut hub = SubagentHub::new(SubagentsConfig::default(), tx);
    let key = key();
    let mut now_ms = 0;
    gateway.push_result(json!({"subagents": [], "delegations": []}));
    hub.attach(key.clone(), client);
    for _ in 0..50 {
        tokio::task::yield_now().await;
        tokio::time::advance(Duration::from_millis(10)).await;
        now_ms += 10;
    }
    while let Ok(msg) = rx.try_recv() {
        if let AppMessage::Subagents(inner) = msg {
            hub.on_message(inner, clock_ms(now_ms));
        }
    }

    for n in &notifications {
        now_ms += 100;
        hub.on_notification(&key, n, clock_ms(now_ms));
    }
    while let Ok(msg) = rx.try_recv() {
        if let AppMessage::Subagents(inner) = msg {
            hub.on_message(inner, clock_ms(now_ms));
        }
    }
    // Only one child completes in the recording; the hub's polls reconcile
    // two empty lists, which retires the other as gone.
    for _ in 0..2 {
        gateway.push_result(json!({"subagents": [], "delegations": []}));
        hub.on_message(SubagentMsg::PollTick { key: key.clone() }, clock_ms(now_ms));
        for _ in 0..50 {
            tokio::task::yield_now().await;
            tokio::time::advance(Duration::from_millis(10)).await;
            now_ms += 10;
        }
        while let Ok(msg) = rx.try_recv() {
            if let AppMessage::Subagents(inner) = msg {
                hub.on_message(inner, clock_ms(now_ms));
            }
        }
    }
    let tree = hub.tree(&key).expect("the hub holds the tree");
    for row in tree.rows_in_order() {
        assert!(
            tree.row(row.index).state.is_terminal(),
            "every row is terminal"
        );
    }

    // The hub takes its batches internally and surfaces no drain, so the
    // one-batch assertion replays the same frames through a bare tree.
    let mut oracle = SubagentTree::new();
    let mut now_ms = 0;
    for n in &notifications {
        now_ms += 100;
        let ev = SubagentEvent::from_notification(n).expect("a subagent event");
        oracle.apply(&ev, clock_ms(now_ms));
    }
    let empty: hermes_contract::schemas::SubagentListResult =
        serde_json::from_value(json!({"subagents": [], "delegations": []})).expect("empty list");
    oracle.reconcile(&empty, clock_ms(now_ms));
    oracle.reconcile(&empty, clock_ms(now_ms));
    let batches = oracle.take_finished_batches();
    assert_eq!(batches.len(), 1, "exactly one batch is returned");
    assert!(batches[0].key.starts_with("deleg_"), "one delegation");
    let mut ids: Vec<String> = batches[0]
        .rows
        .iter()
        .map(|(row, _)| row.id.clone())
        .collect();
    ids.sort();
    assert_eq!(
        ids,
        vec!["sa-0-1d577efe".to_owned(), "sa-1-523d43ce".to_owned()]
    );
}

#[tokio::test(start_paused = true)]
async fn recorded_control_replays() {
    let path = fixture("subagent-control.jsonl");
    assert!(
        path.exists(),
        "the subagent-control recording is absent; the developer records it against the \
         operator's gateway per #242 section 9.3 (exit 2 means a precondition is absent: report, \
         never hand-write the file)"
    );
    let source = ReplaySource::open(&path).expect("the control recording opens");

    // The driver's four calls and their answers, matched by frame id.
    let mut calls: BTreeMap<String, String> = BTreeMap::new();
    for frame in source.outbound() {
        if let (Some(id), Some(method)) = (
            frame.frame.get("id"),
            frame.frame.get("method").and_then(Value::as_str),
        ) {
            calls.insert(id.to_string(), method.to_owned());
        }
    }
    let mut answers: BTreeMap<String, Value> = BTreeMap::new();
    for frame in source.inbound() {
        if frame.frame.get("method").is_some() {
            continue;
        }
        if let Some(id) = frame.frame.get("id")
            && let Some(method) = calls.get(&id.to_string())
        {
            answers.insert(
                method.clone(),
                frame.frame.get("result").cloned().unwrap_or(Value::Null),
            );
        }
    }
    let list = answers.get("subagent.list").expect("the list was answered");
    assert!(
        list["subagents"]
            .as_array()
            .is_some_and(|rows| !rows.is_empty()),
        "the list answer holds snapshots"
    );
    let steer = answers
        .get("subagent.steer")
        .expect("the steer was answered");
    assert!(
        matches!(steer["status"].as_str(), Some("queued" | "rejected")),
        "the steer answer was queued or rejected"
    );
    let interrupt = answers
        .get("subagent.interrupt")
        .expect("the interrupt was answered");
    assert_eq!(interrupt["found"], json!(true));

    let mut tree = SubagentTree::new();
    let mut now_ms = 0;
    let mut interrupted = None;
    for n in subagent_notifications(&source) {
        now_ms += 100;
        let ev = SubagentEvent::from_notification(&n).expect("a subagent event");
        tree.apply(&ev, clock_ms(now_ms));
        if ev.kind == talaria_core::subagents::SubagentEventKind::Complete
            && ev.payload.status.as_ref().is_some_and(|status| {
                matches!(
                    status,
                    hermes_contract::schemas::SubagentStatus::Interrupted
                )
            })
        {
            interrupted = ev.payload.subagent_id.clone();
        }
    }
    let id = interrupted.expect("a subagent.complete with status interrupted");
    assert_eq!(
        tree.get(&id).expect("the stopped row").state,
        RowState::Interrupted
    );
    assert_eq!(tree.take_finished_batches().len(), 1, "one batch");
}
