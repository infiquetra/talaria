//! Checkpoint lists and the prompt-to-checkpoint match. Issue 233.

use super::turns::RewindTurn;

/// One checkpoint row, newest first.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Checkpoint {
    /// 1-based index in list order. 1 is the newest.
    pub index: usize,
    /// Full hash.
    pub hash: String,
    /// First 8 characters, or the whole hash when it is shorter.
    pub short: String,
    /// Checkpoint message.
    pub message: String,
    /// Timestamp text as the gateway sent it.
    pub raw_time: String,
    /// Unix seconds, when `raw_time` parses.
    pub time: Option<i64>,
}

/// The answer of a checkpoint list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CheckpointList {
    /// Checkpoints are off.
    Off,
    /// Checkpoints are on and none exist.
    Empty,
    /// Rows, newest first.
    Rows(Vec<Checkpoint>),
    /// The list call failed.
    Unavailable(String),
}

impl CheckpointList {
    /// Build a list from the enabled flag and `(hash, timestamp, message)` rows.
    #[must_use]
    pub fn from_result(enabled: bool, rows: Vec<(String, String, String)>) -> Self {
        let _ = (enabled, rows);
        todo!("Issue 233. Checkpoint rewind and diff.")
    }
}

/// Unix seconds for a git ISO-8601 author time, or `None`.
#[must_use]
pub fn parse_checkpoint_time(s: &str) -> Option<i64> {
    let _ = s;
    todo!("Issue 233. Checkpoint rewind and diff.")
}

/// `HH:MM` from a time that parses, otherwise empty.
#[must_use]
pub fn clock_text(s: &str) -> String {
    let _ = s;
    todo!("Issue 233. Checkpoint rewind and diff.")
}

/// The checkpoint a confirm screen would restore.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CheckpointMatch {
    /// Index in the list.
    pub index: usize,
    /// No kept checkpoint is older than the prompt.
    pub oldest_kept: bool,
}

/// Why a prompt has no checkpoint.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NoMatch {
    /// Checkpoints are off.
    Off,
    /// The list could not be read.
    Unknown,
    /// The prompt has no time.
    NoTime,
    /// Nothing at or after the prompt.
    NoChanges,
    /// The turn edited files and no checkpoint falls inside its window.
    NoCheckpointForEdits,
}

/// The oldest checkpoint at or after the prompt's window start.
pub fn checkpoint_for_turn(
    turn: &RewindTurn,
    list: &CheckpointList,
) -> Result<CheckpointMatch, NoMatch> {
    let _ = (turn, list);
    todo!("Issue 233. Checkpoint rewind and diff.")
}
