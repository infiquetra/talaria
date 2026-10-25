//! Replay a frame log into session cells. Issue 221.
//!
//! The frame log is opened by `ReplaySource`. Each inbound `event` is applied by the reducer.

use std::path::Path;

use hermes_client::ReplaySource;
use hermes_contract::events::{EventFrame, Notification};
use serde_json::{Value, json};
use talaria_core::session::{Cell, Event, SessionId, SessionState, ToolCell, update};

/// Every inbound `event` params object, in `seq` order.
pub fn inbound_events(path: &Path) -> Vec<Value> {
    let source = open(path);
    source
        .inbound()
        .filter_map(|item| event_params(&item.frame))
        .collect()
}

/// Session state after every inbound event in the log.
pub fn replay_log(path: &Path) -> SessionState {
    let source = open(path);
    let session = session_of(&source).unwrap_or_else(|| panic!("the frame log has no session"));
    let mut state = SessionState::empty(SessionId::new(session.clone()));
    for item in source.inbound() {
        let Some(params) = event_params(&item.frame) else {
            continue;
        };
        let _ = update(&mut state, session_event(&session, params));
    }
    state
}

/// Tool cells in arrival order.
pub fn tool_cells(state: &SessionState) -> Vec<&ToolCell> {
    state
        .cells()
        .iter()
        .filter_map(|cell| match cell {
            Cell::Tool(tool) => Some(tool),
            _ => None,
        })
        .collect()
}

fn open(path: &Path) -> ReplaySource {
    ReplaySource::open(path)
        .unwrap_or_else(|error| panic!("the frame log could not be opened: {error}"))
}

fn session_of(source: &ReplaySource) -> Option<String> {
    source.inbound().find_map(|item| {
        let params = event_params(&item.frame)?;
        let id = params.get("session_id")?.as_str()?;
        if id.is_empty() {
            None
        } else {
            Some(id.to_owned())
        }
    })
}

fn event_params(frame: &Value) -> Option<Value> {
    if frame.get("method")?.as_str()? != "event" {
        return None;
    }
    frame.get("params").cloned()
}

fn session_event(outer_session: &str, mut raw: Value) -> Event {
    if let Some(object) = raw.as_object_mut() {
        object
            .entry("session_id".to_string())
            .or_insert_with(|| json!(outer_session));
    }
    let frame = match EventFrame::from_params(&raw) {
        Ok(frame) => frame,
        Err(_) => fallback_frame(&raw),
    };
    Event::Session {
        session_id: SessionId::new(outer_session),
        frame,
        raw,
    }
}

fn fallback_frame(raw: &Value) -> EventFrame {
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
