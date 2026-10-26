//! Red-run tests 14 and 15 (#242): finished batches and pruning.
//!
//! Red: every test calls a stubbed `SubagentTree` method and panics in
//! `todo!()`.

use hermes_contract::events::EventFrame;
use serde_json::{Value, json};
use talaria_core::subagents::{Clock, RowState, SubagentEvent, SubagentTree};

fn clock(ms: u64) -> Clock {
    Clock {
        mono_ms: ms,
        wall_s: ms as f64 / 1000.0,
    }
}

fn event(kind: &str, payload: Value) -> SubagentEvent {
    let params = json!({
        "type": kind,
        "session_id": "local-test",
        "seq": 1,
        "replay_epoch": "epoch",
        "payload": payload,
    });
    let frame = EventFrame::from_params(&params).expect("contract-shaped params");
    SubagentEvent::from_notification(&frame.notification).expect("subagent event")
}

fn payload(id: &str) -> Value {
    json!({
        "subagent_id": id,
        "goal": format!("goal {id}"),
        "task_index": 0,
        "task_count": 1,
    })
}

fn with(payload: Value, key: &str, value: Value) -> Value {
    let mut object = payload.as_object().cloned().expect("object");
    object.insert(key.to_owned(), value);
    Value::Object(object)
}

fn start(tree: &mut SubagentTree, id: &str, delegation: Option<&str>, ms: u64) {
    let spawned = match delegation {
        Some(name) => with(payload(id), "delegation_id", json!(name)),
        None => payload(id),
    };
    tree.apply(&event("subagent.spawn_requested", spawned), clock(ms));
    let started = match delegation {
        Some(name) => with(payload(id), "delegation_id", json!(name)),
        None => payload(id),
    };
    tree.apply(&event("subagent.start", started), clock(ms + 1));
}

fn complete(tree: &mut SubagentTree, id: &str, delegation: Option<&str>, ms: u64) {
    let mut body = with(payload(id), "status", json!("completed"));
    if let Some(name) = delegation {
        body = with(body, "delegation_id", json!(name));
    }
    tree.apply(&event("subagent.complete", body), clock(ms));
}

fn batch_ids(tree: &mut SubagentTree) -> Vec<(String, Vec<String>)> {
    tree.take_finished_batches()
        .iter()
        .map(|batch| {
            (
                batch.key.clone(),
                batch.rows.iter().map(|(row, _)| row.id.clone()).collect(),
            )
        })
        .collect()
}

#[test]
fn batch_returned_once_with_descendants() {
    let mut tree = SubagentTree::new();
    start(&mut tree, "root-a", Some("deleg-1"), 1_000);
    start(&mut tree, "root-b", Some("deleg-1"), 2_000);
    start(&mut tree, "solo", None, 3_000);
    let nested = with(payload("grandchild"), "parent_id", json!("root-a"));
    tree.apply(
        &event("subagent.spawn_requested", nested.clone()),
        clock(4_000),
    );
    tree.apply(&event("subagent.start", nested), clock(4_001));
    complete(&mut tree, "root-a", Some("deleg-1"), 5_000);
    complete(&mut tree, "root-b", Some("deleg-1"), 6_000);
    complete(&mut tree, "solo", None, 7_000);

    assert!(
        tree.take_finished_batches().is_empty(),
        "a grandchild still running holds the batch back"
    );
    assert!(
        tree.take_finished_batches().is_empty(),
        "and a live root without a delegation id holds every batch"
    );

    complete(&mut tree, "grandchild", Some("deleg-1"), 8_000);
    assert!(
        tree.take_finished_batches().is_empty(),
        "the solo root still has no delegation id"
    );

    let joined = with(payload("solo"), "delegation_id", json!("deleg-1"));
    tree.apply(&event("subagent.start", joined), clock(9_000));
    let batches = batch_ids(&mut tree);
    assert_eq!(batches.len(), 1, "one batch with all three roots");
    assert_eq!(batches[0].0, "deleg-1");
    assert_eq!(
        batches[0].1,
        vec![
            "root-a".to_owned(),
            "grandchild".to_owned(),
            "root-b".to_owned(),
            "solo".to_owned(),
        ]
    );
    assert!(
        tree.take_finished_batches().is_empty(),
        "a batch is returned once"
    );

    let mut tree = SubagentTree::new();
    let listed: hermes_contract::schemas::SubagentListResult =
        serde_json::from_value(json!({"subagents": [
            {"subagent_id": "listed-1", "goal": "goal listed-1", "status": "running",
             "task_index": 0, "task_count": 1},
        ], "delegations": []}))
        .expect("contract-shaped list answer");
    tree.reconcile(&listed, clock(1_000));
    tree.reconcile(
        &serde_json::from_value(json!({"subagents": [], "delegations": []})).expect("empty list"),
        clock(2_000),
    );
    tree.reconcile(
        &serde_json::from_value(json!({"subagents": [], "delegations": []})).expect("empty list"),
        clock(3_000),
    );
    assert_eq!(tree.get("listed-1").expect("row").state, RowState::Gone);
    assert!(
        tree.take_finished_batches().is_empty(),
        "a batch with no seen_live row is never returned"
    );
}

#[test]
fn prune_removes_returned_terminal_rows_only() {
    let mut tree = SubagentTree::new();
    start(&mut tree, "root-a", Some("deleg-1"), 1_000);
    start(&mut tree, "root-b", Some("deleg-1"), 2_000);
    start(&mut tree, "waiting", None, 3_000);
    complete(&mut tree, "root-a", Some("deleg-1"), 4_000);
    complete(&mut tree, "root-b", Some("deleg-1"), 5_000);
    assert!(
        tree.take_finished_batches().is_empty(),
        "the waiting root holds the batch"
    );
    assert!(
        !tree.prune_finished(),
        "an all-terminal batch with a seen_live row that was not yet returned stays"
    );
    assert!(tree.get("root-a").is_some());

    let joined = with(payload("waiting"), "delegation_id", json!("deleg-1"));
    tree.apply(&event("subagent.start", joined), clock(6_000));
    complete(&mut tree, "waiting", Some("deleg-1"), 7_000);
    assert_eq!(tree.take_finished_batches().len(), 1);
    start(&mut tree, "running", Some("deleg-2"), 8_000);
    assert!(tree.prune_finished());
    assert!(tree.get("root-a").is_none());
    assert!(tree.get("root-b").is_none());
    assert!(tree.get("waiting").is_none());
    assert!(tree.get("running").is_some(), "a running row stays");

    let mut learnt = SubagentTree::new();
    let listed: hermes_contract::schemas::SubagentListResult =
        serde_json::from_value(json!({"subagents": [
            {"subagent_id": "listed-1", "goal": "goal listed-1", "status": "running",
             "task_index": 0, "task_count": 1},
        ], "delegations": []}))
        .expect("contract-shaped list answer");
    learnt.reconcile(&listed, clock(1_000));
    let empty: hermes_contract::schemas::SubagentListResult =
        serde_json::from_value(json!({"subagents": [], "delegations": []})).expect("empty list");
    learnt.reconcile(&empty, clock(2_000));
    learnt.reconcile(&empty, clock(3_000));
    assert_eq!(learnt.get("listed-1").expect("row").state, RowState::Gone);
    assert!(
        learnt.take_finished_batches().is_empty(),
        "never returned without seen_live"
    );
    assert!(learnt.prune_finished(), "removed and retired too");
    learnt.reconcile(&listed, clock(4_000));
    assert!(
        learnt.get("listed-1").is_none(),
        "a later list naming it creates nothing"
    );
}
