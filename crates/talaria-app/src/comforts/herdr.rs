//! Herdr pane state reporting through the Herdr command line.
//!
//! Stubs. Behavior contract: issue 228.

use std::time::Duration;

use talaria_core::attention::PaneReport;

/// Reports pane state to Herdr in the background, at most one process at a time.
#[derive(Debug)]
pub struct HerdrReporter {
    /// Placeholder until the build adds the reporter.
    _private: (),
}

impl HerdrReporter {
    /// Build a reporter when inside Herdr and reporting is enabled, else none.
    pub fn from_env(
        enabled: bool,
        env: &dyn Fn(&str) -> Option<String>,
        now_ms: fn() -> u64,
    ) -> Option<Self> {
        let _ = (enabled, env, now_ms);
        todo!("#228")
    }

    /// Report one state. Never blocks; the newest waiting state wins.
    pub fn report(&self, pane: &PaneReport, session: Option<(&str, &str)>) {
        let _ = (pane, session);
        todo!("#228")
    }

    /// Release the pane, waiting at most the timeout.
    pub fn release(&self, timeout: Duration) {
        let _ = timeout;
        todo!("#228")
    }

    /// Drain queued report-failure notices. A failing reporter queues exactly one.
    pub fn take_notices(&self) -> Vec<String> {
        todo!("#228")
    }
}
