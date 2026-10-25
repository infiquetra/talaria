//! Typed `/rollback`, `/undo`, and `/diff`. Issue 233. Checkpoint rewind and diff.

/// One parsed `/rollback` argument.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RollbackCommand {
    /// List checkpoints.
    List,
    /// Diff one target.
    Diff(String),
    /// Restore a target, optionally one file.
    Restore {
        /// Number or hash, as typed.
        target: String,
        /// Relative path, inner spaces kept.
        file_path: Option<String>,
        /// True only when a flag is present and there is no file.
        all: bool,
    },
    /// The target is not a checkpoint number or hash.
    BadTarget(String),
    /// The argument does not match the grammar.
    Usage,
}

/// Parse the argument of `/rollback`.
#[must_use]
pub fn parse_rollback(arg: &str) -> RollbackCommand {
    let _ = arg;
    todo!("Issue 233. Checkpoint rewind and diff.")
}

/// A row number of 1 to 6 digits, or 4 to 64 hex characters that do not start with `-`.
#[must_use]
pub fn is_checkpoint_target(s: &str) -> bool {
    let _ = s;
    todo!("Issue 233. Checkpoint rewind and diff.")
}

/// Parse `/undo`'s count. `Err` is the usage text.
pub fn parse_undo(arg: &str) -> Result<usize, ()> {
    let _ = arg;
    todo!("Issue 233. Checkpoint rewind and diff.")
}

/// One parsed `/diff` argument.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiffCommand {
    /// The session diff. `stat_only` omits the patch body.
    Session { stat_only: bool },
    /// Git's view, which this command does not run.
    GitView,
}

/// Parse the argument of `/diff`.
#[must_use]
pub fn parse_diff(arg: &str) -> DiffCommand {
    let _ = arg;
    todo!("Issue 233. Checkpoint rewind and diff.")
}
