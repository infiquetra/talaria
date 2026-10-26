//! What a finished launch reports on the terminal and in the fleet list.

use std::path::PathBuf;

use crate::worktree::WorktreeRecord;

/// A launch that Hermes admitted.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Launched {
    /// The live session id.
    pub session_id: String,
    /// Falls back to [`Self::session_id`] when the snapshot had none.
    pub stored_session_id: String,
    /// The profile the session was created in.
    pub profile: String,
    /// The admission id.
    pub admission_id: String,
    /// The admission status word, copied as the gateway sent it.
    pub admission_status: String,
    /// The model the snapshot reported.
    pub model: Option<String>,
    /// The folder the session is running in.
    pub cwd: PathBuf,
    /// Set when this launch created a worktree.
    pub worktree: Option<WorktreeRecord>,
    /// Warnings that do not fail the launch.
    pub warnings: Vec<String>,
}

/// Why a launch did not start.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LaunchError {
    /// The sentence after `talaria:`.
    pub what: String,
    /// The sentence after `Next step:`.
    pub next_step: String,
}

/// The four results a launch can finish with. Usage errors never reach here.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LaunchOutcome {
    /// Hermes admitted the prompt.
    Launched(Launched),
    /// Nothing was admitted.
    NotStarted {
        /// Why.
        error: LaunchError,
        /// A session that was created and then closed, when one exists.
        closed_session: Option<String>,
        /// The worktree this launch made was removed.
        tree_removed: bool,
        /// The rollback guard left the tree, and why.
        tree_kept: Option<(PathBuf, String)>,
    },
    /// The submit was sent and the reply was lost or unreadable. The tree is kept.
    Unknown {
        /// The live id, when the create was confirmed.
        session_id: Option<String>,
        /// The stored id, when the create was confirmed.
        stored_session_id: Option<String>,
        /// The profile the launch targeted.
        profile: String,
        /// The tree, when this launch made one.
        worktree: Option<WorktreeRecord>,
    },
    /// Interrupted before the submit was sent.
    Interrupted {
        /// The worktree this launch made was removed.
        tree_removed: bool,
        /// The rollback guard left the tree, and why.
        tree_kept: Option<(PathBuf, String)>,
    },
}

/// The process exit code for a finished launch.
#[must_use]
pub fn exit_code(outcome: &LaunchOutcome) -> u8 {
    let _ = outcome;
    todo!("exit code")
}

/// `(stdout, stderr)` for a finished launch.
#[must_use]
pub fn render_text(outcome: &LaunchOutcome) -> (String, String) {
    let _ = outcome;
    todo!("render text")
}

/// One JSON line for a finished launch.
#[must_use]
pub fn render_json(outcome: &LaunchOutcome) -> String {
    let _ = outcome;
    todo!("render json")
}

/// The fleet-list notice for a finished launch.
#[must_use]
pub fn notice_text(outcome: &LaunchOutcome) -> String {
    let _ = outcome;
    todo!("notice text")
}
