//! Read errors for the context and usage commands (#234).

use super::breakdown::FileStatus;

/// Why a context or usage read failed. The core never sees the client error.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ReadError {
    /// The gateway does not serve the method.
    MethodMissing,
    /// The session is not loaded.
    NotFound,
    /// The connection may not read the session.
    PermissionDenied,
    /// Any other refusal, with Hermes's reason.
    Refused { reason: String },
    /// No answer before the deadline, rounded up to whole seconds.
    TimedOut { secs: u64 },
    /// Not connected, or the connection dropped.
    NotConnected,
    /// The reply arrived but does not decode.
    Unreadable,
    /// Nothing was sent: a Talaria bug.
    NotSent,
}

impl ReadError {
    /// The full error row for a command, with its two-space indent.
    pub fn row(&self, command: &str, method: &str, sid: &str) -> String {
        let text = match self {
            ReadError::MethodMissing => format!(
                "This Hermes gateway does not serve {method}. Check that Hermes is at least the \
                 release Talaria is built for."
            ),
            ReadError::NotFound => format!(
                "Hermes does not have this session loaded. Resume it with talaria --resume {sid}."
            ),
            ReadError::PermissionDenied => {
                "Hermes refused: this connection may not read this session.".to_owned()
            }
            ReadError::Refused { reason } => {
                format!("Hermes refused {command}: {reason}.")
            }
            ReadError::TimedOut { secs } => {
                format!("Hermes did not answer within {secs} s. Try again.")
            }
            ReadError::NotConnected => {
                "Talaria is not connected to Hermes. It reconnects on its own; try again when \
                 the status line shows it is connected."
                    .to_owned()
            }
            ReadError::Unreadable => "Hermes's answer could not be read.".to_owned(),
            ReadError::NotSent => {
                format!("Talaria could not send {method}. This is a Talaria bug; please report it.")
            }
        };
        format!("  {text}")
    }

    /// The short text used inside notes.
    pub fn short(&self) -> String {
        match self {
            ReadError::MethodMissing => "not served by this gateway".to_owned(),
            ReadError::NotFound => "session not loaded".to_owned(),
            ReadError::PermissionDenied => "permission denied".to_owned(),
            ReadError::Refused { reason } => format!("refused ({reason})"),
            ReadError::TimedOut { secs } => format!("no answer within {secs} s"),
            ReadError::NotConnected => "not connected".to_owned(),
            ReadError::Unreadable => "answer unreadable".to_owned(),
            ReadError::NotSent => "not sent".to_owned(),
        }
    }
}

/// The glyph and the note for a context file's status.
pub fn file_status(status: &FileStatus) -> (&'static str, Option<String>) {
    match status {
        FileStatus::Loaded => ("✓", None),
        FileStatus::Truncated => (
            "◐",
            Some("truncated — over context_file_max_chars".to_owned()),
        ),
        FileStatus::Flagged => (
            "⚠",
            Some("loaded — matched prompt-injection pattern(s); review the file".to_owned()),
        ),
        FileStatus::Shadowed => (
            "○",
            Some("not loaded — higher-priority context type wins".to_owned()),
        ),
        FileStatus::Blocked => (
            "✗",
            Some("not loaded — blocked by the prompt-injection scan".to_owned()),
        ),
        FileStatus::Empty => ("○", Some("not loaded — empty file".to_owned())),
        FileStatus::Unreadable => ("✗", Some("not loaded — could not be read".to_owned())),
        FileStatus::Suppressed => (
            "○",
            Some("not loaded — cwd fell back to the Hermes install tree".to_owned()),
        ),
        FileStatus::Other(raw) => ("?", Some(format!("status {raw}"))),
    }
}

/// A compaction is already running for this session.
pub const T_C8: &str = "A compaction is already running for this session.";

/// The session's revision is not known yet.
pub const T_C9: &str =
    "Talaria has not learned this session's revision yet; try again in a moment.";

/// The compaction call's answer never came.
pub const T_C13: &str = "No answer from Hermes within 11 minutes, or the connection dropped. The compaction may still finish in Hermes. Run the same /compact line again: if it has finished, Hermes returns its result without compacting twice; if it is still running, Talaria says so.";

/// The compaction call could not be sent.
pub fn t_c14(reason: &str) -> String {
    format!("Talaria could not send the compaction ({reason}). Nothing was changed.")
}
