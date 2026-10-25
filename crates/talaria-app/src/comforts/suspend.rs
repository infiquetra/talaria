//! Suspend to the shell and resume. Behavior contract: issue 228.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
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
    let group = rustix::process::getpgrp();
    rustix::termios::tcgetpgrp(std::io::stdin()).is_ok_and(|foreground| foreground == group)
        && rustix::process::getsid(None).is_ok_and(|session| session != group)
}

/// Stop the process group, then wait for the resume. Blocking.
pub fn suspend_process(continued: &AtomicBool) -> SuspendResult {
    if rustix::process::kill_current_process_group(rustix::process::Signal::TSTP).is_err() {
        return SuspendResult::NotStopped;
    }
    let start = Instant::now();
    wait_for_continue(continued, start, &Instant::now, Duration::from_secs(1))
}

/// Wait for the resume flag, reading the flag before the deadline each pass.
pub fn wait_for_continue(
    continued: &AtomicBool,
    start: Instant,
    now: &dyn Fn() -> Instant,
    limit: Duration,
) -> SuspendResult {
    loop {
        if continued.load(Ordering::SeqCst) {
            return SuspendResult::Resumed;
        }
        if now().saturating_duration_since(start) >= limit {
            return SuspendResult::NotStopped;
        }
        std::thread::sleep(Duration::from_millis(1));
    }
}

/// Watch for a resume this process did not expect. Never polls.
pub fn spawn_sigcont_watch(
    expected: Arc<AtomicBool>,
    released: Arc<AtomicBool>,
    continued: Arc<AtomicBool>,
    on_unexpected: impl Fn() + Send + 'static,
) -> std::io::Result<()> {
    use signal_hook::{consts::SIGCONT, iterator::Signals};
    signal_hook::flag::register(SIGCONT, continued.clone())?;
    let mut signals = Signals::new([SIGCONT])?;
    std::thread::Builder::new()
        .name("talaria-continue".to_owned())
        .spawn(move || {
            for _ in signals.forever() {
                if !expected.load(Ordering::SeqCst) && !released.load(Ordering::SeqCst) {
                    continued.store(false, Ordering::SeqCst);
                    on_unexpected();
                }
            }
        })?;
    Ok(())
}

/// Catch interrupt and quit once, so they end only a released child.
pub fn catch_interrupts_while_released() -> std::io::Result<()> {
    static REGISTERED: std::sync::OnceLock<Result<(), std::io::ErrorKind>> =
        std::sync::OnceLock::new();
    REGISTERED
        .get_or_init(|| {
            for signal in [signal_hook::consts::SIGINT, signal_hook::consts::SIGQUIT] {
                signal_hook::flag::register(signal, Arc::new(AtomicBool::new(false)))
                    .map_err(|error| error.kind())?;
            }
            Ok(())
        })
        .map_err(std::io::Error::from)
}
