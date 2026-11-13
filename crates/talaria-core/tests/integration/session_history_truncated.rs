//! The resume flag lives on the first stored row only. Issue 222.
//!
//! `messages[0]` set to the JSON boolean true is the only case that is truncated.
//! An absent flag, the same flag on the first display row, and a non-boolean are not.

use std::path::PathBuf;

use hermes_contract::schemas::CanonicalSnapshot;
use serde_json::{Value, json};
use talaria_core::session::SessionState;

fn resume_result() -> Value {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/data/resume_snapshot.jsonl");
    let text = std::fs::read_to_string(&path).expect("resume snapshot");
    for line in text.lines() {
        let Ok(value) = serde_json::from_str::<Value>(line) else {
            continue;
        };
        let Some(result) = value.pointer("/frame/result") else {
            continue;
        };
        if result.get("session_id").and_then(Value::as_str).is_some()
            && result.get("messages").and_then(Value::as_array).is_some()
        {
            return result.clone();
        }
    }
    panic!("resume snapshot has no result with messages");
}

fn with_archived_truncated_on_first_row(mut value: Value, flag: Option<Value>) -> Value {
    let row = value
        .get_mut("messages")
        .and_then(Value::as_array_mut)
        .and_then(|rows| rows.first_mut())
        .and_then(Value::as_object_mut)
        .expect("messages[0]");
    match flag {
        Some(flag) => {
            row.insert("archived_truncated".to_string(), flag);
        }
        None => {
            row.remove("archived_truncated");
        }
    }
    value
}

fn state_of(value: Value) -> SessionState {
    let snapshot =
        serde_json::from_value::<CanonicalSnapshot>(value).expect("resume result is a snapshot");
    SessionState::from_snapshot(snapshot)
}

#[test]
fn flag_from_first_row() {
    let raw = resume_result();
    assert!(
        raw.get("display_messages")
            .and_then(Value::as_array)
            .is_some(),
        "the resume result has display rows"
    );
    assert!(
        raw["messages"][0].get("text").is_some(),
        "messages[0] is a row"
    );

    let flagged = with_archived_truncated_on_first_row(raw.clone(), Some(json!(true)));
    assert_eq!(flagged["messages"][0]["archived_truncated"], json!(true));
    assert!(
        flagged["display_messages"][0]
            .get("archived_truncated")
            .is_none()
    );
    let state = state_of(flagged);
    assert!(state.history_truncated(), "messages[0] boolean true");

    let absent = with_archived_truncated_on_first_row(raw.clone(), None);
    assert!(!state_of(absent).history_truncated(), "absent");

    let mut display_only = with_archived_truncated_on_first_row(raw.clone(), None);
    display_only["display_messages"][0]["archived_truncated"] = json!(true);
    assert!(
        !state_of(display_only).history_truncated(),
        "display_messages[0] only"
    );

    let word = with_archived_truncated_on_first_row(raw.clone(), Some(json!("true")));
    assert!(
        !state_of(word).history_truncated(),
        "the string true is absent"
    );

    let number = with_archived_truncated_on_first_row(raw, Some(json!(1)));
    assert!(
        !state_of(number).history_truncated(),
        "the number 1 is absent"
    );
}
