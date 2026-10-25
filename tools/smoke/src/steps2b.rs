//! The phase 2b run: fifteen steps over the eight units of issues #230 to #237.
//!
//! Stage 0 stub: every public item panics `not implemented`.

use std::sync::atomic::AtomicBool;

use crate::{Clock, SmokeConfig, SmokeReport, Terminal};

/// Runs the phase 2b suite: startup, one step per feature, then the headless section. The
/// steps after the first failure are not attempted.
pub fn run_smoke_2b(
    _term: &dyn Terminal,
    _clock: &dyn Clock,
    _cfg: &SmokeConfig,
    _stop: &AtomicBool,
) -> SmokeReport {
    todo!("not implemented")
}
