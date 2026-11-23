//! Listing and removing the worktrees a finished run left behind, on request.
//!
//! The run's `launch.json` names the source checkout and the run's own sidecar records name each
//! worktree; the journal supplies agent labels. Removal never happens automatically.

use std::{
    collections::BTreeMap,
    fs::{self, File, OpenOptions},
    io,
    path::{Path, PathBuf},
};

use serde::Deserialize;
use serde_json::Value;

use crate::worktree::{CleanupEntry, GitWorktrees, WorktreeReport};

/// Why a cleanup did not start.
#[derive(Debug, PartialEq, Eq)]
pub enum CleanupError {
    /// A runner holds the run's lock, so its worktrees are in use.
    Running,
    Failed(String),
}

#[derive(Deserialize)]
struct LaunchCwd {
    cwd: PathBuf,
}

fn has_worktrees(run_dir: &Path) -> bool {
    run_dir.join("worktrees").symlink_metadata().is_ok()
}

fn provider(run_dir: &Path) -> Result<GitWorktrees, String> {
    let launch: LaunchCwd = fs::read(run_dir.join("launch.json"))
        .map_err(|e| e.to_string())
        .and_then(|bytes| serde_json::from_slice(&bytes).map_err(|e| e.to_string()))
        .map_err(|e| format!("cannot read launch.json: {e}"))?;
    let id = run_dir
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or("the run folder has no usable name")?;
    Ok(GitWorktrees::new(
        run_dir.to_path_buf(),
        launch.cwd,
        id.to_owned(),
    ))
}

/// Agent labels by call name (`<digest>-<occurrence>`), read tolerantly: a damaged journal line
/// only costs the labels it would have given.
fn agent_labels(run_dir: &Path) -> BTreeMap<String, String> {
    let mut labels = BTreeMap::new();
    let Ok(bytes) = fs::read(run_dir.join("journal.jsonl")) else {
        return labels;
    };
    for line in bytes.split(|&b| b == b'\n') {
        let Ok(record) = serde_json::from_slice::<Value>(line) else {
            continue;
        };
        if record["t"] != "call_intent" {
            continue;
        }
        let call = &record["call_id"];
        if let (Some(digest), Some(occurrence), Some(label)) = (
            call["digest"].as_str(),
            call["occurrence"].as_u64(),
            record["label"].as_str(),
        ) {
            labels.insert(format!("{digest}-{occurrence}"), label.to_owned());
        }
    }
    labels
}

fn label_all<'a>(run_dir: &Path, rows: impl Iterator<Item = &'a mut WorktreeReport>) {
    let labels = agent_labels(run_dir);
    for row in rows {
        row.agent = labels.get(&row.call).filter(|l| !l.is_empty()).cloned();
    }
}

/// The run's retained worktrees. Safe on a running run: it only reads.
pub fn list(run_dir: &Path) -> Result<Vec<WorktreeReport>, String> {
    if !has_worktrees(run_dir) {
        return Ok(Vec::new());
    }
    let mut rows = provider(run_dir)?.inspect()?;
    label_all(run_dir, rows.iter_mut());
    Ok(rows)
}

/// Remove the run's worktrees as `GitWorktrees::clean_up` describes. A run that a runner holds
/// is refused. While it works, this takes the run's own lock, so a runner cannot start on the
/// run in between.
pub fn clean_up(
    run_dir: &Path,
    force: bool,
    dry_run: bool,
) -> Result<Vec<CleanupEntry>, CleanupError> {
    let held = match OpenOptions::new()
        .read(true)
        .write(true)
        .open(run_dir.join("run.lock"))
    {
        Ok(file) => Some(file),
        Err(e) if e.kind() == io::ErrorKind::NotFound => None,
        Err(e) => return Err(CleanupError::Failed(format!("cannot open run.lock: {e}"))),
    };
    if let Some(file) = &held {
        match file.try_lock() {
            Ok(()) => {}
            Err(fs::TryLockError::WouldBlock) => return Err(CleanupError::Running),
            Err(fs::TryLockError::Error(e)) => {
                return Err(CleanupError::Failed(format!("cannot lock the run: {e}")));
            }
        }
    }
    let result = (|| {
        if !has_worktrees(run_dir) {
            return Ok(Vec::new());
        }
        let mut rows = provider(run_dir)?.clean_up(force, dry_run)?;
        label_all(run_dir, rows.iter_mut().map(|row| &mut row.worktree));
        Ok(rows)
    })()
    .map_err(CleanupError::Failed);
    if let Some(file) = held {
        let _ = File::unlock(&file);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::worktree::CleanupAction;
    use std::{ffi::OsStr, process::Command};

    fn git(repo: &Path, args: &[&str]) {
        let status = Command::new("git")
            .arg("-C")
            .arg(repo)
            .args([
                "-c",
                "user.name=Test",
                "-c",
                "user.email=test@example.invalid",
            ])
            .args(args.iter().map(OsStr::new))
            .status()
            .unwrap();
        assert!(status.success(), "git {args:?}");
    }

    struct Fixture {
        _temp: tempfile::TempDir,
        repo: PathBuf,
        run: PathBuf,
    }

    fn fixture() -> Fixture {
        let temp = tempfile::tempdir().unwrap();
        let repo = temp.path().join("repo");
        fs::create_dir(&repo).unwrap();
        git(&repo, &["init", "-b", "main"]);
        git(&repo, &["commit", "--allow-empty", "-m", "base"]);
        let run = temp.path().join("run-one");
        fs::create_dir(&run).unwrap();
        fs::write(run.join("run.lock"), "").unwrap();
        fs::write(
            run.join("launch.json"),
            serde_json::json!({"cwd": repo}).to_string(),
        )
        .unwrap();
        Fixture {
            _temp: temp,
            repo,
            run,
        }
    }

    impl Fixture {
        fn tree(&self, digit: char) -> PathBuf {
            GitWorktrees::new(self.run.clone(), self.repo.clone(), "run-one".into())
                .for_call(&digit.to_string().repeat(64), 0)
                .unwrap()
        }
    }

    #[test]
    fn labels_come_from_the_journal_and_damaged_lines_are_skipped() {
        let f = fixture();
        f.tree('a');
        f.tree('b');
        let call = |digit: char, label: &str| serde_json::json!({"t":"call_intent","call_id":{"digest":digit.to_string().repeat(64),"occurrence":0},"label":label});
        fs::write(
            f.run.join("journal.jsonl"),
            format!(
                "{}\nnot json at all\n{}\n{{\"t\":\"call_intent\"",
                call('a', "reviewer"),
                call('b', "")
            ),
        )
        .unwrap();
        let rows = list(&f.run).unwrap();
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].agent.as_deref(), Some("reviewer"));
        assert_eq!(rows[1].agent, None);
        assert!(rows.iter().all(|r| r.run_id == "run-one"));
    }

    #[test]
    fn a_run_without_worktrees_lists_nothing_and_needs_no_launch_file() {
        let f = fixture();
        fs::remove_file(f.run.join("launch.json")).unwrap();
        assert!(list(&f.run).unwrap().is_empty());
        assert!(clean_up(&f.run, false, false).unwrap().is_empty());
    }

    #[test]
    fn a_missing_launch_file_is_reported_when_worktrees_exist() {
        let f = fixture();
        f.tree('a');
        fs::remove_file(f.run.join("launch.json")).unwrap();
        assert!(list(&f.run).unwrap_err().contains("launch.json"));
    }

    #[test]
    fn a_run_a_runner_holds_is_refused_and_nothing_changes() {
        let f = fixture();
        let tree = f.tree('a');
        let holder = File::options()
            .read(true)
            .write(true)
            .open(f.run.join("run.lock"))
            .unwrap();
        holder.try_lock().unwrap();
        for (force, dry_run) in [(false, false), (true, false), (false, true)] {
            assert_eq!(
                clean_up(&f.run, force, dry_run).unwrap_err(),
                CleanupError::Running
            );
        }
        assert!(tree.exists());
        // Listing only reads, so it still works on a running run.
        assert_eq!(list(&f.run).unwrap().len(), 1);
        holder.unlock().unwrap();
        let rows = clean_up(&f.run, false, false).unwrap();
        assert_eq!(rows[0].action, CleanupAction::Removed);
        assert!(!tree.exists());
    }

    #[test]
    fn cleanup_frees_the_runs_lock_when_it_is_done() {
        let f = fixture();
        f.tree('a');
        clean_up(&f.run, false, false).unwrap();
        let probe = File::options()
            .read(true)
            .write(true)
            .open(f.run.join("run.lock"))
            .unwrap();
        assert!(probe.try_lock().is_ok());
    }
}
