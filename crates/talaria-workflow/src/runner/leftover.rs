//! Agent working folders that still hold uncommitted changes when a run ends.
use crate::{
    engine::EngineEvent,
    journal::{ROUTE_WARNING, Record, RecordLine},
};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeSet,
    io::Read,
    path::Path,
    process::{Command, Stdio},
    time::{Duration, Instant},
};

/// One agent working folder with edits nobody committed.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct LeftoverFolder {
    pub path: String,
    pub changed_files: usize,
}

/// How long one `git status` may run before it is killed.
const STATUS_LIMIT: Duration = Duration::from_secs(20);

/// The worktrees Talaria made for this run's agents that hold uncommitted changes, in path
/// order. Only folders under the run's own `worktrees_dir`, named by the journal's records,
/// are inspected: an agent can write repository configuration, so no other folder is trusted
/// with a git command. Nothing is changed or deleted; a folder that is not a git work tree,
/// or that is gone, is left out.
pub(crate) fn leftover_folders(
    records: &[RecordLine],
    worktrees_dir: &Path,
) -> Vec<LeftoverFolder> {
    let Ok(root) = worktrees_dir.canonicalize() else {
        return Vec::new();
    };
    let folders: BTreeSet<&str> = records
        .iter()
        .filter_map(|r| match &r.record {
            Record::CallIntent { launch, .. } => launch.cwd.as_deref(),
            _ => None,
        })
        .collect();
    folders
        .into_iter()
        .filter(|path| {
            Path::new(path)
                .canonicalize()
                .is_ok_and(|real| real != root && real.starts_with(&root))
        })
        .filter_map(|path| {
            changed_files(Path::new(path)).map(|changed_files| LeftoverFolder {
                path: path.into(),
                changed_files,
            })
        })
        .filter(|folder| folder.changed_files > 0)
        .collect()
}

/// The read-only status command. Repository configuration in an agent's folder can name
/// programs (`core.fsmonitor`), so both options that could run one are switched off, submodules are
/// not entered, and the command takes no optional locks.
fn status_command(folder: &Path) -> Command {
    let mut command = Command::new("git");
    command
        .args([
            "-c",
            "core.fsmonitor=false",
            "-c",
            "core.untrackedCache=false",
        ])
        .arg("-C")
        .arg(folder)
        .args([
            "status",
            "--porcelain",
            "--no-renames",
            "--untracked-files=normal",
            "--ignore-submodules=all",
        ])
        .env("GIT_OPTIONAL_LOCKS", "0")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    command
}

fn changed_files(folder: &Path) -> Option<usize> {
    if !folder.is_dir() {
        return None;
    }
    let mut child = status_command(folder).spawn().ok()?;
    let mut stdout = child.stdout.take()?;
    let reader = std::thread::spawn(move || {
        let mut text = String::new();
        let _ = stdout.read_to_string(&mut text);
        text
    });
    let deadline = Instant::now() + STATUS_LIMIT;
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break Some(status),
            Ok(None) if Instant::now() < deadline => {
                std::thread::sleep(Duration::from_millis(20));
            }
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                break None;
            }
        }
    };
    let text = reader.join().ok()?;
    status?
        .success()
        .then(|| text.lines().filter(|line| !line.trim().is_empty()).count())
}

/// The plain sentence shown for one leftover folder.
pub(crate) fn describe(folder: &LeftoverFolder) -> String {
    format!(
        "uncommitted changes left in an agent working folder: {} ({} changed file{})",
        folder.path,
        folder.changed_files,
        if folder.changed_files == 1 { "" } else { "s" }
    )
}

/// The journal's notes about work found on an agent's route that the run was not following,
/// without their fixed opening words.
pub(crate) fn untracked_work(records: &[RecordLine]) -> Vec<String> {
    records
        .iter()
        .filter_map(|r| match &r.record {
            Record::Engine {
                event: Some(EngineEvent::Log { message, .. }),
                ..
            } => message.strip_prefix(ROUTE_WARNING).map(str::to_owned),
            _ => None,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::journal::ResolvedLaunch;
    use std::ffi::OsStr;

    fn intent(cwd: &Path) -> RecordLine {
        RecordLine {
            v: 1,
            seq: 1,
            at_ms: 0,
            record: Record::CallIntent {
                key: "k".repeat(32),
                attempt: 0,
                call_id: crate::engine::CallId {
                    digest: "0".repeat(64),
                    occurrence: 0,
                },
                label: "a".into(),
                phase: None,
                workflow_path: vec![],
                launch: ResolvedLaunch {
                    profile: "p".into(),
                    model: "m".into(),
                    provider: None,
                    effort: None,
                    toolsets: vec![],
                    skills: vec![],
                    max_turns: None,
                    isolation: None,
                    cwd: Some(cwd.to_string_lossy().into_owned()),
                },
                request_id: "r".into(),
            },
        }
    }
    fn git(dir: &Path, args: &[&str]) {
        assert!(
            Command::new("git")
                .args(args)
                .current_dir(dir)
                .status()
                .unwrap()
                .success()
        );
    }

    #[test]
    fn the_status_command_switches_off_programs_a_repository_can_name() {
        let command = status_command(Path::new("/work"));
        let args: Vec<&OsStr> = command.get_args().collect();
        let want = [
            "-c",
            "core.fsmonitor=false",
            "-c",
            "core.untrackedCache=false",
            "-C",
            "/work",
            "status",
            "--porcelain",
            "--no-renames",
            "--untracked-files=normal",
            "--ignore-submodules=all",
        ];
        assert_eq!(args, want.map(OsStr::new));
        let locks = command
            .get_envs()
            .find(|(name, _)| *name == OsStr::new("GIT_OPTIONAL_LOCKS"));
        assert_eq!(
            locks,
            Some((OsStr::new("GIT_OPTIONAL_LOCKS"), Some(OsStr::new("0"))))
        );
    }

    #[test]
    fn only_folders_under_the_runs_worktrees_are_inspected_and_no_repository_program_runs() {
        let temp = tempfile::tempdir().unwrap();
        let worktrees = temp.path().join("worktrees");
        let own = worktrees.join("call-0");
        let foreign = temp.path().join("elsewhere");
        for folder in [&own, &foreign] {
            std::fs::create_dir_all(folder).unwrap();
            git(folder, &["init", "-q"]);
            std::fs::write(folder.join("edit.txt"), "x").unwrap();
        }
        // The agent's own repository configuration names a program for git to run.
        let marker = temp.path().join("ran");
        let script = temp.path().join("monitor.sh");
        std::fs::write(
            &script,
            format!("#!/bin/sh\ntouch '{}'\n", marker.display()),
        )
        .unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
        git(
            &own,
            &[
                "config",
                "--local",
                "core.fsmonitor",
                script.to_str().unwrap(),
            ],
        );
        git(
            &foreign,
            &[
                "config",
                "--local",
                "core.fsmonitor",
                script.to_str().unwrap(),
            ],
        );
        let records = [intent(&own), intent(&foreign)];
        let found = leftover_folders(&records, &worktrees);
        assert_eq!(
            found,
            vec![LeftoverFolder {
                path: own.to_string_lossy().into_owned(),
                changed_files: 1
            }]
        );
        assert!(
            !marker.exists(),
            "a repository program ran during git status"
        );
        // The worktrees directory itself and a path that only looks like it are not inspected.
        let lookalike = temp.path().join("worktrees-copy");
        std::fs::create_dir_all(&lookalike).unwrap();
        git(&lookalike, &["init", "-q"]);
        std::fs::write(lookalike.join("edit.txt"), "x").unwrap();
        assert!(leftover_folders(&[intent(&lookalike), intent(&worktrees)], &worktrees).is_empty());
    }
}
