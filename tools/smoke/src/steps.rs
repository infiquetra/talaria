//! The six steps, the run and the tab guard.

use std::sync::atomic::AtomicBool;

use crate::clock::Clock;
use crate::config::SmokeConfig;
use crate::report::SmokeReport;
use crate::terminal::{TabId, Terminal};

pub fn run_smoke(
    term: &dyn Terminal,
    clock: &dyn Clock,
    cfg: &SmokeConfig,
    stop: &AtomicBool,
) -> SmokeReport {
    let _ = (term, clock, cfg, stop);
    todo!("smoke")
}

/// Owns the tab id; `Drop` is only a fallback.
pub struct TabGuard<'a> {
    _term: &'a dyn Terminal,
    _tab: TabId,
}

impl TabGuard<'_> {
    pub fn close(self) -> Result<(), CleanupError> {
        todo!("smoke")
    }
}

#[derive(Debug, thiserror::Error)]
#[error("tab {} was not closed", tab.0)]
pub struct CleanupError {
    pub tab: TabId,
    pub detail: String,
}
