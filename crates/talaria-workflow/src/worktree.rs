//! Per-call Git worktrees retained until their owner explicitly removes them.

mod git;
mod provenance;

pub use git::{
    CleanupAction, CleanupEntry, GitWorktrees, IgnoredEntry, IgnoredKind, SAVED_SUFFIX, SavedWork,
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
) -> Result<SavedWork, String> {
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

/// The saved branch named at the end of a failed call's reason, accepted only when it can be
/// this call's own: the call ran in its own worktree (`isolated`) and the name is its
/// [`saved_branch_name`] or a numbered variant of it (`-2`, `-3` and so on). A reason can end
/// with the agent's own reply text, so any other name there is only text.
pub fn own_saved_branch_in(
    reason: &str,
    run_id: &str,
    call_id: &crate::engine::CallId,
    isolated: bool,
) -> Option<String> {
    if !isolated {
        return None;
    }
    let named = saved_branch_in(reason)?;
    let own = saved_branch_name(run_id, &call_id.digest, call_id.occurrence);
    let numbered = named
        .strip_prefix(own.as_str())
        .and_then(|rest| rest.strip_prefix('-'))
        .is_some_and(|text| {
            text.parse::<u64>()
                .is_ok_and(|n| n >= 2 && n.to_string() == text)
        });
    (named == own || numbered).then_some(named)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_the_calls_own_saved_branch_reads_back_and_only_when_it_was_isolated() {
        let call = crate::engine::CallId {
            digest: "d".repeat(64),
            occurrence: 0,
        };
        let own = saved_branch_name("run1", &call.digest, 0);
        let read = |branch: &str, isolated: bool| {
            own_saved_branch_in(
                &with_saved_branch("agent failed: x", branch),
                "run1",
                &call,
                isolated,
            )
        };
        assert_eq!(read(&own, true).as_deref(), Some(own.as_str()));
        let second = format!("{own}-2");
        assert_eq!(read(&second, true).as_deref(), Some(second.as_str()));
        // An agent's reply that ends like a saved note names nothing.
        for fake in [
            "main".to_owned(),
            format!("{own}-x"),
            format!("{own}-1"),
            format!("{own}-02"),
            format!("{own}-2-3"),
            saved_branch_name("run1", &call.digest, 1),
            saved_branch_name("other", &call.digest, 0),
        ] {
            assert_eq!(read(&fake, true), None, "{fake}");
        }
        // A call without isolation is never saved, so even its own name is only text.
        assert_eq!(read(&own, false), None);
    }

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
