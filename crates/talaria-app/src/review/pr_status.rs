//! Pull-request status, polled sparingly. Step 0 stubs. Issue #245.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;

use talaria_core::directory::rows::RowKey;
use talaria_core::review::PrStatus;

use crate::review::gh::GhRunner;
use crate::worktree::GitRunner;

/// The pull-request status service. Step 0 keeps it empty.
#[derive(Debug)]
pub struct PrStatusService;

impl PrStatusService {
    /// Serve status with this clock and notice sink.
    pub fn new(
        gh: GhRunner,
        git: GitRunner,
        clock: Arc<dyn Fn() -> Instant + Send + Sync>,
        notify: impl Fn(PrStatusNotice) + Send + Sync + 'static,
    ) -> PrStatusService {
        let _ = (gh, git, clock, notify);
        todo!()
    }

    /// Start the refreshes that are due. The timer calls this once a second.
    pub fn tick(&self) {
        todo!()
    }

    /// Note that a surface wants status, or stopped wanting it.
    pub fn want(&self, demand: PrDemand) {
        let _ = demand;
        todo!()
    }

    /// Refresh one repository at once.
    pub fn refresh_now(&self, main_root: &Path) {
        let _ = main_root;
        todo!()
    }

    /// The known labels by row.
    pub fn labels(&self) -> BTreeMap<RowKey, PrStatus> {
        todo!()
    }

    /// The known status of one branch, and whether it is stale.
    pub fn for_branch(&self, main_root: &Path, branch: &str) -> Option<(PrStatus, bool)> {
        let _ = (main_root, branch);
        todo!()
    }
}

/// A surface that wants pull-request status.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PrDemand {
    /// The fleet view opened or closed.
    Fleet(bool),
    /// A review opened or closed. The main root it shows.
    Review(PathBuf, bool),
    /// The status line wants the foreground folder's label, or nothing.
    StatusLine(Option<PathBuf>),
}

/// The service has new labels.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PrStatusNotice {
    /// Labels changed.
    Updated,
}
