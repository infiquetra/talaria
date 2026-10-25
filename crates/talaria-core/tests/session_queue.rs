//! Issue 232. Queue and steer. Cells through the session reducer.
//!
//! Tests whose named failure is the current reducer do not call a queue stub.
//! `idle_submit_gets_one_cell` does: the fanout routing is the stub.

#[path = "support/events.rs"]
mod events;

use std::path::PathBuf;

use events::{EPOCH, SESSION, ev, session_event, submit};
use hermes_contract::canonical_events::SessionInfoFrame;
use hermes_contract::schemas::{
    AdmissionStatus, CanonicalCorrectionAck, CanonicalCorrectionAckStatus,
    CanonicalCorrectionResult, CanonicalPendingAdmission, CanonicalSnapshot, Status,
};
use serde_json::{Value, json};
use talaria_core::queue::{AdmissionId, QueueAction, SubmissionId};
use talaria_core::session::{Cell, Event, NoticeKind, SessionId, SessionState, TurnState, update};

fn user_texts(state: &SessionState) -> Vec<String> {
    state
        .cells()
        .iter()
        .filter_map(|cell| match cell {
            Cell::User(cell) => Some(cell.text.clone()),
            _ => None,
        })
        .collect()
}

fn notice_texts(state: &SessionState) -> Vec<String> {
    state
        .cells()
        .iter()
        .filter_map(|cell| match cell {
            Cell::Notice(cell) => Some(cell.text.clone()),
            _ => None,
        })
        .collect()
}

fn running() -> SessionState {
    let mut state = SessionState::empty(SessionId::new(SESSION));
    update(&mut state, ev(1, "message.start", json!({})));
    update(
        &mut state,
        ev(2, "message.delta", json!({"text": "reply so far"})),
    );
    state
}

fn receipt(id: &str, status: Status) -> AdmissionStatus {
    serde_json::from_value(json!({
        "admission_id": id,
        "authority_epoch": 1,
        "execution_generation": null,
        "outcome": null,
        "ref": {"profile_id": "fixture", "session_id": SESSION},
        "sequence": 1,
        "status": status
    }))
    .expect("contract schema AdmissionStatus")
}

fn row(id: &str, text: &str, status: &str, seq: i64) -> CanonicalPendingAdmission {
    serde_json::from_value(json!({
        "admission_id": id,
        "authority_epoch": 1,
        "execution_generation": null,
        "input_id": format!("in-{id}"),
        "outcome": null,
        "ref": {"profile_id": "fixture", "session_id": SESSION},
        "sequence": seq,
        "status": status,
        "text": text,
        "controls": ["steer", "replace"],
        "steering": false
    }))
    .expect("contract schema CanonicalPendingAdmission")
}

/// Settlement has no execution generation. The turn must stay open.
fn settlement(seq: u64, admission: &str, outcome: &str) -> Event {
    session_event(
        SESSION,
        json!({
            "type": "message.complete",
            "session_id": SESSION,
            "replay_epoch": EPOCH,
            "seq": seq,
            "authority_epoch": 1,
            "admission_id": admission,
            "payload": {"text": "", "outcome": outcome, "admission_id": admission}
        }),
    )
}

#[test]
fn cancelled_row_does_not_lend_its_text() {
    let mut state = running();
    update(&mut state, submit("alpha", "sub-a"));
    update(&mut state, submit("bravo", "sub-b"));
    update(
        &mut state,
        Event::QueueCallResult {
            action: QueueAction::Submit,
            admission_id: None,
            submission_id: Some(SubmissionId("sub-a".into())),
            result: receipt("adm-a", Status::Queued),
        },
    );
    update(
        &mut state,
        Event::QueueCallResult {
            action: QueueAction::Submit,
            admission_id: None,
            submission_id: Some(SubmissionId("sub-b".into())),
            result: receipt("adm-b", Status::Queued),
        },
    );
    update(
        &mut state,
        Event::QueueCallResult {
            action: QueueAction::Remove,
            admission_id: Some(AdmissionId("adm-a".into())),
            submission_id: Some(SubmissionId("sub-a".into())),
            result: receipt("adm-a", Status::Terminal),
        },
    );
    update(&mut state, settlement(3, "adm-a", "cancelled"));
    update(
        &mut state,
        session_event(
            SESSION,
            json!({
                "type": "message.start",
                "session_id": SESSION,
                "replay_epoch": EPOCH,
                "seq": 4,
                "execution_generation": 2,
                "authority_epoch": 1,
                "admission_id": "adm-b",
                "payload": {}
            }),
        ),
    );
    assert_eq!(user_texts(&state), vec!["bravo".to_string()]);
    let user_at = state
        .cells()
        .iter()
        .position(|cell| matches!(cell, Cell::User(_)))
        .expect("bravo's cell");
    let reply_at = state
        .cells()
        .iter()
        .position(|cell| matches!(cell, Cell::Assistant(_)))
        .expect("the reply");
    assert!(user_at > reply_at);
}

#[test]
fn edited_row_cell_shows_edited_text() {
    let mut state = running();
    update(&mut state, submit("alpha", "sub-a"));
    update(
        &mut state,
        Event::QueueCallResult {
            action: QueueAction::Edit,
            admission_id: Some(AdmissionId("adm-a".into())),
            submission_id: Some(SubmissionId("sub-a2".into())),
            result: receipt("adm-a2", Status::Queued),
        },
    );
    update(
        &mut state,
        session_event(
            SESSION,
            json!({
                "type": "session.info",
                "session_id": SESSION,
                "replay_epoch": EPOCH,
                "seq": 3,
                "execution_generation": 1,
                "authority_epoch": 1,
                "payload": {
                    "pending": [{
                        "admission_id": "adm-a2",
                        "authority_epoch": 1,
                        "execution_generation": null,
                        "input_id": "sub-a2",
                        "outcome": null,
                        "ref": {"profile_id": "fixture", "session_id": SESSION},
                        "sequence": 2,
                        "status": "queued",
                        "text": "alpha edited",
                        "controls": ["steer", "replace"],
                        "steering": false
                    }],
                    "running": true,
                    "execution_generation": 1,
                    "authority_epoch": 1,
                    "desktop_protocol": "hermes-gateway-v1",
                    "stored_session_id": "stored",
                    "revision": 1
                }
            }),
        ),
    );
    update(
        &mut state,
        session_event(
            SESSION,
            json!({
                "type": "message.start",
                "session_id": SESSION,
                "replay_epoch": EPOCH,
                "seq": 4,
                "execution_generation": 2,
                "authority_epoch": 1,
                "admission_id": "adm-a2",
                "payload": {}
            }),
        ),
    );
    assert_eq!(user_texts(&state), vec!["alpha edited".to_string()]);
}

#[test]
fn idle_submit_gets_one_cell() {
    let mut state = SessionState::empty(SessionId::new(SESSION));
    update(&mut state, submit("x", "sub-x"));
    assert_eq!(user_texts(&state), vec!["x".to_string()]);
    update(
        &mut state,
        Event::QueueCallResult {
            action: QueueAction::Submit,
            admission_id: None,
            submission_id: Some(SubmissionId("sub-x".into())),
            result: receipt("adm-x", Status::Started),
        },
    );
    update(
        &mut state,
        session_event(
            SESSION,
            json!({
                "type": "message.start",
                "session_id": SESSION,
                "replay_epoch": EPOCH,
                "seq": 1,
                "execution_generation": 1,
                "authority_epoch": 1,
                "admission_id": "adm-x",
                "payload": {}
            }),
        ),
    );
    let mut info = SessionInfoFrame {
        pending: Some(vec![row("adm-x", "x", "started", 1)]),
        running: Some(true),
        execution_generation: Some(1),
        revision: Some(1),
        authority_epoch: Some(1),
        stored_session_id: None,
        desktop_protocol: Some("hermes-gateway-v1".to_string()),
    };
    if let Some(rows) = info.pending.as_mut() {
        rows[0].input_id = "sub-x".to_string();
    }
    state.queue.apply_fanout(&info, EPOCH, 2);
    assert_eq!(user_texts(&state), vec!["x".to_string()]);
    assert!(state.queue.ledger.is_empty());

    let mut early = SessionState::empty(SessionId::new(SESSION));
    update(&mut early, submit("x", "sub-x"));
    update(
        &mut early,
        session_event(
            SESSION,
            json!({
                "type": "message.start",
                "session_id": SESSION,
                "replay_epoch": EPOCH,
                "seq": 1,
                "execution_generation": 1,
                "authority_epoch": 1,
                "admission_id": "adm-x",
                "payload": {}
            }),
        ),
    );
    early.queue.apply_fanout(&info, EPOCH, 2);
    update(
        &mut early,
        Event::QueueCallResult {
            action: QueueAction::Submit,
            admission_id: None,
            submission_id: Some(SubmissionId("sub-x".into())),
            result: receipt("adm-x", Status::Started),
        },
    );
    assert_eq!(user_texts(&early), vec!["x".to_string()]);
    assert!(early.queue.ledger.is_empty());
}

#[test]
fn start_matches_by_admission_not_arrival() {
    let mut state = running();
    update(
        &mut state,
        session_event(
            SESSION,
            json!({
                "type": "session.info",
                "session_id": SESSION,
                "replay_epoch": EPOCH,
                "seq": 3,
                "execution_generation": 1,
                "authority_epoch": 1,
                "payload": {
                    "pending": [{
                        "admission_id": "adm-y",
                        "authority_epoch": 1,
                        "execution_generation": null,
                        "input_id": "other-client",
                        "outcome": null,
                        "ref": {"profile_id": "fixture", "session_id": SESSION},
                        "sequence": 2,
                        "status": "queued",
                        "text": "y",
                        "controls": ["steer", "replace"],
                        "steering": false
                    }],
                    "running": true,
                    "execution_generation": 1,
                    "authority_epoch": 1,
                    "desktop_protocol": "hermes-gateway-v1",
                    "stored_session_id": "stored",
                    "revision": 1
                }
            }),
        ),
    );
    update(&mut state, submit("x", "sub-x"));
    update(
        &mut state,
        session_event(
            SESSION,
            json!({
                "type": "message.start",
                "session_id": SESSION,
                "replay_epoch": EPOCH,
                "seq": 4,
                "execution_generation": 2,
                "authority_epoch": 1,
                "admission_id": "adm-y",
                "payload": {}
            }),
        ),
    );
    update(
        &mut state,
        ev(5, "message.delta", json!({"text": "y's reply"})),
    );
    update(
        &mut state,
        ev(6, "message.complete", json!({"status": "complete"})),
    );
    update(
        &mut state,
        session_event(
            SESSION,
            json!({
                "type": "message.start",
                "session_id": SESSION,
                "replay_epoch": EPOCH,
                "seq": 7,
                "execution_generation": 3,
                "authority_epoch": 1,
                "admission_id": "adm-x",
                "payload": {}
            }),
        ),
    );
    let texts = user_texts(&state);
    assert!(
        texts.iter().any(|text| text == "y"),
        "the other client's row keeps its own text"
    );
    assert_eq!(texts.iter().filter(|text| text.as_str() == "x").count(), 1);
}

#[test]
fn accepted_steer_closes_segment() {
    let mut state = running();
    let ack = CanonicalCorrectionResult::CorrectionAck(CanonicalCorrectionAck {
        authority_epoch: 3,
        delivered: None,
        execution_generation: 1,
        status: CanonicalCorrectionAckStatus::Queued,
        text: "The code word is PELICAN.".to_string(),
    });
    update(&mut state, Event::SteerResult(ack));
    update(
        &mut state,
        ev(3, "message.delta", json!({"text": " after the steer"})),
    );
    let cells = state.cells();
    let notice_at = cells
        .iter()
        .position(|cell| match cell {
            Cell::Notice(notice) => {
                notice.text.starts_with("⤷ steer:")
                    && notice.text.contains("The code word is PELICAN.")
                    && notice.text.contains(" · sent;")
            }
            _ => false,
        })
        .expect("the steer notice");
    let first = cells.iter().find_map(|cell| match cell {
        Cell::Assistant(cell) => Some(cell),
        _ => None,
    });
    assert!(first.is_some_and(|cell| cell.sealed && cell.text == "reply so far"));
    let later = cells.iter().rposition(|cell| match cell {
        Cell::Assistant(cell) => cell.text.contains("after the steer"),
        _ => false,
    });
    assert!(later.is_some_and(|index| index > notice_at));
}

#[test]
fn settlement_frames_do_not_end_turn() {
    for (seq, admission, outcome) in [
        (3, "steer-row", "steered"),
        (3, "cancel-row", "cancelled"),
        (3, "replaced-row", "cancelled"),
    ] {
        let mut state = running();
        let before = state.cells().len();
        update(&mut state, settlement(seq, admission, outcome));
        assert!(
            matches!(state.turn(), TurnState::Running),
            "a queue settlement does not end the turn"
        );
        assert_eq!(state.cells().len(), before);
    }
}

fn resume_with_steer() -> CanonicalSnapshot {
    let path =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/frames/queue-steer.jsonl");
    let text = std::fs::read_to_string(&path).unwrap_or_else(|_| panic!("read the recording"));
    for line in text.lines() {
        let Ok(record) = serde_json::from_str::<Value>(line) else {
            continue;
        };
        let Some(result) = record
            .get("frame")
            .and_then(|frame| frame.get("result"))
            .cloned()
        else {
            continue;
        };
        let Some(messages) = result.get("messages").and_then(Value::as_array) else {
            continue;
        };
        let steered = messages
            .iter()
            .any(|message| message.get("display_kind").and_then(Value::as_str) == Some("steer"));
        if !steered {
            continue;
        }
        // The recorder redacts `info.launch_request` to a string; the contract's field is an
        // optional object, so the redacted value is dropped before decoding.
        let mut result = result;
        if let Some(info) = result.get_mut("info").and_then(Value::as_object_mut) {
            info.remove("launch_request");
        }
        return serde_json::from_value(result).unwrap_or_else(|_| panic!("resume snapshot"));
    }
    panic!("resume snapshot");
}

#[test]
fn resumed_steer_row_renders_as_notice() {
    let state = SessionState::from_snapshot(resume_with_steer());
    assert!(
        user_texts(&state)
            .iter()
            .all(|text| !text.contains("PELICAN")),
        "a steer is not a user cell"
    );
    let notices = notice_texts(&state);
    assert!(
        notices.iter().any(|text| {
            text.contains("PELICAN") && text.contains("⤷ steer:") && !text.contains(" · sent;")
        }),
        "history shows the steer without the live suffix"
    );
    assert!(
        notices
            .iter()
            .any(|text| text.contains("The code word is PELICAN."))
    );
}

#[test]
fn resumed_queued_row_gets_cell() {
    let snapshot: CanonicalSnapshot = serde_json::from_value(json!({
        "authority_epoch": 1,
        "execution_generation": 1,
        "info": {"desktop_protocol": "hermes-gateway-v1"},
        "last_sequence": 10,
        "message_count": 1,
        "messages": [{
            "role": "assistant",
            "content": "still working"
        }],
        "pending": [
            {
                "admission_id": "run",
                "authority_epoch": 1,
                "execution_generation": 1,
                "input_id": "in-run",
                "outcome": null,
                "ref": {"profile_id": "fixture", "session_id": SESSION},
                "sequence": 1,
                "status": "started",
                "text": "the running prompt",
                "controls": [],
                "steering": false
            },
            {
                "admission_id": "d",
                "authority_epoch": 1,
                "execution_generation": null,
                "input_id": "in-d",
                "outcome": null,
                "ref": {"profile_id": "fixture", "session_id": SESSION},
                "sequence": 2,
                "status": "queued",
                "text": "delta waits",
                "controls": ["steer", "replace"],
                "steering": false
            }
        ],
        "prompts": [],
        "replay_epoch": EPOCH,
        "revision": 1,
        "running": true,
        "session_id": SESSION,
        "stored_session_id": "stored",
        "subscription_id": "sub"
    }))
    .expect("contract schema CanonicalSnapshot");
    let mut state = SessionState::from_snapshot(snapshot);
    update(
        &mut state,
        session_event(
            SESSION,
            json!({
                "type": "message.start",
                "session_id": SESSION,
                "replay_epoch": EPOCH,
                "seq": 11,
                "execution_generation": 2,
                "authority_epoch": 1,
                "admission_id": "d",
                "payload": {}
            }),
        ),
    );
    assert_eq!(user_texts(&state), vec!["delta waits".to_string()]);
    let _ = NoticeKind::System;
}
