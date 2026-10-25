//! The phase 2a run: the twenty-one steps of issues #221 to #228 behind one function.
//!
//! Stage 0 stub: every public item panics `not implemented`.

use std::sync::atomic::AtomicBool;

use crate::{Clock, Paster, Phase2aConfig, SmokeConfig, SmokeReport, Terminal};

/// Runs the phase 2a suite: the shared first-suite steps, then one step per feature, then the
/// tmux section. The steps after the first failure are not attempted.
pub fn run_smoke_2a(
    _term: &dyn Terminal,
    _paster: &dyn Paster,
    _clock: &dyn Clock,
    _cfg: &SmokeConfig,
    _cfg2: &Phase2aConfig,
    _stop: &AtomicBool,
) -> SmokeReport {
    unimplemented!("not implemented")
}
