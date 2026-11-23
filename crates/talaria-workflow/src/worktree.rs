//! Per-call Git worktrees retained until their owner explicitly removes them.

mod git;
mod provenance;

pub use git::{
    CleanupAction, CleanupEntry, GitWorktrees, IgnoredEntry, IgnoredKind, SAVED_SUFFIX,
    WorktreeInfo, WorktreeReport, save_uncommitted,
};

use crate::{engine::AgentRequest, hermes::WorktreeProvider};
use std::path::{Path, PathBuf};

impl WorktreeProvider for GitWorktrees {
    fn worktree_for(&self, request: &AgentRequest) -> Result<PathBuf, String> {
        self.for_call(&request.call_id.digest, request.call_id.occurrence)
    }
}

/// The branch an isolated call's saved work goes to: its worktree branch with `-saved`.
pub fn saved_branch_name(run_id: &str, digest: &str, occurrence: u32) -> String {
    format!("talaria/wf/{run_id}/{digest}-{occurrence}{SAVED_SUFFIX}")
}

/// Save the uncommitted work in `folder`, an agent worktree of the run in `run_dir`, to a new
/// branch (see [`save_uncommitted`]). Only a folder inside the run's own `worktrees` folder is
/// touched: any other folder is refused without running git in it.
pub fn save_agent_worktree(
    run_dir: &Path,
    folder: &Path,
    branch: &str,
    message: &str,
) -> Result<Option<String>, String> {
    let root = run_dir
        .join("worktrees")
        .canonicalize()
        .map_err(|e| format!("the run has no worktrees folder: {e}"))?;
    let real = folder
        .canonicalize()
        .map_err(|e| format!("the worktree folder is gone: {e}"))?;
    if real == root || !real.starts_with(&root) {
        return Err(format!(
            "{} is not one of this run's worktrees; nothing was saved",
            folder.display()
        ));
    }
    save_uncommitted(&real, branch, message)
}

/// Added to a failed or stopped agent's reason when its uncommitted work was saved.
const SAVED_NOTE: &str = " (uncommitted work saved to branch ";

/// `reason` with the saved branch named at its end, so every view that shows the reason says
/// where the work went.
pub fn with_saved_branch(reason: &str, branch: &str) -> String {
    format!("{reason}{SAVED_NOTE}{branch})")
}

/// The saved branch a reason written by [`with_saved_branch`] names.
pub fn saved_branch_in(reason: &str) -> Option<String> {
    let (_, rest) = reason.rsplit_once(SAVED_NOTE)?;
    rest.strip_suffix(')')
        .filter(|name| !name.is_empty() && !name.contains(char::is_whitespace))
        .map(str::to_owned)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_saved_branch_reads_back_from_the_reason() {
        let reason = with_saved_branch(
            "stopped in Hermes (outcome 'interrupted')",
            "talaria/wf/r/c-0-saved",
        );
        assert_eq!(
            saved_branch_in(&reason).as_deref(),
            Some("talaria/wf/r/c-0-saved")
        );
        assert_eq!(
            saved_branch_in("stopped in Hermes (outcome 'interrupted')"),
            None
        );
    }
}
