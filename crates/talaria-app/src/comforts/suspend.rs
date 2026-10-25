//! Suspend to the shell and resume. Stubs. Behavior contract: issue 228.

use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::time::{Duration, Instant};

/// What a suspend attempt produced.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SuspendResult {
    /// The process stopped and resumed.
    Resumed,
    /// No stop arrived within the limit.
    NotStopped,
}

/// Whether a job-control shell can resume this process.
pub fn job_control_available() -> bool {
    todo!("#228")
}

/// Stop the process group, then wait for the resume. Blocking.
pub fn suspend_process(continued: &AtomicBool) -> SuspendResult {
    let _ = continued;
    todo!("#228")
}

/// Wait for the resume flag, reading the flag before the deadline each pass.
pub fn wait_for_continue(
    continued: &AtomicBool,
    start: Instant,
    now: &dyn Fn() -> Instant,
    limit: Duration,
) -> SuspendResult {
    let _ = (continued, start, now, limit);
    todo!("#228")
}

/// Watch for a resume this process did not expect. Never polls.
pub fn spawn_sigcont_watch(
    expected: Arc<AtomicBool>,
    released: Arc<AtomicBool>,
    continued: Arc<AtomicBool>,
    on_unexpected: impl Fn() + Send + 'static,
) -> std::io::Result<()> {
    let _ = (expected, released, continued, on_unexpected);
    todo!("#228")
}

/// Catch interrupt and quit once, so they end only a released child.
pub fn catch_interrupts_while_released() -> std::io::Result<()> {
    todo!("#228")
}
