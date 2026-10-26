//! The git-config record of one worktree.

use std::path::{Path, PathBuf};

use crate::bg::outcome::LaunchError;
use crate::worktree::git::GitRunner;

/// One worktree this program created.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WorktreeRecord {
    /// The main worktree.
    pub main_root: PathBuf,
    /// The new worktree.
    pub path: PathBuf,
    /// `talaria/<slug>`.
    pub branch: String,
    /// `origin/<branch>`.
    pub base_ref: String,
    /// The 40-character commit the tree was branched from.
    pub base_commit: String,
    /// Whether the fetch that resolved the commit succeeded.
    pub base_fetched: bool,
    /// The profile the session was created in.
    pub profile: String,
    /// The live session id, once the create is confirmed.
    pub session_id: Option<String>,
    /// Unix seconds when the tree was created.
    pub created_at: i64,
}

fn missing() -> LaunchError {
    LaunchError {
        what: "git was not found on this machine.".to_owned(),
        next_step: "Install git, or run without --worktree.".to_owned(),
    }
}

/// Write `talaria-session` onto an existing record. Step 0 reports git missing.
pub async fn record_session(
    git: &GitRunner,
    record: &WorktreeRecord,
    session_id: &str,
) -> Result<(), LaunchError> {
    let _ = session_id;
    let found = git
        .run(
            &record.main_root,
            &["config", "--local", "--get", "core.repositoryformatversion"],
            std::time::Duration::from_secs(10),
        )
        .await;
    let _ = found;
    Err(missing())
}

/// Read one record back. Step 0 finds none, because git is reported missing.
pub async fn read_record(
    git: &GitRunner,
    main_root: &Path,
    branch: &str,
) -> Option<WorktreeRecord> {
    let _ = branch;
    let found = git
        .run_read_only(
            main_root,
            &["config", "--local", "--get-regexp", "branch"],
            std::time::Duration::from_secs(10),
        )
        .await;
    let _ = found;
    None
}

/// Every record under a main worktree. Step 0 returns none.
pub async fn records_in(git: &GitRunner, main_root: &Path) -> Vec<WorktreeRecord> {
    let found = git
        .run_read_only(
            main_root,
            &["config", "--local", "--get-regexp", "branch"],
            std::time::Duration::from_secs(10),
        )
        .await;
    let _ = found;
    Vec::new()
}
