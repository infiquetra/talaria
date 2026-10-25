//! Issue 232. Queue and steer. Recorded sessions replay to hand-written cells.
//!
//! The three queue recordings are already on disk. This file does not record them.
//! Cells come from `update` only. A wrong user cell is no queue handling.

use std::path::PathBuf;

use hermes_client::ReplaySource;
use hermes_contract::events::{EventFrame, Notification};
use hermes_contract::schemas::CanonicalSnapshot;
use serde_json::Value;
use talaria_core::session::{Cell, Event, SessionId, SessionState, update};

const SLEEP: &str =
    "Run the shell command sleep 20, then reply with the word done in capital letters.";
const BRAVO: &str = "Reply with the word bravo in capital letters.";
const ANCHOR: &str = "Reply with the word anchor in capital letters.";
const ONE: &str = "Reply with the word one in capital letters.";
const TWO: &str = "Reply with the word two in capital letters.";
const THREE: &str = "Reply with the word three in capital letters.";
const STEER_TURN: &str = "Run sleep 15 and then sleep 15 as two separate shell commands. Then reply with the code word I gave you followed by the digits 42 with no space, or NONE.";
const COUNT: &str = "Count from 1 to 30, one number per line.";
const SECOND: &str = "Reply with the single word: second.";

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

fn looks_like_snapshot(result: &Value) -> bool {
    let Some(object) = result.as_object() else {
        return false;
    };
    object.get("session_id").and_then(Value::as_str).is_some()
        && object.get("messages").and_then(Value::as_array).is_some()
        && object.get("replay_epoch").and_then(Value::as_str).is_some()
}

fn fallback(raw: &Value) -> EventFrame {
    EventFrame {
        session_id: raw
            .get("session_id")
            .and_then(Value::as_str)
            .map(str::to_owned),
        seq: raw.get("seq").and_then(Value::as_u64),
        replay_epoch: raw
            .get("replay_epoch")
            .and_then(Value::as_str)
            .map(str::to_owned),
        authority_epoch: raw.get("authority_epoch").and_then(Value::as_i64),
        execution_generation: raw.get("execution_generation").and_then(Value::as_i64),
        admission_id: raw
            .get("admission_id")
            .and_then(Value::as_str)
            .map(str::to_owned),
        notification: Notification::Unknown(raw.clone()),
    }
}

fn to_event(params: &Value) -> Event {
    let session = params
        .get("session_id")
        .and_then(Value::as_str)
        .unwrap_or("");
    let frame = EventFrame::from_params(params).unwrap_or_else(|_| fallback(params));
    Event::Session {
        session_id: SessionId::new(session),
        frame,
        raw: params.clone(),
    }
}

fn replay(file: &str) -> SessionState {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/frames")
        .join(file);
    let source = ReplaySource::open(&path).unwrap_or_else(|_| panic!("read the recording"));
    let mut initial = None;
    let mut events = Vec::new();
    let mut session_id = String::new();
    for frame in source.inbound() {
        let value = &frame.frame;
        if value.get("method").and_then(Value::as_str) == Some("event") {
            let params = value.get("params").cloned().unwrap_or(Value::Null);
            if session_id.is_empty()
                && let Some(id) = params.get("session_id").and_then(Value::as_str)
                && !id.is_empty()
            {
                session_id = id.to_string();
            }
            events.push(to_event(&params));
            continue;
        }
        let Some(result) = value.get("result") else {
            continue;
        };
        if initial.is_none() && looks_like_snapshot(result) {
            initial = serde_json::from_value::<CanonicalSnapshot>(result.clone()).ok();
        }
    }
    let mut state = match initial {
        Some(snapshot) => SessionState::from_snapshot(snapshot),
        None => SessionState::empty(SessionId::new(session_id)),
    };
    for event in events {
        update(&mut state, event);
    }
    state
}

#[test]
fn fixture_final_cells() {
    let edited = replay("queue-edit-remove.jsonl");
    assert_eq!(
        user_texts(&edited),
        vec![SLEEP.to_string(), BRAVO.to_string(), ANCHOR.to_string()],
        "queue-edit-remove: no queue handling"
    );

    let steered = replay("queue-steer.jsonl");
    let steered_users = user_texts(&steered);
    assert_eq!(
        steered_users,
        vec![STEER_TURN.to_string()],
        "queue-steer: no queue handling"
    );
    assert!(
        steered_users.iter().all(|text| !text.contains("PELICAN")),
        "queue-steer: a steer is not a user cell"
    );
    let steer_notice = notice_texts(&steered)
        .into_iter()
        .find(|text| text.contains("PELICAN"))
        .unwrap_or_else(|| panic!("queue-steer: no steer notice"));
    assert!(steer_notice.contains("⤷ steer:"));
    assert!(!steer_notice.contains(" · sent;"));

    let moved = replay("queue-move-send-now.jsonl");
    assert_eq!(
        user_texts(&moved),
        vec![
            SLEEP.to_string(),
            ONE.to_string(),
            THREE.to_string(),
            TWO.to_string()
        ],
        "queue-move-send-now: no queue handling"
    );

    let queued = replay("queued-prompt.jsonl");
    assert_eq!(
        user_texts(&queued),
        vec![COUNT.to_string(), SECOND.to_string()],
        "queued-prompt: no queue handling"
    );
}
