//! Remove a worktree this launch created, when nothing was admitted.

use std::time::Duration;

use crate::worktree::git::{GitRunner, error_line, safe_text};
use crate::worktree::record::{WorktreeRecord, tree_is_contained};

const LOCAL: Duration = Duration::from_secs(10);
const REMOVE: Duration = Duration::from_secs(30);

/// `Ok(None)` means the tree was removed. `Ok(Some(reason))` means the guard kept it.
///
/// The guard keeps a tree that is not under `.worktrees/talaria-` with a `talaria/` branch,
/// and a tree whose branch has a commit beyond the base. An `Err` is a git failure part way.
pub async fn rollback(git: &GitRunner, record: &WorktreeRecord) -> Result<Option<String>, String> {
    let root = record.main_root.as_path();
    let ours = record.path.parent() == Some(root.join(".worktrees").as_path())
        && record
            .path
            .file_name()
            .is_some_and(|name| name.to_string_lossy().starts_with("talaria-"))
        && record.branch.starts_with("talaria/")
        && tree_is_contained(record);
    if !ours {
        return Ok(Some("it is not a tree this launch made".to_owned()));
    }
    let range = format!("{}..{}", record.base_commit, record.branch);
    let ahead = git
        .run_read_only(root, &["rev-list", "--count", &range], LOCAL)
        .await
        .map_err(|error| failure(&format!("{error:?}")))?;
    if ahead.status != 0 {
        return Err(error_line(&ahead));
    }
    if ahead.stdout.trim() != "0" {
        return Ok(Some("it has commits beyond its base".to_owned()));
    }

    let path = record.path.to_string_lossy().into_owned();
    // A failed removal is not final: the folder is deleted below, and the registration
    // is pruned, so only a tree that still stands afterwards is an error.
    let _ = git
        .run(root, &["worktree", "remove", "--force", &path], REMOVE)
        .await;
    if record.path.symlink_metadata().is_ok() {
        // Never follows a symbolic link, so a link made for `.worktreeinclude` is only removed.
        std::fs::remove_dir_all(&record.path).map_err(|error| failure(&error.to_string()))?;
    }
    let pruned = git
        .run(root, &["worktree", "prune"], LOCAL)
        .await
        .map_err(|error| failure(&format!("{error:?}")))?;
    if pruned.status != 0 {
        return Err(error_line(&pruned));
    }
    // Deleting the branch also deletes its `branch.<branch>` record section.
    let deleted = git
        .run(root, &["branch", "-D", &record.branch], LOCAL)
        .await
        .map_err(|error| failure(&format!("{error:?}")))?;
    if deleted.status != 0 {
        return Err(error_line(&deleted));
    }
    Ok(None)
}

fn failure(reason: &str) -> String {
    safe_text(reason)
}
