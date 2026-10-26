use super::*;
use hermes_contract::schemas::SubagentListResult;
use serde_json::{Value, json};

fn clock(ms: u64) -> Clock {
    Clock {
        mono_ms: ms,
        wall_s: ms as f64 / 1000.0,
    }
}

fn event(kind: SubagentEventKind, extra: Value) -> SubagentEvent {
    let mut payload = json!({"subagent_id":"child", "goal":"work", "task_index":0, "task_count":1});
    payload
        .as_object_mut()
        .unwrap()
        .extend(extra.as_object().unwrap().clone());
    SubagentEvent {
        kind,
        payload: serde_json::from_value(payload).unwrap(),
    }
}

fn list(rows: Value) -> SubagentListResult {
    serde_json::from_value(json!({"subagents":rows})).unwrap()
}

#[test]
fn repeated_queued_events_refresh_expiry_without_changing_version() {
    let mut tree = SubagentTree::new();
    let spawn = event(SubagentEventKind::SpawnRequested, json!({}));
    tree.apply(&spawn, clock(0));
    let version = tree.version();
    assert!(!tree.apply(&spawn, clock(500_000)).changed);
    assert_eq!(tree.version(), version);
    assert!(!tree.expire(clock(600_000)));
    assert!(tree.expire(clock(1_100_000)));
}

#[test]
fn reconciliation_resets_misses_and_replaces_steering_with_unknown() {
    let mut tree = SubagentTree::new();
    let ready = list(json!([{"subagent_id":"child", "accepting_steer":true}]));
    tree.reconcile(&ready, clock(0));
    tree.reconcile(&list(json!([])), clock(1));
    tree.reconcile(&list(json!([{"subagent_id":"child"}])), clock(2));
    assert_eq!(tree.get("child").unwrap().accepting_steer, None);
    tree.reconcile(&list(json!([])), clock(3));
    assert_eq!(tree.get("child").unwrap().state, RowState::Running);
    tree.reconcile(&list(json!([])), clock(4));
    assert_eq!(tree.get("child").unwrap().state, RowState::Gone);
}

#[test]
fn output_tail_precedes_timeout_summary_and_terminal_rows_stay_frozen() {
    let mut tree = SubagentTree::new();
    tree.apply(&event(SubagentEventKind::Start, json!({})), clock(0));
    tree.apply(
        &event(
            SubagentEventKind::Complete,
            json!({
                "status":"timeout", "duration_seconds":12.5,
                "output_tail":[{"tool":"read", "preview":" a\n b ", "is_error":true}]
            }),
        ),
        clock(1),
    );
    let row = tree.get("child").unwrap().clone();
    assert_eq!(
        row.activity[0],
        Activity {
            kind: ActivityKind::Tool,
            text: "read(\"a b\")".into(),
            is_error: true
        }
    );
    assert_eq!(
        row.activity[1],
        Activity {
            kind: ActivityKind::Summary,
            text: "Timed out after 12.5s".into(),
            is_error: true
        }
    );
    tree.apply(
        &event(
            SubagentEventKind::Start,
            json!({"goal":"changed", "delegation_id":"later"}),
        ),
        clock(2),
    );
    assert_eq!(tree.get("child"), Some(&row));
}

#[test]
fn finished_root_without_delegation_is_a_standalone_batch() {
    let mut tree = SubagentTree::new();
    tree.apply(&event(SubagentEventKind::Start, json!({})), clock(0));
    tree.apply(
        &event(SubagentEventKind::Complete, json!({"status":"completed"})),
        clock(1),
    );
    let batches = tree.take_finished_batches();
    assert_eq!(batches.len(), 1);
    assert_eq!(batches[0].key, "row:child");
    assert!(tree.prune_finished());
    assert!(
        !tree
            .apply(&event(SubagentEventKind::Start, json!({})), clock(2))
            .changed
    );
}

#[test]
fn parents_still_group_descendants_beyond_the_drawing_depth_cap() {
    let mut tree = SubagentTree::new();
    for depth in 0..12 {
        let mut fields = json!({"subagent_id":format!("child-{depth}"), "delegation_id":format!("delegation-{depth}")});
        if depth > 0 {
            fields["parent_id"] = json!(format!("child-{}", depth - 1));
        }
        tree.apply(
            &event(SubagentEventKind::Start, fields.clone()),
            clock(depth),
        );
        fields["status"] = json!("completed");
        tree.apply(&event(SubagentEventKind::Complete, fields), clock(depth));
    }
    let batch = tree.take_finished_batches();
    assert_eq!(batch.len(), 1);
    assert_eq!(batch[0].key, "delegation-0");
    assert_eq!(batch[0].rows.len(), 12);
    assert_eq!(batch[0].rows.last().unwrap().1, 8);
}
