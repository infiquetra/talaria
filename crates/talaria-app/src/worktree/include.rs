//! `.worktreeinclude` copies and links. Step 0 does not read the file.

use std::path::Path;
use std::time::Duration;

use crate::worktree::git::GitRunner;

/// Apply `.worktreeinclude`. Step 0 asks git whether the file is ignored and returns no warnings.
pub async fn apply_include(git: &GitRunner, main_root: &Path, tree: &Path) -> Vec<String> {
    let _ = tree;
    let found = git
        .run_read_only(
            main_root,
            &["check-ignore", "-q", "--", ".worktreeinclude"],
            Duration::from_secs(10),
        )
        .await;
    let _ = found;
    Vec::new()
}
