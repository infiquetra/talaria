//! Entry point for the configuration screens suite.

use crate::{Clock, SmokeConfig, SmokeReport, Terminal};
use std::{path::PathBuf, sync::atomic::AtomicBool};

/// Location for the empty home used by the no-gateway check.
pub struct Phase4aConfig {
    pub empty_home_parent: PathBuf,
}

pub fn run_smoke_4a(
    _term: &dyn Terminal,
    _clock: &dyn Clock,
    _cfg: &SmokeConfig,
    _cfg4a: &Phase4aConfig,
    _stop: &AtomicBool,
) -> SmokeReport {
    panic!("not implemented")
}
