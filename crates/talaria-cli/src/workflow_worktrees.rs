//! `talaria workflow worktrees`: list and remove the worktrees and branches runs leave behind.
//! Nothing is removed unless a person runs `remove`.
use std::{
    io::{self, Write},
    path::PathBuf,
};

use clap::{Args, Subcommand};
use serde_json::json;
use talaria_workflow::{
    runner::{self, FindError, worktrees::CleanupError},
    worktree::{CleanupAction, CleanupEntry, WorktreeReport},
};

use crate::{
    args::{required_path, required_text},
    workflow::safe,
};

/// List or remove the git worktrees and branches that workflow runs keep for review.
#[derive(Clone, Debug, Args)]
pub struct WorktreesArgs {
    #[command(subcommand)]
    pub command: WorktreesCommand,
}
#[derive(Clone, Debug, Subcommand)]
pub enum WorktreesCommand {
    /// List a run's agent worktrees (or every run's): folder, branch, uncommitted changes, and
    /// commits the source checkout's HEAD does not contain.
    List(ListArgs),
    /// Remove a run's agent worktrees and their branches. Without --force, keeps any worktree
    /// that has uncommitted changes or commits the source checkout's HEAD lacks.
    Remove(RemoveArgs),
}
#[derive(Clone, Debug, Args)]
pub struct ListArgs {
    /// The run to list; every run in the runs directory when left out.
    #[arg(value_name = "RUN", value_parser = required_text)]
    pub target: Option<String>,
    /// Print one JSON object instead of text.
    #[arg(long)]
    pub json: bool,
    /// Use this runs directory instead of the configured one.
    #[arg(long, value_parser = required_path)]
    pub runs_dir: Option<PathBuf>,
}
#[derive(Clone, Debug, Args)]
pub struct RemoveArgs {
    /// The run whose worktrees to remove.
    #[arg(value_name = "RUN", value_parser = required_text)]
    pub target: String,
    /// Also remove worktrees with uncommitted changes or unmerged commits, and say what is lost.
    #[arg(long)]
    pub force: bool,
    /// Show what would happen and change nothing.
    #[arg(long)]
    pub dry_run: bool,
    /// Print one JSON object instead of text.
    #[arg(long)]
    pub json: bool,
    /// Use this runs directory instead of the configured one.
    #[arg(long, value_parser = required_path)]
    pub runs_dir: Option<PathBuf>,
}

fn fail(text: impl std::fmt::Display, code: u8) -> u8 {
    eprintln!("{}", safe(&text.to_string()));
    code
}

fn print(text: &str) -> u8 {
    match io::stdout().write_all(text.as_bytes()) {
        Ok(()) => 0,
        Err(e) => fail(e, 1),
    }
}

fn plural(count: usize, noun: &str) -> String {
    format!("{count} {noun}{}", if count == 1 { "" } else { "s" })
}

pub fn execute(args: WorktreesArgs) -> u8 {
    match args.command {
        WorktreesCommand::List(args) => list(args),
        WorktreesCommand::Remove(args) => remove(args),
    }
}

fn list(args: ListArgs) -> u8 {
    let runs = runner::runs_dir(args.runs_dir.as_deref());
    let mut rows = Vec::new();
    let mut warnings = Vec::new();
    if let Some(target) = &args.target {
        let dir = match runner::find_run(&runs, target) {
            Ok(dir) => dir,
            Err(e) => return fail(e, 2),
        };
        match runner::worktrees::list(&dir) {
            Ok(found) => rows = found,
            Err(e) => return fail(format!("run {target}: {e}"), 1),
        }
    } else {
        for run in runner::discover_runs(&runs) {
            match runner::worktrees::list(std::path::Path::new(&run.run_dir)) {
                Ok(found) => rows.extend(found),
                Err(e) => warnings.push(format!("run {}: {e}", run.run_id)),
            }
        }
    }
    if args.json {
        let body = json!({"worktrees": rows, "warnings": warnings});
        return print(&format!("{body}\n"));
    }
    let mut text = String::new();
    for row in &rows {
        text.push_str(&row_line(row));
    }
    if rows.is_empty() {
        text.push_str("No workflow worktrees found.\n");
    }
    for warning in &warnings {
        text.push_str(&format!("warning: {}\n", safe(warning)));
    }
    print(&text)
}

fn row_line(row: &WorktreeReport) -> String {
    let changes = if row.changed_files == 0 {
        "clean".to_string()
    } else {
        plural(row.changed_files, "changed file")
    };
    let commits = if row.unmerged_commits == 0 {
        "all commits in HEAD".to_string()
    } else {
        format!("{} not in HEAD", plural(row.unmerged_commits, "commit"))
    };
    let mut line = format!(
        "{}  agent {}  {}  branch {}{}  {}  {}\n",
        safe(&row.run_id),
        safe(row.agent.as_deref().unwrap_or("(unknown)")),
        safe(&row.path.to_string_lossy()),
        safe(&row.branch),
        if row.branch_exists {
            ""
        } else {
            " (branch missing)"
        },
        if row.folder_exists {
            changes
        } else {
            "folder missing".to_string()
        },
        commits,
    );
    if let Some(problem) = &row.problem {
        line.push_str(&format!("  problem: {}\n", safe(problem)));
    }
    line
}

fn remove(args: RemoveArgs) -> u8 {
    let runs = runner::runs_dir(args.runs_dir.as_deref());
    let dir = match runner::find_run(&runs, &args.target) {
        Ok(dir) => dir,
        Err(e @ (FindError::UnknownId { .. } | FindError::NotAFolder { .. })) => {
            return fail(e, 2);
        }
        Err(e) => return fail(e, 1),
    };
    let run_id = dir
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let entries = match runner::worktrees::clean_up(&dir, args.force, args.dry_run) {
        Ok(entries) => entries,
        Err(CleanupError::Running) => {
            return fail(
                format!(
                    "run {run_id} is still running; stop it first (talaria workflow stop {run_id}). Nothing was changed."
                ),
                1,
            );
        }
        Err(CleanupError::Failed(e)) => return fail(format!("run {run_id}: {e}"), 1),
    };
    let failed = entries
        .iter()
        .any(|entry| entry.action == CleanupAction::Failed);
    if args.json {
        let body = json!({
            "run_id": run_id,
            "force": args.force,
            "dry_run": args.dry_run,
            "entries": entries,
        });
        let code = print(&format!("{body}\n"));
        return if failed { 1 } else { code };
    }
    let code = print(&remove_text(&run_id, &entries, args.force, args.dry_run));
    if failed { 1 } else { code }
}

fn remove_text(run_id: &str, entries: &[CleanupEntry], force: bool, dry_run: bool) -> String {
    let mut text = String::new();
    if dry_run {
        text.push_str("Dry run: nothing is changed.\n");
    }
    for entry in entries {
        let who = format!(
            "{} (agent {})",
            safe(&entry.worktree.path.to_string_lossy()),
            safe(entry.worktree.agent.as_deref().unwrap_or("unknown"))
        );
        match entry.action {
            CleanupAction::Removed | CleanupAction::WouldRemove => {
                let verb = if entry.action == CleanupAction::Removed {
                    "removed"
                } else {
                    "would remove"
                };
                text.push_str(&format!(
                    "{verb} {who} and branch {}\n",
                    safe(&entry.worktree.branch)
                ));
                let lost = if entry.action == CleanupAction::Removed {
                    "discarded"
                } else {
                    "would discard"
                };
                for item in &entry.discarded {
                    text.push_str(&format!("  {lost}: {}\n", safe(item)));
                }
            }
            CleanupAction::Kept => text.push_str(&format!(
                "kept {who}: {}\n",
                safe(&entry.reasons.join("; "))
            )),
            CleanupAction::Failed => text.push_str(&format!(
                "failed {who}: {}\n",
                safe(&entry.reasons.join("; "))
            )),
        }
    }
    let count = |action| entries.iter().filter(|e| e.action == action).count();
    let removed = count(CleanupAction::Removed) + count(CleanupAction::WouldRemove);
    let kept = count(CleanupAction::Kept);
    let failed = count(CleanupAction::Failed);
    if entries.is_empty() {
        text.push_str(&format!(
            "Run {} has no worktrees to remove.\n",
            safe(run_id)
        ));
    } else {
        text.push_str(&format!(
            "{} {}, {} kept, {} failed.\n",
            if dry_run { "Would remove" } else { "Removed" },
            removed,
            kept,
            failed
        ));
        if kept > 0 && !force {
            text.push_str(
                "Kept worktrees have uncommitted changes or commits the source checkout lacks; --force removes them and discards that work.\n",
            );
        }
    }
    text
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::workflow::{WorkflowCommand, parse};
    use std::{ffi::OsStr, fs, process::Command};
    use talaria_workflow::worktree::GitWorktrees;

    fn args(parts: &[&str]) -> Vec<String> {
        parts.iter().map(|p| p.to_string()).collect()
    }

    fn worktrees(parts: &[&str]) -> WorktreesCommand {
        let parsed = parse(&args(parts)).unwrap();
        match parsed.command {
            WorkflowCommand::Worktrees(a) => a.command,
            other => panic!("not worktrees: {other:?}"),
        }
    }

    #[test]
    fn the_subcommands_and_their_flags_parse() {
        let WorktreesCommand::List(l) = worktrees(&["workflow", "worktrees", "list", "--json"])
        else {
            panic!()
        };
        assert!(l.target.is_none() && l.json);
        let WorktreesCommand::List(l) = worktrees(&["workflow", "worktrees", "list", "run1"])
        else {
            panic!()
        };
        assert_eq!(l.target.as_deref(), Some("run1"));
        let WorktreesCommand::Remove(r) = worktrees(&[
            "workflow",
            "worktrees",
            "remove",
            "run1",
            "--force",
            "--dry-run",
            "--json",
        ]) else {
            panic!()
        };
        assert!(r.force && r.dry_run && r.json && r.target == "run1");
        assert!(parse(&args(&["workflow", "worktrees", "remove"])).is_err());
    }

    fn git(repo: &std::path::Path, args: &[&str]) {
        let status = Command::new("git")
            .arg("-C")
            .arg(repo)
            .args(["-c", "user.name=Test", "-c", "user.email=t@example.invalid"])
            .args(args.iter().map(OsStr::new))
            .status()
            .unwrap();
        assert!(status.success());
    }

    /// A runs directory holding one finished run with two worktrees: one clean, one dirty.
    fn runs_fixture() -> (tempfile::TempDir, PathBuf, PathBuf, PathBuf) {
        let temp = tempfile::tempdir().unwrap();
        let repo = temp.path().join("repo");
        fs::create_dir(&repo).unwrap();
        git(&repo, &["init", "-b", "main"]);
        git(&repo, &["commit", "--allow-empty", "-m", "base"]);
        let runs = temp.path().join("runs");
        let run = runs.join("run-one");
        fs::create_dir_all(&run).unwrap();
        fs::write(run.join("run.lock"), "").unwrap();
        fs::write(
            run.join("launch.json"),
            serde_json::json!({"cwd": repo}).to_string(),
        )
        .unwrap();
        let provider = GitWorktrees::new(run.clone(), repo.clone(), "run-one".into());
        let clean = provider.for_call(&"a".repeat(64), 0).unwrap();
        let dirty = provider.for_call(&"b".repeat(64), 0).unwrap();
        fs::write(dirty.join("note.txt"), "x").unwrap();
        (temp, runs, clean, dirty)
    }

    fn runs_arg(runs: &std::path::Path) -> String {
        runs.to_string_lossy().into_owned()
    }

    #[test]
    fn remove_through_the_command_keeps_dirty_work_and_force_takes_it() {
        let (_temp, runs, clean, dirty) = runs_fixture();
        let runs_dir = runs_arg(&runs);
        let remove = |extra: &[&str]| {
            let mut parts = vec!["workflow", "worktrees", "remove", "run-one", "--runs-dir"];
            parts.push(&runs_dir);
            parts.extend(extra);
            let WorktreesCommand::Remove(a) = worktrees(&parts) else {
                panic!()
            };
            execute(WorktreesArgs {
                command: WorktreesCommand::Remove(a),
            })
        };
        assert_eq!(remove(&["--dry-run"]), 0);
        assert!(clean.exists() && dirty.exists());
        assert_eq!(remove(&[]), 0);
        assert!(!clean.exists(), "the clean worktree is removed");
        assert!(dirty.join("note.txt").exists(), "the dirty one is kept");
        assert_eq!(remove(&["--force"]), 0);
        assert!(!dirty.exists());
    }

    #[test]
    fn the_command_refuses_a_run_a_runner_holds_and_an_unknown_run() {
        let (_temp, runs, clean, _dirty) = runs_fixture();
        let runs_dir = runs_arg(&runs);
        let holder = fs::File::options()
            .read(true)
            .write(true)
            .open(runs.join("run-one").join("run.lock"))
            .unwrap();
        holder.try_lock().unwrap();
        let go = |target: &str| {
            let parts = [
                "workflow",
                "worktrees",
                "remove",
                target,
                "--force",
                "--runs-dir",
                &runs_dir,
            ];
            let WorktreesCommand::Remove(a) = worktrees(&parts) else {
                panic!()
            };
            execute(WorktreesArgs {
                command: WorktreesCommand::Remove(a),
            })
        };
        assert_eq!(go("run-one"), 1);
        assert!(clean.exists());
        assert_eq!(go("no-such-run"), 2);
    }

    #[test]
    fn row_lines_say_what_each_worktree_holds() {
        let (_temp, runs, _clean, _dirty) = runs_fixture();
        let rows = runner::worktrees::list(&runs.join("run-one")).unwrap();
        assert_eq!(rows.len(), 2);
        let first = row_line(&rows[0]);
        assert!(
            first.contains("run-one") && first.contains("clean"),
            "{first}"
        );
        assert!(first.contains("all commits in HEAD"), "{first}");
        let second = row_line(&rows[1]);
        assert!(second.contains("1 changed file"), "{second}");
        let body = serde_json::to_value(&rows[1]).unwrap();
        assert_eq!(body["changed_files"], 1);
        assert_eq!(body["unmerged_commits"], 0);
        assert!(body["path"].is_string() && body["branch"].is_string());
    }
}
