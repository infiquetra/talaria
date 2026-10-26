//! How a launch runs git. Step 0 reports that the program was not found.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::Duration;

/// Captured output of one git invocation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GitOutput {
    /// The process status, or `-1` when git did not exit.
    pub status: i32,
    /// Standard output, lossy-decoded.
    pub stdout: String,
    /// Standard error, lossy-decoded.
    pub stderr: String,
}

/// Why a git invocation produced no output.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum GitError {
    /// The program could not be started.
    NotFound,
    /// The timeout elapsed and the child was killed.
    TimedOut,
    /// The launch was cancelled and the child was killed.
    Cancelled,
    /// The child could not be started, or its output could not be read.
    Io(String),
}

/// The git program a launch uses, plus the environment added to every call.
#[derive(Clone, Debug)]
pub struct GitRunner {
    program: PathBuf,
}

impl GitRunner {
    /// Run `git` from `PATH`.
    #[must_use]
    pub fn from_env() -> Self {
        Self {
            program: PathBuf::from("git"),
        }
    }

    /// Run this program instead of `git`. Tests pass a wrapper script.
    #[must_use]
    pub fn with_program(program: PathBuf) -> Self {
        Self { program }
    }

    /// A call that may write. Step 0 does not start the program.
    pub async fn run(
        &self,
        cwd: &Path,
        args: &[&str],
        timeout: Duration,
    ) -> Result<GitOutput, GitError> {
        let _ = (cwd, args, timeout, &self.program);
        Err(GitError::NotFound)
    }

    /// A read-only call. Step 0 does not start the program.
    pub async fn run_read_only(
        &self,
        cwd: &Path,
        args: &[&str],
        timeout: Duration,
    ) -> Result<GitOutput, GitError> {
        let _ = (cwd, args, timeout, &self.program);
        Err(GitError::NotFound)
    }
}

/// The environment of one git call. `read_only` adds `GIT_OPTIONAL_LOCKS=0`.
#[must_use]
pub fn git_env(base: &BTreeMap<String, String>, read_only: bool) -> BTreeMap<String, String> {
    let _ = (base, read_only);
    todo!("git env")
}

/// The `-c core.sshCommand=...` prefix, or nothing when ssh is already configured.
#[must_use]
pub fn git_prefix_args(user_ssh_command_configured: bool) -> Vec<String> {
    let _ = user_ssh_command_configured;
    todo!("git prefix")
}
