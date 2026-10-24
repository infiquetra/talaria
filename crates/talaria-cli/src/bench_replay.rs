//! The replay entry point the performance harness drives.

use std::path::PathBuf;
use std::process::ExitCode;

use hermes_client::replay_source::Speed;

#[allow(dead_code)]
pub struct BenchReplayArgs {
    pub fixture: PathBuf,
    pub speed: Speed,
    pub report: Option<PathBuf>,
}

#[derive(Debug)]
#[allow(dead_code)]
pub enum ArgError {
    MissingFixture,
    BadSpeed(String),
    MissingValue(String),
}

/// `Ok(None)` when the flag is absent.
pub fn parse_args(args: &[String]) -> Result<Option<BenchReplayArgs>, ArgError> {
    let _ = args;
    todo!("bench")
}

pub fn run(args: BenchReplayArgs) -> ExitCode {
    let _ = args;
    todo!("bench")
}
