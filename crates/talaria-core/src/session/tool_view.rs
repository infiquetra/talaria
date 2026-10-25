//! A pure view of one tool cell. Issue 221. The cell is not changed.

use super::{ToolCell, ToolProgress};
use crate::diff::DiffModel;

/// What the screen shows for one tool cell.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ToolView {
    /// Short name, such as `Shell` or a label.
    pub title: String,
    /// What the call acted on. Absent for a label title and for a quiet tool.
    pub subject: Option<String>,
    /// Running, succeeded, failed, unfinished or unknown.
    pub outcome: OutcomeView,
    /// How long the call took, in Hermes's wording.
    pub duration: Option<String>,
    /// Progress wording, when the cell has progress.
    pub progress: Option<String>,
    /// Cleaned output, when the cell has a result.
    pub output: Option<OutputView>,
    /// Parsed diff, when the cell has one.
    pub diff: Option<DiffModel>,
    /// Quiet tools draw no body.
    pub quiet: bool,
}

/// How the header states the outcome.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum OutcomeView {
    /// The call is still running.
    Running,
    /// Hermes did not mark the call as an error.
    Succeeded {
        /// Optional headline, such as `exit 1` for a tool that is not a shell.
        headline: Option<String>,
    },
    /// Hermes marked the call as an error.
    Failed {
        /// First line of the failure, possibly prefixed with the exit code.
        headline: String,
        /// The whole failure text when it is long or has more than one line.
        detail: Option<String>,
    },
    /// The turn ended while the call was still running.
    Unfinished,
    /// A tool row restored from history, with no outcome.
    Unknown,
}

/// Cleaned output of one call.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OutputView {
    /// Output after escape and carriage-return cleanup.
    pub text: String,
    /// Logical lines in `text`.
    pub lines: usize,
    /// The gateway cut the result at 32,768 characters.
    pub cut_by_hermes: bool,
    /// Integer exit code, when the result had one.
    pub exit_code: Option<i64>,
    /// The terminal tool's own truncation note.
    pub tool_note: Option<String>,
}

/// Derive the view. Issue 221. The cell is not read yet.
#[must_use]
pub fn tool_view(cell: &ToolCell) -> ToolView {
    let _ = cell;
    todo!()
}

/// Hermes's duration words. Negative and non-finite values are absent.
///
/// Issue 221. The number is not formatted yet.
#[must_use]
pub fn format_duration(seconds: f64) -> Option<String> {
    let _ = seconds;
    todo!()
}

/// Progress words from a payload. Issue 221. The payload is not read yet.
#[must_use]
pub fn format_progress(progress: &ToolProgress) -> Option<String> {
    let _ = progress;
    todo!()
}
