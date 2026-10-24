//! Hand-built reducer events. Payload fields stay on `raw`.
//!
//! When `EventFrame::from_params` rejects a params object, this builder still returns an event.
//! It keeps the raw params and an `Unknown` notification, whose `type_name()` is the raw `type`.
//! The frame-log reader does not do that. It skips the frame and counts it.
//!
//! Each integration test is its own crate, so a helper used by one binary is dead in the others.

#![allow(dead_code)]

use hermes_contract::events::{EventFrame, Notification};
use serde_json::{Value, json};
use talaria_core::session::{Event, PromptKey, SessionId};

/// Session id used by the rule tests.
pub const SESSION: &str = "local-session-1";

/// Epoch used by the rule tests.
pub const EPOCH: &str = "epoch-1";

/// Build `Event::Session`. `outer_session` is the scope id, which may differ from `raw`.
pub fn session_event(outer_session: &str, mut raw: Value) -> Event {
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

/// An event for [`SESSION`] with a seq, generation 1 and [`EPOCH`].
pub fn ev(seq: u64, type_name: &str, payload: Value) -> Event {
    ev_full(SESSION, seq, 1, EPOCH, type_name, payload)
}

/// Same as [`ev`] with an explicit generation.
pub fn ev_gen(seq: u64, generation: i64, type_name: &str, payload: Value) -> Event {
    ev_full(SESSION, seq, generation, EPOCH, type_name, payload)
}

/// An event whose outer session id is `session`.
pub fn ev_session(session: &str, seq: u64, type_name: &str, payload: Value) -> Event {
    ev_full(session, seq, 1, EPOCH, type_name, payload)
}

/// An event with no `seq`. The cursor must not move.
pub fn ev_noseq(type_name: &str, payload: Value) -> Event {
    session_event(
        SESSION,
        json!({
            "type": type_name,
            "session_id": SESSION,
            "replay_epoch": EPOCH,
            "execution_generation": 1,
            "authority_epoch": 1,
            "payload": payload,
        }),
    )
}

fn ev_full(
    session: &str,
    seq: u64,
    generation: i64,
    epoch: &str,
    type_name: &str,
    payload: Value,
) -> Event {
    session_event(
        session,
        json!({
            "type": type_name,
            "session_id": session,
            "replay_epoch": epoch,
            "seq": seq,
            "execution_generation": generation,
            "authority_epoch": 1,
            "payload": payload,
        }),
    )
}

/// `Event::Submit`. Does not set `unattended`.
pub fn submit(text: &str, submission_id: &str) -> Event {
    Event::Submit {
        text: text.to_string(),
        submission_id: submission_id.to_string(),
    }
}

/// A prompt key for [`SESSION`].
pub fn key(prompt_id: &str, generation: i64) -> PromptKey {
    PromptKey {
        session_id: SessionId::new(SESSION),
        prompt_id: prompt_id.to_string(),
        execution_generation: generation,
    }
}

/// Approval request payload helpers.
pub fn approval_request(prompt_id: &str, generation: i64, choices: &[&str]) -> Value {
    json!({
        "kind": "approval",
        "prompt_id": prompt_id,
        "execution_generation": generation,
        "command": "rm -rf .pytest_cache",
        "description": "recursive delete",
        "choices": choices,
    })
}

/// Turn raw params into `Event::Session` when `EventFrame::from_params` accepts them.
///
/// Property tests drop a value the contract refuses. Rule tests use [`session_event`], which
/// keeps the raw object instead.
pub fn try_session_event(raw: &Value) -> Option<Event> {
    let frame = EventFrame::from_params(raw).ok()?;
    let session_id = frame.session_id.clone().unwrap_or_default();
    Some(Event::Session {
        session_id: SessionId::new(session_id),
        frame,
        raw: raw.clone(),
    })
}

/// Clarification request payload.
pub fn clarify_request(prompt_id: &str, generation: i64, question: &str) -> Value {
    json!({
        "kind": "clarify",
        "prompt_id": prompt_id,
        "execution_generation": generation,
        "question": question,
        "choices": ["skip"],
        "multi_select": false,
    })
}
