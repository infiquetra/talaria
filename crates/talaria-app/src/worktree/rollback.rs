//! Remove a worktree this launch created, when nothing was admitted.

use std::time::Duration;

use crate::worktree::git::GitRunner;
use crate::worktree::record::WorktreeRecord;

/// `Ok(None)` means the tree was removed. `Ok(Some(reason))` means the guard kept it.
///
/// Step 0 reports that git was not found, so nothing is removed.
pub async fn rollback(git: &GitRunner, record: &WorktreeRecord) -> Result<Option<String>, String> {
    let found = git
        .run(
            &record.main_root,
            &["worktree", "list", "--porcelain"],
            Duration::from_secs(10),
        )
        .await;
    let _ = found;
    Err("git was not found on this machine.".to_owned())
}
