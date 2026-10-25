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

/// Eligible user prompts, newest first.
#[must_use]
pub fn rewind_turns(messages: &[Value]) -> Vec<RewindTurn> {
    let _ = messages;
    todo!("Issue 233. Checkpoint rewind and diff.")
}

/// Lossless text of a message content value.
#[must_use]
pub fn lossless_text(content: &Value) -> Option<String> {
    let _ = content;
    todo!("Issue 233. Checkpoint rewind and diff.")
}

/// The row is present and not archived.
#[must_use]
pub fn is_active_row(messages: &[Value], row_id: i64) -> bool {
    let _ = (messages, row_id);
    todo!("Issue 233. Checkpoint rewind and diff.")
}
