//! Runs review effects. One mutation slot per process. Step 0 stubs. Issue #245.

use std::sync::Arc;

use talaria_core::review::{ReviewEffect, ReviewTarget};
use tokio::sync::mpsc;

use crate::app::AppMessage;
use crate::review::gh::GhRunner;
use crate::review::pr_status::PrStatusService;
use crate::worktree::GitRunner;

/// Where a running effect sends its answer.
pub type AppSender = mpsc::UnboundedSender<AppMessage>;

/// The review service. Step 0 keeps it empty.
#[derive(Debug)]
pub struct ReviewService;

impl ReviewService {
    /// Serve reviews with these runners. Tests pass `GhRunner::with_binary`.
    pub fn new(git: GitRunner, gh: GhRunner, pr: Arc<PrStatusService>) -> ReviewService {
        let _ = (git, gh, pr);
        todo!()
    }

    /// Open a review. The effects read the first listing.
    pub fn open(&mut self, target: ReviewTarget, now: f64) -> Vec<ReviewEffect> {
        let _ = (target, now);
        todo!()
    }

    /// Run one effect. Answers go through `tx`.
    pub fn execute(&self, effect: ReviewEffect, tx: AppSender) {
        let _ = (effect, tx);
        todo!()
    }
}
