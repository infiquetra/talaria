use crate::{Clock, SmokeConfig, SmokeReport, Terminal};
use std::{path::PathBuf, sync::atomic::AtomicBool};

pub struct Phase4bConfig {
    pub empty_home_parent: PathBuf,
}

pub fn run_smoke_4b(
    _term: &dyn Terminal,
    _clock: &dyn Clock,
    _cfg: &SmokeConfig,
    _cfg4b: &Phase4bConfig,
    _stop: &AtomicBool,
) -> SmokeReport {
    panic!("not implemented")
}
