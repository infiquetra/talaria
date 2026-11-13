//! Eligible prompts. Issue 233. Checkpoint rewind and diff.

use serde_json::{Value, json};
use talaria_core::rewind::{RewindTurn, is_active_row, lossless_text, rewind_turns};

fn user(id: i64, content: Value, extra: Value) -> Value {
    let mut row = json!({"role": "user", "row_id": id, "content": content});
    if let (Some(row), Some(extra)) = (row.as_object_mut(), extra.as_object()) {
        for (key, value) in extra {
            row.insert(key.clone(), value.clone());
        }
    }
    row
}

fn text_part(kind: &str, text: &str) -> Value {
    json!({"type": kind, "text": text})
}

fn by_id(turns: &[RewindTurn], row_id: i64) -> &RewindTurn {
    turns
        .iter()
        .find(|turn| turn.row_id == row_id)
        .unwrap_or_else(|| panic!("missing row {row_id}"))
}

#[test]
fn eligible_user_rows_newest_first_with_ordinals() {
    let messages = vec![
        user(1, json!("one"), json!({})),
        json!({"role": "assistant", "content": "answer", "row_id": 2}),
        json!({"role": "tool", "content": "tool", "row_id": 3}),
        user(5, json!("steer me"), json!({"display_kind": "steer"})),
        user(7, json!("archived"), json!({"archived": true})),
        user(9, json!("hidden"), json!({"display_kind": "hidden"})),
        user(11, json!("eleven"), json!({})),
        user(13, json!("thirteen"), json!({"display_kind": null})),
    ];
    let turns = rewind_turns(&messages);
    let pairs: Vec<(i64, usize)> = turns
        .iter()
        .map(|turn| (turn.row_id, turn.ordinal))
        .collect();
    assert_eq!(pairs, vec![(13, 4), (11, 3), (5, 2), (1, 1)]);
}

#[test]
fn rows_without_positive_integer_row_id_are_skipped() {
    let messages = vec![
        user(
            1,
            json!("kept only if the id is a positive integer"),
            json!({}),
        ),
        json!({"role": "user", "content": "null id", "row_id": null}),
        json!({"role": "user", "content": "zero", "row_id": 0}),
        json!({"role": "user", "content": "negative", "row_id": -3}),
        json!({"role": "user", "content": "float", "row_id": 2.5}),
        json!({"role": "user", "content": "string", "row_id": "7"}),
        json!({"role": "user", "content": "missing"}),
    ];
    // The invalid rows are the subject. Drop the valid one so none remain.
    let invalid: Vec<Value> = messages.into_iter().skip(1).collect();
    let turns = rewind_turns(&invalid);
    assert!(turns.is_empty());
}

#[test]
fn compaction_carriers_and_archived_rows_are_skipped() {
    let markers = [
        "[CONTEXT COMPACTION start",
        "prefix [CONTEXT SUMMARY]: rest",
        "before [END OF PRIOR CONTEXT after",
    ];
    let mut messages = Vec::new();
    for (index, marker) in markers.iter().enumerate() {
        let id = (index as i64) + 1;
        messages.push(user(id, json!(marker), json!({})));
        messages.push(user(id + 10, json!([text_part("text", marker)]), json!({})));
    }
    messages.push(user(30, json!("still here"), json!({"archived": true})));
    messages.push(user(31, json!("live"), json!({})));
    let turns = rewind_turns(&messages);
    assert_eq!(turns.len(), 1);
    assert_eq!(turns[0].row_id, 31);
    assert_eq!(turns[0].ordinal, 1);
}

#[test]
fn prompt_text_from_string_or_text_parts() {
    let string_row = user(1, json!("plain"), json!({}));
    let parts = user(
        2,
        json!([
            text_part("input_text", "alpha"),
            text_part("input_text", "beta")
        ]),
        json!({}),
    );
    let image = user(
        3,
        json!([text_part("text", "caption"), {"type": "image_url", "image_url": {"url": "x"}}]),
        json!({}),
    );
    let extra = user(
        4,
        json!([{"type": "text", "text": "nope", "extra": true}]),
        json!({}),
    );
    let empty = user(5, json!(""), json!({}));
    let turns = rewind_turns(&[string_row, parts, image, extra, empty]);
    assert_eq!(by_id(&turns, 1).prompt_text.as_deref(), Some("plain"));
    assert_eq!(
        by_id(&turns, 2).prompt_text.as_deref(),
        Some("alphabeta"),
        "text parts join with no separator"
    );
    let image_turn = by_id(&turns, 3);
    assert_eq!(image_turn.prompt_text, None);
    assert!(image_turn.attachments);
    assert_eq!(image_turn.text_parts, "caption");
    assert_eq!(by_id(&turns, 4).prompt_text, None);
    assert_eq!(lossless_text(&json!("")), Some(String::new()));
    assert_eq!(by_id(&turns, 5).prompt_text.as_deref(), Some(""));
    assert_eq!(by_id(&turns, 5).preview, "(no text)");
    assert!(is_active_row(&turns_as_rows(&turns), 1));
}

fn turns_as_rows(turns: &[RewindTurn]) -> Vec<Value> {
    turns
        .iter()
        .map(|turn| json!({"role": "user", "row_id": turn.row_id, "content": ""}))
        .collect()
}

#[test]
fn later_count_and_turn_window() {
    let patch = json!({
        "id": "call-1",
        "type": "function",
        "function": {"name": "patch", "arguments": "{}"}
    });
    let terminal = json!({
        "id": "call-2",
        "type": "function",
        "function": {"name": "terminal", "arguments": "{}"}
    });
    let bare = json!({"name": "patch"});
    let messages = vec![
        json!({"role": "user", "row_id": 1, "content": "first", "timestamp": 1000}),
        json!({"role": "assistant", "row_id": 2, "content": "", "timestamp": 1100, "tool_calls": [terminal]}),
        json!({"role": "user", "row_id": 5, "content": "target", "timestamp": 1050}),
        json!({"role": "user", "row_id": 6, "content": "gone", "timestamp": 1060, "archived": true}),
        json!({"role": "assistant", "row_id": 7, "content": "", "archived": true}),
        json!({"role": "assistant", "row_id": 8, "content": "", "tool_calls": [patch]}),
        json!({"role": "user", "row_id": 11, "content": "next", "timestamp": 1300}),
        json!({"role": "assistant", "row_id": 12, "content": "", "tool_calls": [bare]}),
        json!({"role": "user", "row_id": 13, "content": "newest", "timestamp": 1400}),
        json!({"role": "assistant", "row_id": 14, "content": "done"}),
    ];
    let turns = rewind_turns(&messages);
    let target = by_id(&turns, 5);
    assert_eq!(target.later_count, 6, "archived rows 6 and 7 are excluded");
    assert_eq!(target.window_start, Some(1100));
    assert_eq!(target.window_end, Some(1300));
    assert!(target.edits_files);
    assert!(
        !by_id(&turns, 1).edits_files,
        "a terminal call is not a file edit"
    );
    assert!(
        !by_id(&turns, 11).edits_files,
        "a tool call without function.name is not a file edit"
    );
}
