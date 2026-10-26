//! Red-run tests 1 to 13, 16 and 17 (#242): the subagent tree.
//!
//! Payloads are JSON literals in the contract's shape, so a field name typo
//! fails at deserialization, not silently. Red: every test calls a stubbed
//! `SubagentTree` or `SubagentEvent` method and panics in `todo!()`.

use hermes_contract::events::EventFrame;
use serde_json::{Value, json};
use talaria_core::subagents::{
    Applied, Clock, RowState, SubagentCounts, SubagentEvent, SubagentEventKind, SubagentTree,
};

fn clock(ms: u64) -> Clock {
    Clock {
        mono_ms: ms,
        wall_s: ms as f64 / 1000.0,
    }
}

fn params(kind: &str, payload: Value) -> Value {
    json!({
        "type": kind,
        "session_id": "local-test",
        "seq": 1,
        "replay_epoch": "epoch",
        "payload": payload,
    })
}

fn notification_of(kind: &str, payload: Value) -> hermes_contract::events::Notification {
    EventFrame::from_params(&params(kind, payload))
        .expect("contract-shaped params")
        .notification
}

fn event(kind: &str, payload: Value) -> SubagentEvent {
    SubagentEvent::from_notification(&notification_of(kind, payload)).expect("subagent event")
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

fn without(payload: Value, key: &str) -> Value {
    let mut object = payload.as_object().cloned().expect("object");
    object.remove(key);
    Value::Object(object)
}

fn list_result(snapshots: Value) -> hermes_contract::schemas::SubagentListResult {
    serde_json::from_value(json!({"subagents": snapshots, "delegations": []}))
        .expect("contract-shaped list answer")
}

fn snapshot(id: &str) -> Value {
    json!({
        "subagent_id": id,
        "goal": format!("goal {id}"),
        "status": "running",
        "task_index": 0,
        "task_count": 1,
    })
}

fn state_of(tree: &SubagentTree, id: &str) -> RowState {
    tree.get(id)
        .unwrap_or_else(|| panic!("row {id} exists"))
        .state
}

#[test]
fn event_payload_reads_contract_fields() {
    let kinds = [
        (
            "subagent.spawn_requested",
            SubagentEventKind::SpawnRequested,
        ),
        ("subagent.start", SubagentEventKind::Start),
        ("subagent.progress", SubagentEventKind::Progress),
        ("subagent.thinking", SubagentEventKind::Thinking),
        ("subagent.tool", SubagentEventKind::Tool),
        ("subagent.complete", SubagentEventKind::Complete),
    ];
    for (name, kind) in kinds {
        let decoded = event(name, payload("child-1"));
        assert_eq!(decoded.kind, kind, "{name}");
        assert_eq!(decoded.payload.goal, "goal child-1");
        assert_eq!(decoded.payload.task_count, 1);
        assert_eq!(decoded.payload.task_index, 0);
        assert_eq!(decoded.payload.subagent_id.as_deref(), Some("child-1"));
    }

    let other = notification_of("message.start", json!({}));
    assert!(SubagentEvent::from_notification(&other).is_none());
    let unknown = notification_of("frobnicate", json!({"goal": "x"}));
    assert!(SubagentEvent::from_notification(&unknown).is_none());

    for broken in [
        without(payload("child-1"), "goal"),
        without(payload("child-1"), "task_count"),
        without(payload("child-1"), "task_index"),
        with(payload("child-1"), "task_index", json!("1")),
    ] {
        assert!(
            EventFrame::from_params(&params("subagent.start", broken)).is_err(),
            "a payload that fails its contract shape never reaches the tree"
        );
    }
}

#[test]
fn status_words_map_and_complete_fails_closed() {
    let terminal = [
        ("completed", RowState::Completed),
        (
            "failed",
            RowState::Failed(talaria_core::subagents::FailKind::Failed),
        ),
        (
            "error",
            RowState::Failed(talaria_core::subagents::FailKind::Error),
        ),
        (
            "timeout",
            RowState::Failed(talaria_core::subagents::FailKind::Timeout),
        ),
        ("interrupted", RowState::Interrupted),
        (
            "queued",
            RowState::Failed(talaria_core::subagents::FailKind::Failed),
        ),
        (
            "running",
            RowState::Failed(talaria_core::subagents::FailKind::Failed),
        ),
    ];
    for (word, state) in terminal {
        let mut tree = SubagentTree::new();
        let id = format!("child-{word}");
        tree.apply(&event("subagent.start", payload(&id)), clock(1_000));
        let done = with(payload(&id), "status", json!(word));
        tree.apply(&event("subagent.complete", done), clock(2_000));
        assert_eq!(state_of(&tree, &id), state, "complete with {word}");
    }
    let mut tree = SubagentTree::new();
    tree.apply(
        &event("subagent.start", payload("child-absent")),
        clock(1_000),
    );
    tree.apply(
        &event("subagent.complete", payload("child-absent")),
        clock(2_000),
    );
    assert_eq!(
        state_of(&tree, "child-absent"),
        RowState::Failed(talaria_core::subagents::FailKind::Failed),
        "complete with no status fails closed"
    );

    for kind in [
        "subagent.start",
        "subagent.progress",
        "subagent.thinking",
        "subagent.tool",
    ] {
        for word in ["completed", "failed", "queued", "running"] {
            let mut tree = SubagentTree::new();
            let id = format!("child-{kind}-{word}");
            tree.apply(
                &event("subagent.spawn_requested", payload(&id)),
                clock(1_000),
            );
            assert_eq!(state_of(&tree, &id), RowState::Queued);
            let acting = with(payload(&id), "status", json!(word));
            tree.apply(&event(kind, acting), clock(2_000));
            assert_eq!(
                state_of(&tree, &id),
                RowState::Running,
                "{kind} with status {word}: a child that acts has started"
            );
        }
    }
    let mut tree = SubagentTree::new();
    tree.apply(
        &event("subagent.spawn_requested", payload("child-spawn")),
        clock(1_000),
    );
    let again = with(payload("child-spawn"), "status", json!("running"));
    tree.apply(&event("subagent.spawn_requested", again), clock(2_000));
    assert_eq!(state_of(&tree, "child-spawn"), RowState::Queued);

    let mut tree = SubagentTree::new();
    let finished = with(snapshot("child-listed-done"), "status", json!("completed"));
    tree.reconcile(&list_result(json!([finished])), clock(1_000));
    assert!(tree.get("child-listed-done").is_none());
    let live = with(snapshot("child-listed-live"), "status", json!("running"));
    tree.reconcile(&list_result(json!([live])), clock(2_000));
    assert_eq!(state_of(&tree, "child-listed-live"), RowState::Running);
}

#[test]
fn only_spawn_and_start_create_rows() {
    for kind in [
        "subagent.progress",
        "subagent.thinking",
        "subagent.tool",
        "subagent.complete",
    ] {
        let mut tree = SubagentTree::new();
        tree.apply(&event(kind, payload("child-1")), clock(1_000));
        assert!(tree.rows_in_order().is_empty(), "{kind}");
        assert_eq!(tree.version(), 0, "{kind}");
    }
}

#[test]
fn terminal_state_is_never_overwritten() {
    let mut tree = SubagentTree::new();
    tree.apply(&event("subagent.start", payload("child-1")), clock(1_000));
    let failed = with(payload("child-1"), "status", json!("failed"));
    tree.apply(&event("subagent.complete", failed), clock(2_000));
    tree.apply(&event("subagent.start", payload("child-1")), clock(3_000));
    tree.apply(
        &event("subagent.spawn_requested", payload("child-1")),
        clock(4_000),
    );
    tree.reconcile(&list_result(json!([snapshot("child-1")])), clock(5_000));
    assert_eq!(
        state_of(&tree, "child-1"),
        RowState::Failed(talaria_core::subagents::FailKind::Failed)
    );
}

#[test]
fn retired_ids_never_return() {
    let mut tree = SubagentTree::new();
    tree.apply(&event("subagent.start", payload("child-1")), clock(1_000));
    let done = with(payload("child-1"), "status", json!("completed"));
    tree.apply(&event("subagent.complete", done), clock(2_000));
    assert_eq!(tree.take_finished_batches().len(), 1);
    assert!(tree.prune_finished());
    tree.apply(&event("subagent.start", payload("child-1")), clock(3_000));
    assert!(
        tree.get("child-1").is_none(),
        "a retired start creates nothing"
    );
    tree.reconcile(&list_result(json!([snapshot("child-1")])), clock(4_000));
    assert!(
        tree.get("child-1").is_none(),
        "a retired list entry is skipped"
    );

    for index in 0..513 {
        let id = format!("batch-{index}");
        tree.apply(&event("subagent.start", payload(&id)), clock(5_000));
        let done = with(payload(&id), "status", json!("completed"));
        tree.apply(&event("subagent.complete", done), clock(6_000));
        assert_eq!(tree.take_finished_batches().len(), 1, "batch {index}");
        assert!(tree.prune_finished(), "batch {index}");
    }
    tree.apply(&event("subagent.start", payload("batch-0")), clock(7_000));
    assert_eq!(
        state_of(&tree, "batch-0"),
        RowState::Running,
        "the oldest of 513 retired ids is forgotten"
    );
    tree.apply(&event("subagent.start", payload("batch-1")), clock(8_000));
    assert!(
        tree.get("batch-1").is_none(),
        "ids inside the cap of 512 stay retired"
    );
}

#[test]
fn event_without_id_is_ignored() {
    let mut tree = SubagentTree::new();
    let before = tree.version();
    let applied = tree.apply(
        &event("subagent.start", without(payload("child-1"), "subagent_id")),
        clock(1_000),
    );
    assert_eq!(
        applied,
        Applied {
            changed: false,
            missing_id: true
        }
    );
    assert_eq!(tree.version(), before);
    let empty = with(payload("child-1"), "subagent_id", json!(""));
    let applied = tree.apply(&event("subagent.tool", empty), clock(2_000));
    assert_eq!(
        applied,
        Applied {
            changed: false,
            missing_id: true
        }
    );
    assert_eq!(tree.version(), before);
}

#[test]
fn fields_merge_one_by_one() {
    let mut tree = SubagentTree::new();
    tree.apply(&event("subagent.start", payload("child-1")), clock(1_000));
    let row = tree.get("child-1").expect("row").clone();
    assert_eq!(row.goal, "goal child-1");
    assert_eq!(row.tool_count, None);
    assert!(row.started_at.is_none());

    let richer = with(
        with(payload("child-1"), "goal", json!("a better goal")),
        "tool_count",
        json!(3),
    );
    tree.apply(&event("subagent.progress", richer), clock(2_000));
    let row = tree.get("child-1").expect("row").clone();
    assert_eq!(row.goal, "a better goal");
    assert_eq!(row.tool_count, Some(3));

    let poorer = with(payload("child-1"), "tool_count", json!(1));
    tree.apply(&event("subagent.progress", poorer), clock(3_000));
    let row = tree.get("child-1").expect("row").clone();
    assert_eq!(row.goal, "goal child-1");
    assert_eq!(row.tool_count, Some(3), "tool_count keeps the max");

    let empty_goal = with(payload("child-1"), "goal", json!(""));
    tree.apply(&event("subagent.progress", empty_goal), clock(4_000));
    assert_eq!(tree.get("child-1").expect("row").goal, "goal child-1");

    let delegation = with(payload("child-1"), "delegation_id", json!("deleg-1"));
    tree.apply(&event("subagent.start", delegation), clock(5_000));
    assert_eq!(
        tree.get("child-1").expect("row").delegation_id.as_deref(),
        Some("deleg-1")
    );
    tree.apply(
        &event("subagent.progress", payload("child-1")),
        clock(6_000),
    );
    assert_eq!(
        tree.get("child-1").expect("row").delegation_id.as_deref(),
        Some("deleg-1"),
        "a later absent value keeps"
    );
    let moved = with(payload("child-1"), "delegation_id", json!("deleg-2"));
    tree.apply(&event("subagent.progress", moved), clock(7_000));
    assert_eq!(
        tree.get("child-1").expect("row").delegation_id.as_deref(),
        Some("deleg-2"),
        "a later different value replaces"
    );
    let nulled = with(payload("child-1"), "delegation_id", Value::Null);
    tree.apply(&event("subagent.progress", nulled), clock(7_500));
    assert_eq!(
        tree.get("child-1").expect("row").delegation_id.as_deref(),
        Some("deleg-2"),
        "null keeps"
    );

    tree.reconcile(
        &list_result(json!([with(
            snapshot("child-1"),
            "started_at",
            json!(100.0)
        )])),
        clock(8_000),
    );
    assert_eq!(tree.get("child-1").expect("row").started_at, Some(100.0));
}

#[test]
fn activity_keeps_24_and_drops_repeats() {
    let mut tree = SubagentTree::new();
    tree.apply(&event("subagent.start", payload("child-1")), clock(1_000));
    let preview = "p".repeat(200);
    let tool = with(
        with(payload("child-1"), "tool_name", json!("Terminal")),
        "tool_preview",
        json!(preview),
    );
    tree.apply(&event("subagent.tool", tool), clock(2_000));
    let row = tree.get("child-1").expect("row").clone();
    assert_eq!(row.activity.len(), 1);
    let line = &row.activity[0].text;
    assert!(line.starts_with("Terminal(\""), "{line}");
    let inner: String = line.chars().skip(10).take(96).collect();
    assert_eq!(inner.chars().count(), 96);
    assert!(inner.ends_with('…'), "{line}");

    let tail_preview = "q".repeat(600);
    let tail = with(
        payload("child-1"),
        "output_tail",
        json!([{"tool": "ReadFile", "preview": tail_preview, "is_error": false}]),
    );
    tree.apply(&event("subagent.tool", tail), clock(3_000));
    let row = tree.get("child-1").expect("row").clone();
    // The tool rule may or may not add a bare line when the tool name is
    // absent; the pinned half is the output_tail line, cut the same way.
    assert!(row.activity.len() >= 2, "{}", row.activity.len());
    let tailed = row
        .activity
        .iter()
        .find(|line| line.text.starts_with("ReadFile(\""))
        .expect("the output_tail line");
    let inner: String = tailed.text.chars().skip(10).take(96).collect();
    assert_eq!(inner.chars().count(), 96);
    assert!(inner.ends_with('…'), "{}", tailed.text);

    let progress = with(payload("child-1"), "text", json!("half done"));
    tree.apply(&event("subagent.progress", progress.clone()), clock(4_000));
    tree.apply(&event("subagent.progress", progress), clock(5_000));
    let row = tree.get("child-1").expect("row").clone();
    assert_eq!(
        row.activity
            .iter()
            .filter(|line| line.text == "half done")
            .count(),
        1
    );

    for index in 0..30 {
        let tool = with(
            with(
                payload("child-1"),
                "tool_name",
                json!(format!("Tool{index}")),
            ),
            "tool_preview",
            json!(format!("run {index}")),
        );
        tree.apply(&event("subagent.tool", tool), clock(6_000 + index));
    }
    let row = tree.get("child-1").expect("row").clone();
    assert_eq!(row.activity.len(), 24);
    assert!(
        row.activity.back().expect("newest").text.contains("run 29"),
        "newest last"
    );
}

#[test]
fn tree_order_and_dangling_parents() {
    let mut tree = SubagentTree::new();
    tree.apply(&event("subagent.start", payload("root")), clock(1_000));
    let child = with(payload("leaf-child"), "parent_id", json!("root"));
    tree.apply(&event("subagent.start", child), clock(2_000));
    let grandchild = with(payload("leaf-grand"), "parent_id", json!("leaf-child"));
    tree.apply(&event("subagent.start", grandchild), clock(3_000));
    let orphan = with(payload("orphan"), "parent_id", json!("nobody"));
    tree.apply(&event("subagent.start", orphan), clock(4_000));
    let order = tree.rows_in_order();
    let depths: Vec<(String, u8)> = order
        .iter()
        .map(|row| (tree.row(row.index).id.clone(), row.depth))
        .collect();
    assert_eq!(
        depths,
        vec![
            ("root".to_owned(), 0),
            ("leaf-child".to_owned(), 1),
            ("leaf-grand".to_owned(), 2),
            ("orphan".to_owned(), 0),
        ]
    );

    let mut cyclic = SubagentTree::new();
    let a_parent_b = with(payload("cycle-a"), "parent_id", json!("cycle-b"));
    cyclic.apply(&event("subagent.start", a_parent_b), clock(1_000));
    let b_parent_a = with(payload("cycle-b"), "parent_id", json!("cycle-a"));
    cyclic.apply(&event("subagent.start", b_parent_a), clock(2_000));
    let mut seen: Vec<String> = cyclic
        .rows_in_order()
        .iter()
        .map(|row| cyclic.row(row.index).id.clone())
        .collect();
    seen.sort();
    assert_eq!(seen, vec!["cycle-a".to_owned(), "cycle-b".to_owned()]);

    let mut mixed = SubagentTree::new();
    mixed.apply(
        &event("subagent.start", payload("from-event")),
        Clock {
            mono_ms: 1_000,
            wall_s: 50.0,
        },
    );
    mixed.reconcile(
        &list_result(json!([with(
            snapshot("from-list"),
            "started_at",
            json!(100.0)
        )])),
        clock(2_000),
    );
    let ids: Vec<String> = mixed
        .rows_in_order()
        .iter()
        .map(|row| mixed.row(row.index).id.clone())
        .collect();
    assert_eq!(ids, vec!["from-event".to_owned(), "from-list".to_owned()]);
}

#[test]
fn reconcile_adds_and_upgrades() {
    let mut tree = SubagentTree::new();
    let listed = with(snapshot("child-1"), "last_tool", json!("Terminal"));
    tree.reconcile(&list_result(json!([listed])), clock(1_000));
    let row = tree.get("child-1").expect("listed row").clone();
    assert_eq!(row.state, RowState::Running);
    assert!(!row.seen_live);
    assert_eq!(row.activity.len(), 1);
    assert_eq!(row.activity[0].text, "Terminal");

    let mut queued = SubagentTree::new();
    queued.apply(
        &event("subagent.spawn_requested", payload("child-2")),
        clock(1_000),
    );
    queued.reconcile(&list_result(json!([snapshot("child-2")])), clock(2_000));
    assert_eq!(state_of(&queued, "child-2"), RowState::Running);

    let mut finished = SubagentTree::new();
    let done = with(snapshot("child-3"), "status", json!("completed"));
    finished.reconcile(&list_result(json!([done])), clock(1_000));
    assert!(finished.get("child-3").is_none());
}

#[test]
fn absent_twice_is_gone_queued_exempt() {
    let mut tree = SubagentTree::new();
    tree.reconcile(&list_result(json!([snapshot("child-a")])), clock(1_000));
    tree.reconcile(&list_result(json!([])), clock(2_000));
    assert_eq!(state_of(&tree, "child-a"), RowState::Running);
    tree.reconcile(&list_result(json!([])), clock(3_000));
    assert_eq!(state_of(&tree, "child-a"), RowState::Gone);

    let mut queued = SubagentTree::new();
    queued.apply(
        &event("subagent.spawn_requested", payload("child-q")),
        clock(1_000),
    );
    for tick in 2..5 {
        queued.reconcile(&list_result(json!([])), clock(tick * 1_000));
    }
    assert_eq!(state_of(&queued, "child-q"), RowState::Queued);
}

#[test]
fn queued_row_expires_after_ten_minutes() {
    let mut tree = SubagentTree::new();
    tree.apply(
        &event("subagent.spawn_requested", payload("child-1")),
        clock(0),
    );
    assert!(!tree.expire(clock(599_999)));
    assert_eq!(state_of(&tree, "child-1"), RowState::Queued);
    assert!(tree.expire(clock(600_000)));
    assert!(tree.get("child-1").is_none());
    assert!(
        tree.rows_in_order().is_empty(),
        "expiry removes the row from the order"
    );
    assert_eq!(tree.counts().queued, 0);
    tree.apply(&event("subagent.start", payload("child-1")), clock(601_000));
    assert_eq!(
        state_of(&tree, "child-1"),
        RowState::Running,
        "expiry never retires: a later start recreates the row"
    );
}

#[test]
fn stop_requested_until_terminal() {
    let mut tree = SubagentTree::new();
    assert!(!tree.mark_stop_requested("child-1"));
    tree.apply(&event("subagent.start", payload("child-1")), clock(1_000));
    assert!(tree.mark_stop_requested("child-1"));
    assert!(tree.get("child-1").expect("row").stop_requested);
    let done = with(payload("child-1"), "status", json!("completed"));
    tree.apply(&event("subagent.complete", done), clock(2_000));
    assert!(!tree.get("child-1").expect("row").stop_requested);
}

#[test]
fn counts_and_version() {
    let mut tree = SubagentTree::new();
    tree.apply(
        &event("subagent.spawn_requested", payload("queued")),
        clock(1_000),
    );
    for id in ["done", "bad", "stopped", "gone"] {
        tree.apply(&event("subagent.start", payload(id)), clock(2_000));
    }
    for (id, status) in [
        ("done", "completed"),
        ("bad", "failed"),
        ("stopped", "interrupted"),
    ] {
        let complete = with(payload(id), "status", json!(status));
        tree.apply(&event("subagent.complete", complete), clock(3_000));
    }
    tree.reconcile(&list_result(json!([])), clock(4_000));
    tree.reconcile(&list_result(json!([])), clock(5_000));
    assert_eq!(state_of(&tree, "gone"), RowState::Gone);
    tree.apply(&event("subagent.start", payload("live")), clock(6_000));
    assert_eq!(
        tree.counts(),
        SubagentCounts {
            live: 2,
            queued: 1,
            terminal: 4,
            failed: 2,
            stopped: 1,
        }
    );

    let mut versions = SubagentTree::new();
    let v0 = versions.version();
    versions.apply(&event("subagent.start", payload("child-1")), clock(1_000));
    assert_eq!(versions.version(), v0 + 1, "a new row raises the version");
    let done = with(payload("child-1"), "status", json!("completed"));
    versions.apply(&event("subagent.complete", done), clock(2_000));
    assert_eq!(
        versions.version(),
        v0 + 2,
        "a state change raises the version"
    );
    versions.apply(&event("subagent.start", payload("child-2")), clock(3_000));
    let v1 = versions.version();
    let progress = with(payload("child-2"), "text", json!("working"));
    versions.apply(&event("subagent.progress", progress.clone()), clock(4_000));
    assert_eq!(
        versions.version(),
        v1 + 1,
        "an activity line raises the version"
    );
    versions.apply(&event("subagent.progress", progress), clock(5_000));
    assert_eq!(versions.version(), v1 + 1, "a repeated event leaves it");
    let started = with(snapshot("child-2"), "started_at", json!(50.0));
    versions.reconcile(&list_result(json!([started.clone()])), clock(6_000));
    let v2 = versions.version();
    assert_eq!(v2, v1 + 2, "a merge that sets a field raises the version");
    versions.reconcile(&list_result(json!([started])), clock(7_000));
    assert_eq!(versions.version(), v2, "an identical list leaves it");
    assert!(!versions.expire(clock(8_000)));
    assert_eq!(
        versions.version(),
        v2,
        "an expire that removes nothing leaves it"
    );
}

mod invariants {
    use std::collections::{BTreeMap, BTreeSet};

    use proptest::prelude::*;
    use serde_json::{Value, json};

    use super::{clock, event, list_result, payload, snapshot, with};

    #[derive(Clone, Debug)]
    enum Op {
        Event {
            kind: usize,
            id: usize,
            status: Option<usize>,
        },
        List {
            present: Vec<bool>,
        },
        Prune,
        Expire {
            step_ms: u64,
        },
    }

    fn op_strategy() -> impl Strategy<Value = Op> {
        prop_oneof![
            (0..6usize, 0..12usize, proptest::option::of(0..7usize))
                .prop_map(|(kind, id, status)| Op::Event { kind, id, status }),
            proptest::collection::vec(any::<bool>(), 12).prop_map(|present| Op::List { present }),
            Just(Op::Prune),
            (0..700_000u64).prop_map(|step_ms| Op::Expire { step_ms }),
        ]
    }

    fn words() -> [&'static str; 7] {
        [
            "completed",
            "failed",
            "error",
            "timeout",
            "interrupted",
            "queued",
            "running",
        ]
    }

    fn kinds() -> [&'static str; 6] {
        [
            "subagent.spawn_requested",
            "subagent.start",
            "subagent.progress",
            "subagent.thinking",
            "subagent.tool",
            "subagent.complete",
        ]
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(64))]
        #[test]
        fn tree_invariants(ops in proptest::collection::vec(op_strategy(), 0..200)) {
            let mut tree = super::SubagentTree::new();
            let mut now_ms = 1_000u64;
            let mut version = tree.version();
            let mut was_terminal: BTreeMap<String, bool> = BTreeMap::new();
            for op in &ops {
                match op {
                    Op::Event { kind, id, status } => {
                        let child = format!("child-{id}");
                        let mut body = payload(&child);
                        if let Some(word) = status {
                            body = with(body, "status", json!(words()[*word]));
                        }
                        tree.apply(&event(kinds()[*kind], body), clock(now_ms));
                    }
                    Op::List { present } => {
                        let snapshots: Vec<Value> = present
                            .iter()
                            .enumerate()
                            .filter(|(_, on)| **on)
                            .map(|(id, _)| snapshot(&format!("child-{id}")))
                            .collect();
                        tree.reconcile(&list_result(Value::Array(snapshots)), clock(now_ms));
                    }
                    Op::Prune => {
                        tree.prune_finished();
                    }
                    Op::Expire { step_ms } => {
                        now_ms += *step_ms;
                        tree.expire(clock(now_ms));
                    }
                }
                now_ms += 1;
                assert!(tree.version() >= version, "the version never falls");
                version = tree.version();
                let order = tree.rows_in_order();
                let mut ids = BTreeSet::new();
                for row in &order {
                    let id = tree.row(row.index).id.clone();
                    assert!(ids.insert(id.clone()), "ids are unique");
                    let live = tree.row(row.index).state.is_live();
                    if was_terminal.get(&id).copied().unwrap_or(false) {
                        assert!(!live, "a terminal row never becomes live");
                    } else if !live {
                        was_terminal.insert(id, true);
                    }
                }
                let counts = tree.counts();
                assert_eq!(
                    counts.live as usize + counts.terminal as usize,
                    order.len(),
                    "every row appears exactly once in the order"
                );
                assert!(counts.queued <= counts.live);
                assert!(counts.failed <= counts.terminal);
                assert!(counts.stopped <= counts.terminal);
            }
        }
    }
}
