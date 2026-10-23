//! Record the seven live fixtures, replay one, and scan a directory for secrets.
//!
//! Red-run stubs for P1-04: every body is `todo!("P1-04")`.

use std::path::{Path, PathBuf};

use hermes_client::{Client, InboundStream, ReplaySource};

/// The seven scenarios of the fixture set, in the order the script records them.
pub const SCENARIOS: [&str; 7] = [
    "plain-answer",
    "tool-diff",
    "approval",
    "clarification",
    "interrupted",
    "subagent-batch",
    "queued-prompt",
];

/// A scenario: its file name stem, prompts and what a good recording shows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Scenario {
    pub name: String,
    pub prompts: Vec<String>,
    pub expected_statuses: Vec<String>,
}

impl Scenario {
    /// The scenario named `name`, or `None` when it is not one of [`SCENARIOS`].
    pub fn named(name: &str) -> Option<Scenario> {
        let _ = name;
        todo!("P1-04")
    }
}

/// Why the script stops with exit code 2. The message is plain text for the operator.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{message}")]
pub struct PreconditionError {
    pub message: String,
}

/// Everything `preflight` needs, injected so tests control it.
#[derive(Debug, Clone)]
pub struct PreflightInput {
    /// The Hermes home (`--home`), where discovery looks for the gateway.
    pub hermes_home: PathBuf,
    /// `--profile`.
    pub profile: String,
    pub login_name: String,
    pub host_name: String,
}

/// What a passing `preflight` learned.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Preflight {
    pub profile_id: String,
    pub api_origin: String,
    pub code_sha: String,
    pub other_profiles: Vec<String>,
}

/// Discovery facts the substitution list is built from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiscoveryFacts {
    pub profile_id: String,
    pub api_origin: String,
}

/// Facts about the machine the script runs on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EnvFacts {
    pub home: String,
    pub login_name: String,
}

/// The settings a driver run needs.
#[derive(Debug, Clone)]
pub struct DriverConfig {
    pub model: String,
    pub provider: String,
    pub scratch: PathBuf,
    pub attempt_timeout: std::time::Duration,
}

/// How one attempt at a scenario ended.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AttemptResult {
    Ok,
    Failed(String),
    PreconditionAbsent(String),
}

/// The scratch project directory: `notes.txt` and `.pytest_cache`. Removed on drop.
#[derive(Debug)]
pub struct ScratchDir {
    path: PathBuf,
}

impl ScratchDir {
    pub fn create() -> std::io::Result<ScratchDir> {
        todo!("P1-04")
    }

    pub fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for ScratchDir {
    fn drop(&mut self) {}
}

/// Check the preconditions without starting anything. Runs discovery with
/// `EnsurePolicy::Never`, so no `hermes` process is ever run.
pub async fn preflight(input: &PreflightInput) -> Result<Preflight, PreconditionError> {
    let _ = input;
    todo!("P1-04")
}

/// The required evidence and final statuses of section 8.4 over a recorded file.
pub fn check_evidence(scenario: &Scenario, source: &ReplaySource) -> Result<(), String> {
    let _ = (scenario, source);
    todo!("P1-04")
}

/// The `(needle, replacement)` pairs of section 4.7.
pub fn build_substitutions(
    facts: &DiscoveryFacts,
    env: &EnvFacts,
    scratch: &Path,
) -> Vec<(String, String)> {
    let _ = (facts, env, scratch);
    todo!("P1-04")
}

/// Drive one attempt over a connected client and its inbound stream (the events come
/// only from the stream `Client::start` returned). Never writes the file itself.
pub async fn run_scenario(
    client: &Client,
    inbound: &mut InboundStream,
    scenario: &Scenario,
    config: &DriverConfig,
) -> AttemptResult {
    let _ = (client, inbound, scenario, config);
    todo!("P1-04")
}
