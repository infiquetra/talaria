//! Running the external command and deciding when to run it.

use std::path::PathBuf;
use std::time::{Duration, Instant};

use super::document::StatusDocument;
use super::output::ParsedOutput;
use super::segments::ExternalSegment;

/// The shortest gap between two runs when the document changed.
pub const MIN_RERUN_GAP: Duration = Duration::ZERO;

/// Split with POSIX quoting, never through a shell.
pub fn parse_command(command: &str) -> Result<Vec<String>, String> {
    let _ = command;
    todo!("status")
}

/// Clamp to 1 through 3600 seconds, with no notice.
pub fn interval_from_config(seconds: u64) -> Duration {
    let _ = seconds;
    todo!("status")
}

#[derive(Clone, Debug)]
pub struct CommandLimits {
    pub timeout: Duration,
    pub stdout_bytes: usize,
    pub stderr_bytes: usize,
    pub rows: usize,
}

impl Default for CommandLimits {
    fn default() -> Self {
        todo!("status")
    }
}

#[derive(Clone, Debug)]
pub struct CommandSpec {
    pub argv: Vec<String>,
    pub launch_dir: PathBuf,
    pub env_allowlist: Vec<String>,
}

#[derive(Clone, Debug)]
pub struct CommandResult {
    pub parsed: ParsedOutput,
}

/// Stateless: a failure gives its marker alone.
pub fn external_segment(result: &CommandResult) -> Option<ExternalSegment> {
    let _ = result;
    todo!("status")
}

/// Keeps the last good text beside a failure marker.
#[derive(Default)]
pub struct ExternalDisplay {
    shown: Option<ExternalSegment>,
}

impl ExternalDisplay {
    pub fn apply(&mut self, result: &CommandResult) {
        let _ = result;
        todo!("status")
    }

    pub fn segment(&self) -> Option<&ExternalSegment> {
        let _ = &self.shown;
        todo!("status")
    }
}

/// Run one command under the process contract and parse its output.
pub async fn run_command(
    spec: &CommandSpec,
    env: Vec<(String, String)>,
    stdin: Vec<u8>,
    limits: &CommandLimits,
) -> CommandResult {
    let _ = (spec, env, stdin, limits);
    todo!("status")
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SkipReason {
    Running,
    Unchanged,
    TooSoon,
    Stopped,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Decision {
    Run,
    Skip(SkipReason),
}

/// A pure function of time and the document.
pub struct RefreshPolicy {
    _interval: Duration,
}

impl RefreshPolicy {
    pub fn new(interval: Duration) -> Self {
        Self {
            _interval: interval,
        }
    }

    pub fn decide(&mut self, now: Instant, document: &[u8], running: bool) -> Decision {
        let _ = (now, document, running);
        todo!("status")
    }

    pub fn next_wake(&self, now: Instant) -> Option<Instant> {
        let _ = now;
        todo!("status")
    }
}

/// Owns the policy, the running command and its process group.
pub struct StatusCommandService {
    _private: (),
}

impl StatusCommandService {
    pub fn new(
        spec: CommandSpec,
        interval: Duration,
        parent_env: Vec<(String, String)>,
        is_credential_name: fn(&str) -> bool,
        limits: CommandLimits,
    ) -> Self {
        let _ = (spec, interval, parent_env, is_credential_name, limits);
        todo!("status")
    }

    /// Must be called inside a Tokio runtime.
    pub fn on_tick(
        &mut self,
        now: Instant,
        document: &StatusDocument,
        done: impl FnOnce(CommandResult) + Send + 'static,
    ) -> Decision {
        let _ = (now, document, done);
        todo!("status")
    }

    /// `None` while a command runs.
    pub fn next_wake(&self, now: Instant) -> Option<Instant> {
        let _ = now;
        todo!("status")
    }

    /// Kill a command in flight and return when it is dead.
    pub async fn shutdown(&mut self) {
        todo!("status")
    }
}
