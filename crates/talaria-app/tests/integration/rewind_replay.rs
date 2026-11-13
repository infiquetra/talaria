//! The recorded rewind fixture drives the pure rewind functions. Issue 233.
//! Checkpoint rewind and diff.

use std::path::Path;

use hermes_client::ReplaySource;
use serde_json::{Value, json};
use talaria_core::rewind::{RestoreOutcome, restore_summary, rewind_turns};

fn fixture_path() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/frames/rewind-and-rollback.jsonl")
}

fn resume_messages(text: &str) -> Vec<Value> {
    let mut latest = Vec::new();
    for line in text.lines() {
        let Ok(record) = serde_json::from_str::<Value>(line) else {
            continue;
        };
        let Some(messages) = record
            .pointer("/frame/result/messages")
            .and_then(Value::as_array)
        else {
            continue;
        };
        if !messages.is_empty() {
            latest = messages.clone();
        }
    }
    latest
}

fn rewound_count(text: &str) -> Option<u64> {
    for line in text.lines() {
        let Ok(record) = serde_json::from_str::<Value>(line) else {
            continue;
        };
        let payload = record.pointer("/frame/params/payload");
        if payload
            .and_then(|value| value.get("operation"))
            .and_then(Value::as_str)
            == Some("rewind")
            && let Some(count) = payload
                .and_then(|value| value.get("rewound_count"))
                .and_then(Value::as_u64)
        {
            return Some(count);
        }
    }
    None
}

#[test]
fn recorded_rewind_and_rollback_frames_drive_expected_screens() {
    let path = fixture_path();
    let text = std::fs::read_to_string(&path).expect("recorded rewind fixture");
    let messages = resume_messages(&text);
    assert!(!messages.is_empty(), "the recording holds a session resume");
    let turns = rewind_turns(&messages);
    let source = ReplaySource::open(&path).expect("replay opens");
    assert!(source.inbound().next().is_some());
    let editing: Vec<_> = turns.iter().filter(|turn| turn.edits_files).collect();
    assert!(
        editing.len() >= 2,
        "both original prompts record a write_file or patch through function.name"
    );
    let summary = restore_summary(
        &RestoreOutcome {
            success: true,
            restored_to: Some("abcd1234".to_string()),
            reason: Some("before write_file".to_string()),
            file: None,
            restored_files: None,
            skipped_user_edits: None,
            skipped_oversize: None,
            failed_deletes: None,
            history_removed: Some(1),
            error: None,
        },
        "2",
    );
    let joined = summary.join("\n");
    assert!(joined.contains("abcd1234"));
    assert!(!joined.contains("directory"));
    assert!(!joined.contains('/'));
    let count = rewound_count(&text).expect("a rewind receipt");
    assert!(count > 0);
    let _ = json!(count);
}
