//! The launch service. Step 0 accepts work and sends nothing.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Duration;

use crate::bg::conn::LaunchConn;
use crate::bg::machine::{CwdCheck, LaunchIds};
use crate::bg::outcome::{LaunchError, LaunchOutcome};
use crate::bg::request::LaunchRequest;
use crate::fleet::FleetHandle;
use crate::worktree::GitRunner;

/// A token shared by a launch and the code that can cancel it.
///
/// This is an `Arc<AtomicBool>`, not a utility-crate token. The workspace does not
/// depend on that utility crate, and no merged unit exported a token of its own.
#[derive(Clone, Debug)]
pub struct CancelToken {
    flag: Arc<AtomicBool>,
}

impl CancelToken {
    /// A token that has not been cancelled.
    #[must_use]
    pub fn new() -> Self {
        Self {
            flag: Arc::new(AtomicBool::new(false)),
        }
    }

    /// Cancel every launch holding a clone.
    pub fn cancel(&self) {
        self.flag.store(true, Ordering::SeqCst);
    }

    /// Whether [`Self::cancel`] has been called.
    #[must_use]
    pub fn is_cancelled(&self) -> bool {
        self.flag.load(Ordering::SeqCst)
    }
}

impl Default for CancelToken {
    fn default() -> Self {
        Self::new()
    }
}

/// Run one launch to a finish. Step 0 sends no frame and does not touch git.
pub async fn run_launch(
    conn: &LaunchConn,
    git: &GitRunner,
    request: LaunchRequest,
    ids: LaunchIds,
    fleet: Option<&FleetHandle>,
    cancel: CancelToken,
) -> LaunchOutcome {
    let _ = (conn, git, request, ids, fleet, cancel);
    LaunchOutcome::NotStarted {
        error: LaunchError {
            what: "stub".to_owned(),
            next_step: String::new(),
        },
        closed_session: None,
        tree_removed: false,
        tree_kept: None,
    }
}

/// Compare the folder that was sent with the folder the snapshot reported.
///
/// `None` or `""` is [`CwdCheck::Missing`]. A path that cannot be canonicalised is
/// [`CwdCheck::Differs`]. Equal canonical paths are [`CwdCheck::Matches`].
#[must_use]
pub fn compare_cwd(sent: &Path, reported: Option<&str>) -> CwdCheck {
    let _ = (sent, reported);
    todo!("compare cwd")
}

/// What the fleet list should show while a launch is in flight, and when it finishes.
#[derive(Clone, Debug, PartialEq, Eq)]
// A finished launch carries the full outcome. Tests match the field by name, so it stays unboxed.
#[allow(clippy::large_enum_variant)]
pub enum LaunchNotice {
    /// A stage changed.
    Progress { launch: u64, stage: Stage },
    /// The launch finished.
    Finished { launch: u64, outcome: LaunchOutcome },
}

/// The stage a progress notice names.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Stage {
    /// Opening the link.
    Connecting,
    /// Fetching the default branch.
    Fetching,
    /// Creating the worktree.
    CreatingWorktree,
    /// Creating the session.
    CreatingSession,
    /// Sending the prompt.
    Submitting,
}

/// Three launches of this process are already starting.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Busy;

/// Launches started from one Talaria process.
pub struct LaunchService {
    home: PathBuf,
    git: GitRunner,
    notify: Box<dyn Fn(LaunchNotice) + Send + Sync>,
    next: AtomicU64,
}

impl LaunchService {
    /// A service that notifies `notify` as launches progress. Step 0 never calls it.
    #[must_use]
    pub fn new(
        home: PathBuf,
        git: GitRunner,
        notify: impl Fn(LaunchNotice) + Send + Sync + 'static,
    ) -> Self {
        Self {
            home,
            git,
            notify: Box::new(notify),
            next: AtomicU64::new(1),
        }
    }

    /// Start a launch and return at once. Step 0 always accepts and does not notify.
    pub fn launch(
        &self,
        request: LaunchRequest,
        foreground_profile: String,
        fleet: Option<FleetHandle>,
    ) -> Result<u64, Busy> {
        let _ = (&self.home, &self.git, &self.notify);
        let _ = (request, foreground_profile, fleet);
        Ok(self.next.fetch_add(1, Ordering::Relaxed))
    }

    /// Cancel launches still starting and wait up to `limit`. Step 0 has none.
    pub async fn shutdown(&self, limit: Duration) -> Vec<u64> {
        let _ = (&self.home, limit);
        Vec::new()
    }
}
