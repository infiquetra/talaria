//! The git-config record of one worktree.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::bg::outcome::LaunchError;
use crate::worktree::git::{GitRunner, error_line, run_failed, safe_text};

const LOCAL: Duration = Duration::from_secs(10);

/// The key prefix inside `branch.<branch>.`.
const KEY_PREFIX: &str = "talaria-";

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

/// Whether the tree folder really sits in `<main root>/.worktrees`, after resolving links.
///
/// A parent that does not exist counts as contained: there is nothing to delete through it.
pub(crate) fn tree_is_contained(record: &WorktreeRecord) -> bool {
    let Ok(root) = std::fs::canonicalize(&record.main_root) else {
        return false;
    };
    let Some(parent) = record.path.parent() else {
        return false;
    };
    match std::fs::canonicalize(parent) {
        Ok(resolved) => resolved == root.join(".worktrees"),
        Err(error) => error.kind() == std::io::ErrorKind::NotFound,
    }
}

fn key(branch: &str, name: &str) -> String {
    format!("branch.{branch}.{KEY_PREFIX}{name}")
}

async fn set(git: &GitRunner, main_root: &Path, key: &str, value: &str) -> Result<(), LaunchError> {
    let written = git
        .run(main_root, &["config", "--local", key, value], LOCAL)
        .await
        .map_err(|error| run_failed(error, "write the worktree record"))?;
    if written.status == 0 {
        Ok(())
    } else {
        Err(LaunchError {
            what: format!(
                "git could not write the worktree record: {}.",
                error_line(&written)
            ),
            next_step: "Fix what git reports, then try again; nothing was started.".to_owned(),
        })
    }
}

/// Write the six keys known when the tree is created. `session_id` is added later.
pub(crate) async fn write_record(
    git: &GitRunner,
    record: &WorktreeRecord,
) -> Result<(), LaunchError> {
    let relative = record
        .path
        .strip_prefix(&record.main_root)
        .unwrap_or(&record.path)
        .to_string_lossy()
        .into_owned();
    let created = record.created_at.to_string();
    let fetched = record.base_fetched.to_string();
    let fields = [
        ("path", relative.as_str()),
        ("base", record.base_ref.as_str()),
        ("base-commit", record.base_commit.as_str()),
        ("base-fetched", fetched.as_str()),
        ("profile", record.profile.as_str()),
        ("created", created.as_str()),
    ];
    for (name, value) in fields {
        set(git, &record.main_root, &key(&record.branch, name), value).await?;
    }
    Ok(())
}

/// Write `talaria-session` onto an existing record.
pub async fn record_session(
    git: &GitRunner,
    record: &WorktreeRecord,
    session_id: &str,
) -> Result<(), LaunchError> {
    let result = set(
        git,
        &record.main_root,
        &key(&record.branch, "session"),
        session_id,
    )
    .await;
    result.map_err(|error| LaunchError {
        what: format!(
            "could not record the session id in git config ({}).",
            safe_text(error.what.trim_end_matches('.'))
        ),
        next_step: error.next_step,
    })
}

/// Read one record back.
pub async fn read_record(
    git: &GitRunner,
    main_root: &Path,
    branch: &str,
) -> Option<WorktreeRecord> {
    records_in(git, main_root)
        .await
        .into_iter()
        .find(|record| record.branch == branch)
}

/// Every record under a main worktree.
pub async fn records_in(git: &GitRunner, main_root: &Path) -> Vec<WorktreeRecord> {
    let listed = git
        .run_read_only(
            main_root,
            &[
                "config",
                "--local",
                "--get-regexp",
                r"^branch\..*\.talaria-",
            ],
            LOCAL,
        )
        .await;
    match listed {
        Ok(output) if output.status == 0 => parse_records(&output.stdout, main_root),
        _ => Vec::new(),
    }
}

/// Group `key value` lines by branch. A branch with no `talaria-path` is not a record.
fn parse_records(listing: &str, main_root: &Path) -> Vec<WorktreeRecord> {
    let marker = format!(".{KEY_PREFIX}");
    let mut by_branch: BTreeMap<String, BTreeMap<String, String>> = BTreeMap::new();
    for line in listing.lines() {
        let (name, value) = line.split_once(' ').unwrap_or((line, ""));
        let Some(rest) = name.strip_prefix("branch.") else {
            continue;
        };
        let Some(split) = rest.rfind(&marker) else {
            continue;
        };
        let branch = &rest[..split];
        let field = &rest[split + marker.len()..];
        by_branch
            .entry(branch.to_owned())
            .or_default()
            .insert(field.to_owned(), value.to_owned());
    }
    by_branch
        .into_iter()
        .filter_map(|(branch, fields)| {
            let path = fields.get("path")?;
            let text = |name: &str| fields.get(name).cloned().unwrap_or_default();
            Some(WorktreeRecord {
                main_root: main_root.to_path_buf(),
                path: main_root.join(path),
                branch,
                base_ref: text("base"),
                base_commit: text("base-commit"),
                base_fetched: text("base-fetched") == "true",
                profile: text("profile"),
                session_id: fields.get("session").filter(|id| !id.is_empty()).cloned(),
                created_at: text("created").parse().unwrap_or(0),
            })
        })
        .collect()
}
