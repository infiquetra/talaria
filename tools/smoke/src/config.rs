//! Options, preflight checks and launch lines.

use std::path::{Path, PathBuf};
use std::time::Duration;

/// Everything the run needs, resolved by the preflight.
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

/// Everything the phase 2a run needs beyond [`SmokeConfig`].
pub struct Phase2aConfig {
    /// The tmux executable the tmux section drives.
    pub tmux: PathBuf,
}

/// `KEY='value' '<path>' args...; echo SMOKE_EXIT:$?`
pub fn launch_line_env(_env: &[(&str, &str)], _talaria: &Path, _args: &[&str]) -> String {
    unimplemented!("not implemented")
}

/// `'<path>' args...; echo SMOKE_EXIT:$?`
pub fn launch_line(talaria: &Path, args: &[&str]) -> String {
    let mut line = format!("'{}'", talaria.display());
    for argument in args {
        line.push(' ');
        line.push_str(argument);
    }
    line.push_str("; echo SMOKE_EXIT:$?");
    line
}
