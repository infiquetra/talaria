//! Issue 232. Queue and steer. Rows, fences and the count.

use hermes_contract::canonical_events::SessionInfoFrame;
use hermes_contract::schemas::CanonicalPendingAdmission;
use serde_json::{Value, json};
use talaria_core::queue::{QueueState, RowStatus};

fn admission(id: &str, seq: i64, status: &str, steering: bool) -> CanonicalPendingAdmission {
    // Contract schema CanonicalPendingAdmission.
    serde_json::from_value(json!({
        "admission_id": id,
        "authority_epoch": 1,
        "execution_generation": null,
        "input_id": format!("in-{id}"),
        "outcome": null,
        "ref": {"profile_id": "fixture", "session_id": "local-session-1"},
        "sequence": seq,
        "status": status,
        "text": id,
        "controls": if status == "queued" { json!(["steer", "replace"]) } else { Value::Null },
        "steering": steering
    }))
    .expect("contract schema CanonicalPendingAdmission")
}

fn frame(rows: Vec<CanonicalPendingAdmission>, epoch: i64, generation: i64) -> SessionInfoFrame {
    SessionInfoFrame {
        pending: Some(rows),
        running: Some(true),
        execution_generation: Some(generation),
        revision: Some(1),
        authority_epoch: Some(epoch),
        stored_session_id: None,
        desktop_protocol: Some("hermes-gateway-v1".to_string()),
    }
}

fn ids(state: &QueueState) -> Vec<String> {
    state
        .rows
        .iter()
        .map(|row| row.admission_id.0.clone())
        .collect()
}

#[test]
fn rows_sorted_by_sequence() {
    let mut state = QueueState::default();
    let info = frame(
        vec![
            admission("c", 30, "queued", false),
            admission("a", 10, "queued", false),
            admission("b", 20, "queued", false),
        ],
        1,
        1,
    );
    let delta = state.apply_fanout(&info, "epoch-a", 1);
    assert!(delta.applied);
    assert_eq!(ids(&state), ["a", "b", "c"]);
}

#[test]
fn count_includes_queued_and_unknown_only() {
    let mut state = QueueState::default();
    let info = frame(
        vec![
            admission("q", 1, "queued", false),
            admission("u", 2, "unknown", false),
            admission("run", 3, "started", false),
            admission("steer", 4, "started", true),
            admission("done", 5, "terminal", false),
        ],
        1,
        1,
    );
    state.apply_fanout(&info, "epoch-a", 1);
    assert_eq!(state.queued_count(), 2);
    let view = state.view(true);
    assert_eq!(view.rows.len(), 2);
    assert!(view.rows.iter().any(|row| row.status == RowStatus::Unknown));
    assert_eq!(view.steering.len(), 1);
    assert_eq!(view.steering[0].admission_id.0, "steer");
    assert!(view.rows.iter().all(|row| row.admission_id.0 != "done"));
    assert!(view.rows.iter().all(|row| row.admission_id.0 != "run"));
}

#[test]
fn stale_frames_are_ignored() {
    let mut state = QueueState::default();
    let first = frame(vec![admission("a", 1, "queued", false)], 1, 7);
    assert!(state.apply_fanout(&first, "epoch-a", 10).applied);

    let older_generation = frame(vec![admission("b", 2, "queued", false)], 1, 6);
    assert!(!state.apply_fanout(&older_generation, "epoch-a", 11).applied);
    assert_eq!(ids(&state), ["a"]);

    let older_seq = frame(vec![admission("c", 3, "queued", false)], 1, 7);
    assert!(!state.apply_fanout(&older_seq, "epoch-a", 9).applied);

    state.apply_snapshot(&[admission("s", 4, "queued", false)], 1, 7, "epoch-a", 20);
    let behind_snapshot = frame(vec![admission("d", 5, "queued", false)], 1, 8);
    assert!(!state.apply_fanout(&behind_snapshot, "epoch-a", 15).applied);
    assert_eq!(ids(&state), ["s"]);

    let higher_epoch = frame(vec![admission("e", 1, "queued", false)], 2, 1);
    assert!(state.apply_fanout(&higher_epoch, "epoch-a", 21).applied);
    assert_eq!(ids(&state), ["e"]);

    let new_replay = frame(vec![admission("f", 1, "queued", false)], 2, 1);
    assert!(state.apply_fanout(&new_replay, "epoch-b", 1).applied);
    assert_eq!(ids(&state), ["f"]);

    let lower_epoch = frame(vec![admission("g", 1, "queued", false)], 1, 9);
    assert!(!state.apply_fanout(&lower_epoch, "epoch-b", 2).applied);
    assert_eq!(ids(&state), ["f"]);
}

#[test]
fn frame_without_pending_leaves_queue() {
    let mut state = QueueState::default();
    let seeded = frame(vec![admission("a", 1, "queued", false)], 1, 1);
    state.apply_fanout(&seeded, "epoch-a", 1);
    let settings = SessionInfoFrame {
        pending: None,
        running: None,
        execution_generation: None,
        revision: None,
        authority_epoch: None,
        stored_session_id: Some("stored".to_string()),
        desktop_protocol: Some("hermes-gateway-v1".to_string()),
    };
    let delta = state.apply_fanout(&settings, "epoch-a", 2);
    assert!(!delta.applied);
    assert_eq!(ids(&state), ["a"]);
}

#[test]
fn unknown_head_pauses_queue() {
    let mut paused = QueueState::default();
    paused.apply_fanout(
        &frame(
            vec![
                admission("lost", 1, "unknown", false),
                admission("next", 2, "queued", false),
            ],
            1,
            1,
        ),
        "epoch-a",
        1,
    );
    assert!(paused.paused());

    let mut flowing = QueueState::default();
    flowing.apply_fanout(
        &frame(
            vec![
                admission("next", 1, "queued", false),
                admission("lost", 2, "unknown", false),
            ],
            1,
            1,
        ),
        "epoch-a",
        1,
    );
    assert!(!flowing.paused());
}

#[test]
fn queued_to_started_reported_once() {
    let mut state = QueueState::default();
    state.apply_fanout(
        &frame(vec![admission("a", 1, "queued", false)], 1, 1),
        "epoch-a",
        1,
    );
    let started = state.apply_fanout(
        &frame(vec![admission("a", 1, "started", false)], 1, 1),
        "epoch-a",
        2,
    );
    assert_eq!(started.started.len(), 1);
    assert_eq!(started.started[0].admission_id.0, "a");
    let repeat = state.apply_fanout(
        &frame(vec![admission("a", 1, "started", false)], 1, 1),
        "epoch-a",
        3,
    );
    assert!(repeat.started.is_empty());

    let mut from_snapshot = QueueState::default();
    from_snapshot.apply_snapshot(&[admission("run", 1, "started", false)], 1, 1, "epoch-a", 4);
    assert!(
        from_snapshot
            .rows
            .iter()
            .all(|row| row.status != RowStatus::Queued)
    );
}

#[test]
fn snapshot_rows_count_and_show() {
    let mut state = QueueState::default();
    state.apply_snapshot(
        &[
            admission("run", 1, "started", false),
            admission("a", 2, "queued", false),
            admission("b", 3, "queued", false),
        ],
        3,
        1,
        "epoch-a",
        20,
    );
    assert_eq!(state.queued_count(), 2);
    let view = state.view(true);
    assert_eq!(view.rows.len(), 2);
    assert!(state.known.iter().any(|id| id.0 == "a"));
    assert!(state.known.iter().any(|id| id.0 == "b"));
    assert_eq!(state.generation_fence, Some((3, 1)));
    assert_eq!(state.seq_fence.as_ref().map(|fence| fence.1), Some(20));
}
