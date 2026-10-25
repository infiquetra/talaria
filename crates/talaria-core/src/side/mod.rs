//! Shell commands and side questions: pure state and text.

pub mod classify;
pub mod reduce;
pub mod text;

pub use classify::{Submission, classify};
pub use reduce::btw_command;
pub use text::{
    clean_output, format_duration, shell_outcome_text, shell_refusal_text, side_outcome_text,
    side_refusal_text,
};

use std::collections::BTreeSet;

use crate::session::SessionState;

/// How long an accepted side question waits for its answer.
pub const SIDE_ANSWER_TIMEOUT_MS: u64 = 300_000;

/// One side request, allocated by the reducer from 1.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct RequestId(pub u64);

/// What a pending row waits for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PendingKind {
    Shell {
        command: String,
    },
    Side {
        question: String,
        task_id: Option<String>,
        accepted_ms: Option<u64>,
    },
}

/// One row of the side-tasks section.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PendingTask {
    pub request_id: RequestId,
    pub started_ms: u64,
    pub kind: PendingKind,
}

/// Pending side work and the ids already settled. A field of the session.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SideTasks {
    pub pending: Vec<PendingTask>,
    pub answered: BTreeSet<String>,
    pub unconfirmed: Vec<(RequestId, String)>,
    pub timed_out: BTreeSet<String>,
    pub last_request_id: u64,
}

/// How a shell command ended.
#[derive(Debug, Clone, PartialEq)]
pub enum ShellOutcome {
    Ok {
        stdout: String,
        stderr: String,
        code: i64,
    },
    Refused {
        reason: String,
        detail: Option<String>,
        rpc_code: i64,
    },
    Unknown,
    Unreadable,
    NotSent {
        not_connected: bool,
    },
}

/// How a side question ended.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SideOutcome {
    Answered {
        text: String,
    },
    Failed {
        text: String,
    },
    Refused {
        reason: String,
        detail: Option<String>,
    },
    NotSent {
        not_connected: bool,
    },
}

/// Which window asked the side question.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Origin {
    Here,
    Elsewhere,
}

/// One finished shell command.
#[derive(Debug, Clone, PartialEq)]
pub struct ShellCell {
    pub command: String,
    pub outcome: ShellOutcome,
    pub duration_ms: u64,
}

/// One finished side question.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SideCell {
    pub question: String,
    pub outcome: SideOutcome,
    pub origin: Origin,
    pub late: bool,
}

/// The app-clock time of the next whole-second row change, if any row is pending.
pub fn next_tick(tasks: &SideTasks, now_ms: u64) -> Option<u64> {
    let _ = (tasks, now_ms);
    todo!("#225")
}

/// Move the side rows and ids from a replaced state into its replacement.
pub fn carry_over(old: &mut SessionState, new: &mut SessionState) {
    let _ = (old, new);
    todo!("#225")
}
