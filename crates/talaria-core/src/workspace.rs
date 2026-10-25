//! The session working folder and the `/cd` move for #237.
//!
//! The reducer and the target resolution are stubs: every behaviour panics
//! with `unimplemented!()` until the green implementation lands. The fenced
//! mutation types come from the shared helper.

use std::path::{Path, PathBuf};

use crate::mutation::{MutationLedger, MutationOutcome, MutationRequest};
use crate::session::NoticeKind;

/// One parsed `/cd` command line.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CdCommand {
    /// Show the session's folder.
    Show,
    /// Move the session to the named folder.
    Move(String),
}

/// Parses the text after `/cd`; empty text shows the folder.
pub fn parse_cd_command(args: &str) -> CdCommand {
    let _ = args;
    unimplemented!("not yet implemented")
}

/// Why a `/cd` target could not be resolved.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CdTargetError {
    /// A relative path with no known session folder.
    RelativeWithoutFolder,
    /// A `~name` path, which Talaria never expands.
    TildeUser,
    /// A `~` path with no known home folder.
    NoHome,
}

/// Resolves a `/cd` argument to an absolute normalised path.
pub fn resolve_cd_target(
    arg: &str,
    session_cwd: Option<&Path>,
    home: Option<&Path>,
) -> Result<PathBuf, CdTargetError> {
    let _ = (arg, session_cwd, home);
    unimplemented!("not yet implemented")
}

/// Shortens a leading home folder to `~` by whole components.
pub fn display_path(path: &Path, home: Option<&Path>) -> String {
    let _ = (path, home);
    unimplemented!("not yet implemented")
}

/// The foreground session's folder and branch as Talaria shows them.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SessionFolder {
    /// The session's working folder.
    pub cwd: Option<String>,
    /// The branch of the folder, when known.
    pub branch: Option<String>,
}

impl SessionFolder {
    /// Both fields as the snapshot gives them.
    pub fn from_snapshot(cwd: Option<&str>, branch: Option<&str>) -> SessionFolder {
        let _ = (cwd, branch);
        unimplemented!("not yet implemented")
    }

    /// Applies a `session.info` folder: a different folder clears the branch.
    pub fn apply_info_cwd(&mut self, cwd: &str) {
        let _ = cwd;
        unimplemented!("not yet implemented")
    }

    /// Applies a confirmed move; a missing branch clears it.
    pub fn apply_move(&mut self, cwd: &str, branch: Option<&str>) {
        let _ = (cwd, branch);
        unimplemented!("not yet implemented")
    }
}

/// What the local folder check found.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FolderCheck {
    /// A directory: the move may be sent.
    Directory,
    /// Nothing at the path.
    Missing,
    /// A file or other non-directory.
    NotADirectory,
    /// Any other error, or the check timed out.
    Unreadable,
}

/// Everything the `/cd` reducer needs from the application.
#[derive(Clone, Debug)]
pub struct CdContext<'a> {
    /// The foreground session, if any.
    pub session_id: Option<&'a str>,
    /// Whether a turn is running on the foreground session.
    pub turn_running: bool,
    /// The foreground session's folder.
    pub folder: &'a SessionFolder,
    /// The home folder, if known.
    pub home: Option<&'a Path>,
    /// The interrupt action's label, if bound.
    pub interrupt_label: Option<&'a str>,
}

/// One input to the `/cd` reducer.
#[derive(Clone, Debug, PartialEq)]
pub enum CdInput {
    /// A parsed `/cd` command line.
    Command(CdCommand),
    /// A finished local folder check.
    Checked {
        /// The session the check was for.
        session_id: String,
        /// The checked target.
        target: PathBuf,
        /// What the check found.
        check: FolderCheck,
    },
    /// The outcome of a sent move request.
    Outcome {
        /// The request the outcome answers.
        request: MutationRequest,
        /// The outcome.
        outcome: MutationOutcome,
    },
}

/// One effect the `/cd` reducer emits.
#[derive(Clone, Debug, PartialEq)]
pub enum CdEffect {
    /// Check the folder on the local machine.
    CheckFolder {
        /// The session the check is for.
        session_id: String,
        /// The target to check.
        target: PathBuf,
    },
    /// Send the move request.
    Send(MutationRequest),
    /// The move committed: show the new folder.
    UpdateFolder {
        /// The session that moved.
        session_id: String,
        /// The new folder.
        cwd: String,
        /// The branch of the new folder, when Hermes reports one.
        branch: Option<String>,
    },
    /// Show a notice to the developer.
    Notice {
        /// The notice kind.
        kind: NoticeKind,
        /// The notice text.
        text: String,
    },
}

/// The `/cd` reducer state: at most one check or move at a time.
#[allow(dead_code)]
#[derive(Clone, Debug)]
pub struct CdState {
    /// The session and target of the pending folder check, if any.
    checking: Option<(String, PathBuf)>,
    /// The move request in flight, if any.
    in_flight: Option<MutationRequest>,
}

impl Default for CdState {
    fn default() -> CdState {
        CdState::new()
    }
}

impl CdState {
    /// An idle state.
    pub fn new() -> CdState {
        CdState {
            checking: None,
            in_flight: None,
        }
    }

    /// Reduces one input into effects.
    pub fn update(
        &mut self,
        ctx: &CdContext<'_>,
        ledger: &mut MutationLedger,
        input: CdInput,
    ) -> Vec<CdEffect> {
        let _ = (ctx, ledger, &input);
        unimplemented!("not yet implemented")
    }
}
