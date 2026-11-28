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

use super::runs::{RunStatus, runner_alive, summarize_run};
use crate::worktree::{CleanupEntry, GitWorktrees, WorktreeReport};

/// Why a cleanup did not start.
#[derive(Debug, PartialEq, Eq)]
pub enum CleanupError {
    /// A runner holds the run's lock, so its worktrees are in use. Not overridable.
    Running,
    /// The run has no recorded end (parked, interrupted or unreadable), so its agents may still
    /// be working in their folders. Only `force` overrides this.
    NotFinished(RunStatus),
    Failed(String),
}

/// What a cleanup did, and the state of the run it did it to.
#[derive(Debug)]
pub struct CleanupOutcome {
    pub entries: Vec<CleanupEntry>,
    pub status: RunStatus,
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
    let Ok(bytes) = crate::journal::read_journal_bytes(&run_dir.join("journal.jsonl"), 0) else {
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
/// is always refused. A run without a recorded end is refused unless `force` is set. While it
/// works, this takes the run's own lock, so a runner cannot start on the run in between.
pub fn clean_up(
    run_dir: &Path,
    force: bool,
    dry_run: bool,
) -> Result<CleanupOutcome, CleanupError> {
    // Read the status before taking our own lock, which the status probe would see as a runner.
    let status = summarize_run(run_dir).status;
    if !force && !status.is_finished() {
        return Err(match runner_alive(run_dir) {
            Ok(false) => CleanupError::NotFinished(status),
            Ok(true) => CleanupError::Running,
            Err(e) => CleanupError::Failed(format!("cannot probe run.lock: {e}")),
        });
    }
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
            return Ok(CleanupOutcome {
                entries: Vec::new(),
                status,
            });
        }
        let mut entries = provider(run_dir)?.clean_up(force, dry_run)?;
        label_all(run_dir, entries.iter_mut().map(|row| &mut row.worktree));
        Ok(CleanupOutcome { entries, status })
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

    const RUN_ID: &str = "01ARZ3NDEKTSV4RRFFQ69G5FAV";

    struct Fixture {
        _temp: tempfile::TempDir,
        repo: PathBuf,
        run: PathBuf,
    }

    fn started() -> Value {
        serde_json::json!({"t":"run_started","run_id":RUN_ID,"script_sha256":"b".repeat(64)})
    }
    fn intent() -> Value {
        serde_json::json!({"t":"call_intent","key":"a".repeat(32),"attempt":0,
            "call_id":{"digest":"1".repeat(64),"occurrence":0},"label":"review",
            "phase":null,"workflow_path":[],
            "launch":{"profile":"fixture","model":"model","toolsets":[],"skills":[]},
            "request_id":"wf:fixture:k"})
    }
    fn attention() -> Value {
        serde_json::json!({"t":"call_attention","key":"a".repeat(32),"attempt":0,"episode":0,
            "cause":"unknown","turn":0,"admission_id":"admission-0","execution_generation":1,
            "detail":"receipt unknown"})
    }
    fn finished(outcome: &str) -> Value {
        serde_json::json!({"t":"run_finished","outcome":outcome,
            "summary":{"completed":0,"failed":0,"skipped":0,"parked":0,"unfinished":0}})
    }

    /// A run folder with a manifest and a journal holding `records`.
    fn fixture(records: &[Value]) -> Fixture {
        let temp = tempfile::tempdir().unwrap();
        let repo = temp.path().join("repo");
        fs::create_dir(&repo).unwrap();
        git(&repo, &["init", "-b", "main"]);
        git(&repo, &["commit", "--allow-empty", "-m", "base"]);
        let run = temp.path().join(RUN_ID);
        fs::create_dir(&run).unwrap();
        fs::write(run.join("run.lock"), "").unwrap();
        fs::write(
            run.join("launch.json"),
            serde_json::json!({"cwd": repo}).to_string(),
        )
        .unwrap();
        fs::write(
            run.join("manifest.json"),
            serde_json::json!({"v":1,"run_id":RUN_ID,"created_at_ms":1000,
                "workflow":{"name":"fixture","script_sha256":"b".repeat(64)},"args":null,
                "started_at_ms":1000,"seed":7,"profile":"fixture","on_unknown":"wait",
                "talaria_version":"0.7.0","hermes":{},"roster":{},"approval_policy":{}})
            .to_string(),
        )
        .unwrap();
        let mut journal = String::new();
        for (index, record) in records.iter().enumerate() {
            let mut line = record.clone();
            line["v"] = 1.into();
            line["seq"] = (index + 1).into();
            line["at_ms"] = index.into();
            journal.push_str(&format!("{line}\n"));
        }
        fs::write(run.join("journal.jsonl"), journal).unwrap();
        Fixture {
            _temp: temp,
            repo,
            run,
        }
    }

    fn done() -> Fixture {
        fixture(&[started(), finished("completed")])
    }

    #[test]
    fn oversized_journal_is_not_read_for_worktree_labels() {
        let fixture = fixture(&[started(), intent()]);
        File::create(fixture.run.join("journal.jsonl"))
            .unwrap()
            .set_len(64 * 1024 * 1024 + 1)
            .unwrap();
        assert!(agent_labels(&fixture.run).is_empty());
    }

    impl Fixture {
        fn tree(&self, digit: char) -> PathBuf {
            GitWorktrees::new(self.run.clone(), self.repo.clone(), RUN_ID.into())
                .for_call(&digit.to_string().repeat(64), 0)
                .unwrap()
        }
        fn hold(&self) -> File {
            let holder = File::options()
                .read(true)
                .write(true)
                .open(self.run.join("run.lock"))
                .unwrap();
            holder.try_lock().unwrap();
            holder
        }
    }

    #[test]
    fn labels_come_from_the_journal_and_damaged_lines_are_skipped() {
        let f = done();
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
        assert!(rows.iter().all(|r| r.run_id == RUN_ID));
    }

    #[test]
    fn a_run_without_worktrees_lists_nothing_and_needs_no_launch_file() {
        let f = done();
        fs::remove_file(f.run.join("launch.json")).unwrap();
        assert!(list(&f.run).unwrap().is_empty());
        assert!(clean_up(&f.run, false, false).unwrap().entries.is_empty());
    }

    #[test]
    fn a_missing_launch_file_is_reported_when_worktrees_exist() {
        let f = done();
        f.tree('a');
        fs::remove_file(f.run.join("launch.json")).unwrap();
        assert!(list(&f.run).unwrap_err().contains("launch.json"));
    }

    #[test]
    fn a_run_a_runner_holds_is_refused_even_with_force_and_nothing_changes() {
        // One run with a recorded end, one without: a held lock refuses both, even with force.
        for records in [
            vec![started(), finished("completed")],
            vec![started(), intent()],
        ] {
            let f = fixture(&records);
            let tree = f.tree('a');
            let holder = f.hold();
            for (force, dry_run) in [(false, false), (true, false), (false, true), (true, true)] {
                assert_eq!(
                    clean_up(&f.run, force, dry_run).unwrap_err(),
                    CleanupError::Running,
                    "force={force} dry_run={dry_run}"
                );
            }
            assert!(tree.exists());
            // Listing only reads, so it still works on a running run.
            assert_eq!(list(&f.run).unwrap().len(), 1);
            holder.unlock().unwrap();
        }
    }

    #[test]
    fn a_finished_run_is_cleaned_without_force_whatever_its_outcome() {
        for outcome in [
            "completed",
            "completed_with_failures",
            "budget_exhausted",
            "failed",
            "aborted",
        ] {
            let f = fixture(&[started(), finished(outcome)]);
            let tree = f.tree('a');
            let done = clean_up(&f.run, false, false).unwrap();
            assert!(done.status.is_finished(), "{outcome}");
            assert_eq!(done.entries[0].action, CleanupAction::Removed, "{outcome}");
            assert!(!tree.exists());
        }
    }

    #[test]
    fn an_interrupted_or_parked_or_unreadable_run_is_refused_without_force() {
        let cases = [
            (vec![started(), intent()], RunStatus::Interrupted),
            (vec![started(), intent(), attention()], RunStatus::Parked),
            (Vec::new(), RunStatus::Unreadable),
        ];
        for (records, expected) in cases {
            let f = fixture(&records);
            if expected == RunStatus::Unreadable {
                fs::remove_file(f.run.join("manifest.json")).unwrap();
            }
            let tree = f.tree('a');
            for dry_run in [false, true] {
                assert_eq!(
                    clean_up(&f.run, false, dry_run).unwrap_err(),
                    CleanupError::NotFinished(expected),
                    "{expected:?} dry_run={dry_run}"
                );
            }
            assert!(tree.exists(), "{expected:?}");
            assert_eq!(list(&f.run).unwrap().len(), 1);
        }
    }

    #[test]
    fn force_removes_an_unfinished_runs_worktrees_and_reports_its_status() {
        let cases = [
            (vec![started(), intent()], RunStatus::Interrupted),
            (vec![started(), intent(), attention()], RunStatus::Parked),
        ];
        for (records, expected) in cases {
            let f = fixture(&records);
            let tree = f.tree('a');
            let preview = clean_up(&f.run, true, true).unwrap();
            assert_eq!(preview.status, expected);
            assert_eq!(preview.entries[0].action, CleanupAction::WouldRemove);
            assert!(tree.exists());
            let done = clean_up(&f.run, true, false).unwrap();
            assert_eq!(done.status, expected);
            assert_eq!(done.entries[0].action, CleanupAction::Removed);
            assert!(!tree.exists());
        }
    }

    #[test]
    fn cleanup_frees_the_runs_lock_when_it_is_done() {
        let f = done();
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
