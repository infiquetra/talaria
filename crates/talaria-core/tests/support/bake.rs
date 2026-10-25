//! Bake-off recordings. Each line is a direction and a JSON-RPC `raw` string.
//!
//! Issue 221. A comment at the call site names the recording and the `seq`.

#![allow(dead_code)]

use std::path::PathBuf;

use serde_json::{Value, json};
use talaria_core::session::{Event, SessionId, SessionState, update};

use super::events::session_event;

/// Every `event` params object in one recording, in file order.
pub fn recording(relative: &str) -> Vec<Value> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(relative);
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|_| panic!("the bake-off recording could not be read"));
    let mut events = Vec::new();
    for line in text.lines() {
        let Ok(row) = serde_json::from_str::<Value>(line) else {
            continue;
        };
        let Some(raw) = row
            .get("raw")
            .or_else(|| row.get("frame"))
            .and_then(Value::as_str)
        else {
            continue;
        };
        let Ok(frame) = serde_json::from_str::<Value>(raw) else {
            continue;
        };
        if frame.get("method").and_then(Value::as_str) != Some("event") {
            continue;
        }
        if let Some(params) = frame.get("params") {
            events.push(params.clone());
        }
    }
    assert!(!events.is_empty(), "the bake-off recording has no events");
    events
}

/// The params object whose `seq` is `seq`.
pub fn by_seq(events: &[Value], seq: u64) -> Value {
    events
        .iter()
        .find(|params| params.get("seq").and_then(Value::as_u64) == Some(seq))
        .cloned()
        .unwrap_or_else(|| panic!("the recording has no event at seq {seq}"))
}

/// Same envelope as `template`, with a new seq, type and payload.
pub fn retarget(template: &Value, seq: u64, type_name: &str, payload: Value) -> Value {
    let mut raw = template.clone();
    raw["seq"] = json!(seq);
    raw["type"] = json!(type_name);
    raw["payload"] = payload;
    raw
}

/// Apply one params object to `state`. The session id is the one the recording uses.
pub fn apply(state: &mut SessionState, raw: &Value) {
    let event = session_event("local-session-1", raw.clone());
    // The outer id has to be the state's id. `session_event` sets that.
    let _ = event_is_session(&event);
    update(state, event);
}

fn event_is_session(event: &Event) -> bool {
    matches!(event, Event::Session { session_id, .. } if session_id == &SessionId::new("local-session-1"))
}

/// A fresh session for the bake-off recordings.
pub fn empty() -> SessionState {
    SessionState::empty(SessionId::new("local-session-1"))
}
