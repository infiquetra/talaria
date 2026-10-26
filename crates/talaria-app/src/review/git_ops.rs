//! The git a review runs. Step 0 stubs. Issue #245.

use std::collections::BTreeMap;
use std::path::Path;

use talaria_core::review::{ChangedFile, GitAction, Listing, RecordView, ReviewTarget, Scope};

use crate::worktree::GitRunner;

/// Read a listing for a scope.
pub async fn list(git: &GitRunner, target: &ReviewTarget, scope: Scope) -> Result<Listing, String> {
    let _ = (git, target, scope);
    Err("stub".to_owned())
}

/// Read one file's diff.
pub async fn file_diff(
    git: &GitRunner,
    target: &ReviewTarget,
    scope: Scope,
    base: Option<&str>,
    file: &ChangedFile,
) -> Result<String, String> {
    let _ = (git, target, scope, base, file);
    Err("stub".to_owned())
}

/// Run one git action. The fail-closed push and pull-request guards live here.
pub async fn run_action(
    git: &GitRunner,
    target: &ReviewTarget,
    action: &GitAction,
) -> Result<String, String> {
    let _ = (git, target, action);
    Err("stub".to_owned())
}

/// The draft prompt's inputs: the diff text and recent subjects.
pub async fn commit_context(
    git: &GitRunner,
    target: &ReviewTarget,
) -> Result<(String, String), String> {
    let _ = (git, target);
    Err("stub".to_owned())
}

/// The environment of one review git call: the launch git environment plus
/// `GIT_PAGER=cat`, `PAGER=cat`, `GIT_EDITOR=true`.
pub fn review_env(base: &BTreeMap<String, String>) -> BTreeMap<String, String> {
    let _ = base;
    todo!()
}

/// The default branch of `origin`: `origin/HEAD`, else `ls-remote`, else the
/// record's base ref, else unknown (`None`).
pub async fn default_branch(
    git: &GitRunner,
    main_root: &Path,
    record: Option<&RecordView>,
) -> Option<String> {
    let _ = (git, main_root, record);
    todo!()
}
