//! Options, preflight checks and launch lines.

use std::path::{Path, PathBuf};
use std::time::Duration;

pub struct SmokeConfig {
    /// Absolute path of the binary under test.
    pub talaria: PathBuf,
    pub profile: String,
    pub model: String,
    pub provider: String,
    pub deadline: Duration,
    pub step_scale: f32,
    pub report_dir: Option<PathBuf>,
    /// The environment variable that turns a skip into a failure is set.
    pub require: bool,
    pub temp_root: PathBuf,
    /// Makes tokens and the run id repeatable in tests.
    pub seed: Option<u64>,
}

/// `'<path>' args...; echo SMOKE_EXIT:$?`
pub fn launch_line(talaria: &Path, args: &[&str]) -> String {
    let _ = (talaria, args);
    todo!("smoke")
}
