//! Eligible prompts for a rewind. Issue 233. Checkpoint rewind and diff.

use serde_json::Value;

/// One prompt a rewind can target. Ordinal 1 is the oldest.
#[derive(Debug, Clone, PartialEq)]
pub struct RewindTurn {
    /// Durable message id.
    pub row_id: i64,
    /// Position in transcript order. 1 is the oldest.
    pub ordinal: usize,
    /// First non-blank line, trimmed, or `(no text)`.
    pub preview: String,
    /// Lossless text, or `None` when a part is not a text part.
    pub prompt_text: Option<String>,
    /// Joined text of every text part.
    pub text_parts: String,
    /// A part is not a text part.
    pub attachments: bool,
    /// Stored time, when it is a JSON number.
    pub timestamp: Option<f64>,
    /// Start of the turn window, in whole seconds.
    pub window_start: Option<i64>,
    /// The next prompt's time, in whole seconds. `None` for the newest.
    pub window_end: Option<i64>,
    /// An assistant row in the window called `write_file` or `patch`.
    pub edits_files: bool,
    /// Active rows whose id is at least this prompt's, this prompt included.
    pub later_count: usize,
}

/// Markers a compaction carrier row holds.
const COMPACTION_MARKERS: [&str; 3] = [
    "[CONTEXT COMPACTION",
    "[CONTEXT SUMMARY]:",
    "[END OF PRIOR CONTEXT",
];

/// Part types that count as text.
const TEXT_PART_TYPES: [&str; 3] = ["input_text", "output_text", "text"];

/// Tool names that change files.
const EDIT_TOOLS: [&str; 2] = ["write_file", "patch"];

/// Eligible user prompts, newest first.
#[must_use]
pub fn rewind_turns(messages: &[Value]) -> Vec<RewindTurn> {
    // Ids of every active row with an integer id, sorted, for `later_count`.
    let mut active_ids: Vec<i64> = messages
        .iter()
        .filter(|row| !is_archived(row))
        .filter_map(integer_row_id)
        .collect();
    active_ids.sort_unstable();

    let mut turns: Vec<RewindTurn> = Vec::new();
    let mut newest_time: Option<f64> = None;
    for row in messages {
        if is_archived(row) {
            continue;
        }
        let timestamp = row.get("timestamp").and_then(Value::as_f64);
        if let Some(mut turn) = turn_from_row(row, timestamp, newest_time, &active_ids) {
            if let Some(previous) = turns.last_mut() {
                previous.window_end = timestamp.map(floor_seconds);
            }
            turn.ordinal = turns.len() + 1;
            turns.push(turn);
        } else if is_edit_row(row)
            && let Some(current) = turns.last_mut()
        {
            current.edits_files = true;
        }
        if let Some(time) = timestamp {
            newest_time = Some(newest_time.map_or(time, |newest| newest.max(time)));
        }
    }
    turns.reverse();
    turns
}

/// Lossless text of a message content value.
#[must_use]
pub fn lossless_text(content: &Value) -> Option<String> {
    match content {
        Value::String(text) => Some(text.clone()),
        Value::Array(parts) => {
            let mut joined = String::new();
            for part in parts {
                joined.push_str(exact_text_part(part)?);
            }
            Some(joined)
        }
        _ => None,
    }
}

/// The row is present and not archived.
#[must_use]
pub fn is_active_row(messages: &[Value], row_id: i64) -> bool {
    messages
        .iter()
        .find(|row| integer_row_id(row) == Some(row_id))
        .is_some_and(|row| !is_archived(row))
}

fn is_archived(row: &Value) -> bool {
    row.get("archived") == Some(&Value::Bool(true))
}

fn integer_row_id(row: &Value) -> Option<i64> {
    row.get("row_id").and_then(Value::as_i64)
}

fn floor_seconds(seconds: f64) -> i64 {
    seconds.floor() as i64
}

/// A part that is exactly `{type, text}` with a text type.
fn exact_text_part(part: &Value) -> Option<&str> {
    let object = part.as_object()?;
    if object.len() != 2 {
        return None;
    }
    text_of_part(part)
}

/// The text of a part with a text type, extra keys allowed.
fn text_of_part(part: &Value) -> Option<&str> {
    let kind = part.get("type")?.as_str()?;
    if !TEXT_PART_TYPES.contains(&kind) {
        return None;
    }
    part.get("text")?.as_str()
}

/// An active assistant row that called `write_file` or `patch`.
fn is_edit_row(row: &Value) -> bool {
    if row.get("role").and_then(Value::as_str) != Some("assistant") {
        return false;
    }
    let Some(calls) = row.get("tool_calls").and_then(Value::as_array) else {
        return false;
    };
    calls.iter().any(|call| {
        call.get("function")
            .and_then(|function| function.get("name"))
            .and_then(Value::as_str)
            .is_some_and(|name| EDIT_TOOLS.contains(&name))
    })
}

fn turn_from_row(
    row: &Value,
    timestamp: Option<f64>,
    newest_time: Option<f64>,
    active_ids: &[i64],
) -> Option<RewindTurn> {
    if row.get("role").and_then(Value::as_str) != Some("user") {
        return None;
    }
    let row_id = integer_row_id(row).filter(|id| *id > 0)?;
    match row.get("display_kind") {
        None | Some(Value::Null) => {}
        Some(Value::String(kind)) if kind == "steer" => {}
        Some(_) => return None,
    }
    let content = row.get("content").unwrap_or(&Value::Null);
    let (text_parts, attachments) = text_and_attachments(content);
    if COMPACTION_MARKERS
        .iter()
        .any(|marker| text_parts.contains(marker))
    {
        return None;
    }
    let preview = text_parts
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .unwrap_or("(no text)")
        .to_string();
    let window_start = timestamp.map(|time| {
        let own = floor_seconds(time);
        newest_time.map_or(own, |newest| own.max(floor_seconds(newest)))
    });
    let at_or_after = active_ids.len() - active_ids.partition_point(|id| *id < row_id);
    Some(RewindTurn {
        row_id,
        ordinal: 0,
        preview,
        prompt_text: lossless_text(content),
        text_parts,
        attachments,
        timestamp,
        window_start,
        window_end: None,
        edits_files: false,
        later_count: at_or_after,
    })
}

/// Joined text of the text parts, and whether any part is not a text part.
fn text_and_attachments(content: &Value) -> (String, bool) {
    match content {
        Value::String(text) => (text.clone(), false),
        Value::Array(parts) => {
            let mut joined = String::new();
            let mut attachments = false;
            for part in parts {
                match text_of_part(part) {
                    Some(text) => joined.push_str(text),
                    None => attachments = true,
                }
            }
            (joined, attachments)
        }
        _ => (String::new(), false),
    }
}
