//! Build a `CanonicalSnapshot` from the fields the reducer tests set.
//!
//! Omitted rows default to empty. `desktop_protocol` is always present because the contract
//! requires it. No profile name other than `fixture` is written.
//!
//! Each integration test is its own crate, so a helper used by one binary is dead in the others.

#![allow(dead_code)]

use hermes_contract::schemas::CanonicalSnapshot;
use serde_json::{Value, json};

pub fn snapshot(mut value: Value) -> CanonicalSnapshot {
    let object = value.as_object_mut().expect("snapshot JSON is an object");
    object
        .entry("info".to_string())
        .or_insert_with(|| json!({"desktop_protocol": "1"}));
    serde_json::from_value(value)
        .unwrap_or_else(|error| panic!("snapshot JSON is not a CanonicalSnapshot: {error}"))
}

pub fn base(session_id: &str) -> Value {
    json!({
        "session_id": session_id,
        "stored_session_id": "stored-1",
        "messages": [],
        "message_count": 0,
        "running": false,
        "authority_epoch": 1,
        "replay_epoch": "epoch-1",
        "last_sequence": 0,
        "subscription_id": "sub-1",
        "revision": 1,
        "execution_generation": 1,
        "pending": [],
        "prompts": [],
        "info": {"desktop_protocol": "1"}
    })
}
