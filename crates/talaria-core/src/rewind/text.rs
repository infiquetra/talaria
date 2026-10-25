//! Every user-visible rewind string. Issue 233. Checkpoint rewind and diff.

use super::checkpoints::NoMatch;

/// Which call failed or was refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CallKind {
    /// `session.resume`.
    ReadSession,
    /// `rollback.list`.
    ListCheckpoints,
    /// `rollback.diff`.
    CheckpointDiff,
    /// `rollback.session_diff`.
    SessionDiff,
    /// `session.mutate`.
    Mutate,
    /// `rollback.restore`.
    Restore,
}

/// Why a call did not return a decoded success.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CallFailure {
    /// The gateway refused.
    Refused {
        reason: String,
        field: Option<String>,
        detail: Option<String>,
        revision: Option<i64>,
        execution_generation: Option<i64>,
    },
    /// The reply was lost or timed out.
    Unknown,
    /// The reply did not decode. The gateway may have done the work.
    Unreadable,
    /// Nothing was sent.
    NotSent(String),
}

/// What a restore reply told us. `directory` is never stored.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RestoreOutcome {
    /// The restore reported success.
    pub success: bool,
    /// Short hash, when present.
    pub restored_to: Option<String>,
    /// Checkpoint message, when present.
    pub reason: Option<String>,
    /// The one restored path, when present.
    pub file: Option<String>,
    /// Paths a safe restore wrote.
    pub restored_files: Option<Vec<String>>,
    /// Paths left because the person edited them.
    pub skipped_user_edits: Option<Vec<String>>,
    /// Paths left because they are over the size limit.
    pub skipped_oversize: Option<Vec<String>>,
    /// Paths a delete could not remove.
    pub failed_deletes: Option<Vec<String>>,
    /// Rows Hermes removed after a whole-folder restore.
    pub history_removed: Option<i64>,
    /// Error text when success is false.
    pub error: Option<String>,
}

/// A session diff reply.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionDiffOutcome {
    /// Checkpoints are on.
    pub enabled: bool,
    /// The diff was computed.
    pub success: bool,
    /// Stat text.
    pub stat: String,
    /// Patch text, possibly cut.
    pub diff: String,
    /// Line count of the uncut diff.
    pub diff_lines: u64,
    /// No checkpoint yet, or no changes. The reply does not say which.
    pub empty: bool,
    /// Error text, when present.
    pub error: Option<String>,
}

/// A diff the overlay can draw.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DiffDoc {
    /// Checkpoints are off.
    Off,
    /// Nothing to show. The string says both possible causes.
    Empty(String),
    /// The diff could not be computed.
    Failed(String),
    /// A stat and a patch.
    Text {
        /// Title line.
        title: String,
        /// Second line, for the session diff.
        subtitle: Option<String>,
        /// Stat text.
        stat: String,
        /// Patch text.
        diff: String,
        /// Omit the patch body.
        stat_only: bool,
        /// Cut notice, when one is due.
        cut_note: Option<String>,
    },
}

/// Text for a refused `session.mutate`.
#[must_use]
pub fn mutate_refusal_text(reason: &str, field: Option<&str>, detail: Option<&str>) -> String {
    let _ = (reason, field, detail);
    todo!("Issue 233. Checkpoint rewind and diff.")
}

/// Text for a refused read, from the refusal table.
#[must_use]
pub fn read_refusal_text(call: CallKind, reason: &str, detail: Option<&str>) -> String {
    let _ = (call, reason, detail);
    todo!("Issue 233. Checkpoint rewind and diff.")
}

/// Text for a refused restore.
#[must_use]
pub fn restore_refusal_text(whole_folder: bool, reason: &str, detail: Option<&str>) -> String {
    let _ = (whole_folder, reason, detail);
    todo!("Issue 233. Checkpoint rewind and diff.")
}

/// Text for a failed call.
#[must_use]
pub fn failure_text(f: &CallFailure, call: CallKind) -> String {
    let _ = (f, call);
    todo!("Issue 233. Checkpoint rewind and diff.")
}

/// Lines that say what a restore did. `target_label` is the typed target or the row index.
#[must_use]
pub fn restore_summary(r: &RestoreOutcome, target_label: &str) -> Vec<String> {
    let _ = (r, target_label);
    todo!("Issue 233. Checkpoint rewind and diff.")
}

/// Age of a prompt relative to `now_ms`.
#[must_use]
pub fn relative_age(timestamp_s: f64, now_ms: u64) -> String {
    let _ = (timestamp_s, now_ms);
    todo!("Issue 233. Checkpoint rewind and diff.")
}

/// The dim line for a prompt with no checkpoint.
#[must_use]
pub fn no_match_text(m: &NoMatch) -> &'static str {
    let _ = m;
    todo!("Issue 233. Checkpoint rewind and diff.")
}

/// Line count using the same breaks as Python's `splitlines`.
#[must_use]
pub fn python_line_count(s: &str) -> usize {
    let _ = s;
    todo!("Issue 233. Checkpoint rewind and diff.")
}

/// The document for a session diff.
#[must_use]
pub fn session_diff_doc(r: &SessionDiffOutcome, stat_only: bool) -> DiffDoc {
    let _ = (r, stat_only);
    todo!("Issue 233. Checkpoint rewind and diff.")
}

/// The document for one checkpoint compared with the files now.
#[must_use]
pub fn checkpoint_diff_doc(short: &str, stat: &str, diff: &str) -> DiffDoc {
    let _ = (short, stat, diff);
    todo!("Issue 233. Checkpoint rewind and diff.")
}
