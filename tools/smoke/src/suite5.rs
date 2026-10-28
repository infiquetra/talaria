//! Workflow smoke suite entry point.
use crate::{Clock, SmokeConfig, SmokeReport, Terminal};
use std::sync::atomic::AtomicBool;

/// Expected profile, model, and provider for the workflow smoke suite.
pub struct Phase5Config {
    pub profile: String,
    pub model: String,
    pub provider: String,
}

impl Default for Phase5Config {
    fn default() -> Self {
        panic!("not implemented")
    }
}

/// Runs the workflow smoke suite.
pub fn run_smoke_5(
    _term: &dyn Terminal,
    _clock: &dyn Clock,
    _cfg: &SmokeConfig,
    _cfg5: &Phase5Config,
    _stop: &AtomicBool,
) -> SmokeReport {
    panic!("not implemented")
}
