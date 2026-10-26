//! Create a worktree. Step 0 stops when git is reported missing.

use std::path::Path;
use std::time::Duration;

use crate::bg::outcome::LaunchError;
use crate::bg::service::CancelToken;
use crate::worktree::git::GitRunner;
use crate::worktree::record::WorktreeRecord;

fn missing() -> LaunchError {
    LaunchError {
        what: "git was not found on this machine.".to_owned(),
        next_step: "Install git, or run without --worktree.".to_owned(),
    }
}

/// Create the worktree, copy the include list, and write the record.
///
/// Step 0 calls the runner and returns the git-missing error. No tree is created.
pub async fn prepare_worktree(
    git: &GitRunner,
    folder: &Path,
    slug: &str,
    profile: &str,
    now_unix: i64,
    cancel: &CancelToken,
) -> Result<(WorktreeRecord, Vec<String>), LaunchError> {
    let _ = (slug, profile, now_unix, cancel.is_cancelled());
    let found = git
        .run_read_only(
            folder,
            &["rev-parse", "--is-inside-work-tree"],
            Duration::from_secs(10),
        )
        .await;
    let _ = found;
    Err(missing())
}

/// The main worktree that contains `folder`. Step 0 reports git missing.
pub async fn main_root_of(
    git: &GitRunner,
    folder: &Path,
) -> Result<std::path::PathBuf, LaunchError> {
    let found = git
        .run_read_only(
            folder,
            &["rev-parse", "--show-toplevel"],
            Duration::from_secs(10),
        )
        .await;
    let _ = found;
    Err(missing())
}
