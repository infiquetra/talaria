//! Running one headless turn against the gateway.

use std::path::Path;
use std::time::{Duration, Instant};

use tokio::sync::mpsc::Receiver;

use crate::headless::PrintOptions;
use crate::headless::output::HeadlessResult;
use crate::headless::run::StopCause;
use crate::launch::Launch;

/// Time limits for one run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HeadlessTimings {
    pub receipt: Duration,
    pub cancel: Duration,
    pub interrupt: Duration,
    pub settle: Duration,
    pub detach: Duration,
    pub stop_budget: Duration,
}

impl Default for HeadlessTimings {
    fn default() -> Self {
        todo!("#235")
    }
}

impl HeadlessTimings {
    /// Every limit of the default, divided by one hundred, for fast tests.
    pub fn scaled_for_tests() -> Self {
        todo!("#235")
    }
}

/// Open the session, run one finite turn, detach, and report what happened.
pub async fn run_headless(
    launch: &Launch,
    options: &PrintOptions,
    text: String,
    home: &Path,
    started: Instant,
    stops: Receiver<StopCause>,
    timings: HeadlessTimings,
) -> HeadlessResult {
    let _ = (launch, options, text, home, started, stops, timings);
    todo!("#235")
}
