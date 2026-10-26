//! The `gh` a review runs. Step 0 stubs. Issue #245.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::Duration;

use talaria_core::review::{GhReady, PrStatus};

use crate::worktree::{GitError, GitOutput};

/// The `gh` program a review uses.
#[derive(Clone, Debug)]
pub struct GhRunner {
    program: PathBuf,
}

impl GhRunner {
    /// Run `gh` from `PATH`. `TALARIA_GH_BIN` overrides it, which tests use.
    #[must_use]
    pub fn from_env() -> GhRunner {
        todo!()
    }

    /// Run this program instead of `gh`. Tests pass a stub script.
    #[must_use]
    pub fn with_binary(path: PathBuf) -> GhRunner {
        Self { program: path }
    }

    /// Run `gh` with standard input closed. Step 0 starts nothing.
    pub async fn run(
        &self,
        cwd: &Path,
        args: &[&str],
        timeout: Duration,
    ) -> Result<GitOutput, GitError> {
        let _ = (cwd, args, timeout, &self.program);
        Err(GitError::NotFound)
    }

    /// Whether `gh` can open pull requests here.
    pub async fn readiness(&self, cwd: &Path) -> GhReady {
        let _ = (cwd, &self.program);
        GhReady::NotFound
    }

    /// The pull-request statuses of `branches` in `repo`.
    pub async fn pr_statuses(
        &self,
        cwd: &Path,
        repo: &str,
        branches: &[String],
    ) -> Result<Vec<PrStatus>, String> {
        let _ = (cwd, repo, branches, &self.program);
        Err("stub".to_owned())
    }

    /// Open a draft pull request. The URL it printed.
    pub async fn create_draft(
        &self,
        cwd: &Path,
        base: &str,
        branch: &str,
    ) -> Result<String, String> {
        let _ = (cwd, base, branch, &self.program);
        Err("stub".to_owned())
    }

    /// Open a pull request in the browser.
    pub async fn open_in_browser(&self, cwd: &Path, number: u64) -> Result<(), String> {
        let _ = (cwd, number, &self.program);
        Err("stub".to_owned())
    }
}

/// The environment of one `gh` call: the process environment plus exactly the
/// five fixed keys.
pub fn gh_env(base: &BTreeMap<String, String>) -> BTreeMap<String, String> {
    let _ = base;
    todo!()
}
