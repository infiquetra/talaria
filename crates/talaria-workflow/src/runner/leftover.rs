//! Agent working folders that still hold uncommitted changes when a run ends.
use crate::{
    engine::{CallId, EngineEvent, Isolation},
    journal::{CallOutcome, ROUTE_WARNING, Record, RecordLine},
};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    io::Read,
    path::Path,
    process::{Command, Stdio},
    time::{Duration, Instant},
};

/// One agent working folder with edits nobody committed, or a folder agents ran in without
/// worktree isolation.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct LeftoverFolder {
    pub path: String,
    /// Entries `git status` reports in an agent worktree. Not counted (zero) for a folder
    /// agents ran in without isolation, where no git command is run.
    pub changed_files: usize,
    /// The branch that holds a copy of the changes, when the agent ended badly (cut short,
    /// failed, stopped or never finished) and they were saved. The folder keeps its changes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub saved_branch: Option<String>,
    /// For a folder agents ran in without isolation: the agents that ended badly there.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub agents: Vec<String>,
    /// True for a folder agents ran in without worktree isolation. Talaria does not look at
    /// what they changed there: anything they changed is already there, and nothing was saved.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub in_place: bool,
}

/// How long one `git status` may run before it is killed.
const STATUS_LIMIT: Duration = Duration::from_secs(20);

/// What the journal says about one call: where it ran and how it ended.
struct Call<'a> {
    call_id: &'a CallId,
    label: &'a str,
    cwd: Option<&'a str>,
    isolated: bool,
    /// False once a `call_finished` record says it completed in full.
    ended_badly: bool,
}

fn calls(records: &[RecordLine]) -> Vec<Call<'_>> {
    let mut calls: BTreeMap<&str, Call<'_>> = BTreeMap::new();
    for record in records {
        match &record.record {
            Record::CallIntent {
                key,
                call_id,
                label,
                launch,
                ..
            } => {
                calls.insert(
                    key,
                    Call {
                        call_id,
                        label,
                        cwd: launch.cwd.as_deref(),
                        isolated: launch.isolation == Some(Isolation::Worktree),
                        ended_badly: true,
                    },
                );
            }
            Record::CallFinished { key, outcome, .. } => {
                if let Some(call) = calls.get_mut(key.as_str()) {
                    call.ended_badly = match outcome {
                        CallOutcome::Completed { cut_short, .. } => cut_short.is_some(),
                        CallOutcome::Skipped => false,
                        CallOutcome::Failed { .. } | CallOutcome::Stopped { .. } => true,
                    };
                }
            }
            _ => {}
        }
    }
    calls.into_values().collect()
}

/// What the run left in agent folders, in path order, and notes about saves (one that could
/// not be made, or paths git could not read). A worktree Talaria made for one of the run's agents is listed when it holds
/// uncommitted changes; when that agent ended badly, the changes are also saved to a branch
/// (see `save_agent_worktree`), or the branch saved when the call ended is named again. Only
/// folders under the run's own `worktrees` folder, named by the journal's records, are
/// inspected with git: an agent can write repository configuration, so no other folder is
/// trusted with a git command. A folder agents ran in without isolation is listed once, with
/// those that ended badly, and no git command runs in it. Nothing is deleted.
pub(crate) fn leftover_folders(
    records: &[RecordLine],
    run_dir: &Path,
    run_id: &str,
) -> (Vec<LeftoverFolder>, Vec<String>) {
    let mut found = Vec::new();
    let mut problems = Vec::new();
    let calls = calls(records);
    let root = run_dir.join("worktrees").canonicalize().ok();
    // A folder inside the run's worktrees folder is an agent worktree whatever the launch
    // says; any other folder is one agents ran in without isolation unless the launch asked
    // for isolation.
    let inside = |path: &str| {
        root.as_ref().is_some_and(|root| {
            Path::new(path)
                .canonicalize()
                .is_ok_and(|real| real != *root && real.starts_with(root))
        })
    };
    let mut seen = BTreeSet::new();
    for call in &calls {
        let Some(path) = call.cwd.filter(|path| inside(path)) else {
            continue;
        };
        if !seen.insert(path) {
            continue;
        }
        let Some(changed_files) = changed_files(Path::new(path)).filter(|n| *n > 0) else {
            continue;
        };
        let mut saved_branch = None;
        if call.ended_badly {
            let branch = crate::worktree::saved_branch_name(
                run_id,
                &call.call_id.digest,
                call.call_id.occurrence,
            );
            let message = format!(
                "Save uncommitted work of workflow agent '{}'\n\nTalaria saved this when run {run_id} ended without the agent finishing. The agent's worktree was left as it was.",
                call.label
            );
            match crate::worktree::save_agent_worktree(run_dir, Path::new(path), &branch, &message)
            {
                Ok(saved) => {
                    if let Some(note) = saved.skipped_note() {
                        problems.push(format!(
                            "runner: saving the uncommitted work of agent {}: {note}",
                            call.label
                        ));
                    }
                    saved_branch = saved.branch;
                }
                Err(error) => problems.push(format!(
                    "runner: could not save the uncommitted work of agent {} to a branch: {error}",
                    call.label
                )),
            }
        }
        found.push(LeftoverFolder {
            path: path.into(),
            changed_files,
            saved_branch,
            agents: Vec::new(),
            in_place: false,
        });
    }
    let mut in_place: BTreeMap<&str, Vec<String>> = BTreeMap::new();
    for call in calls.iter().filter(|c| !c.isolated && c.ended_badly) {
        if let Some(path) = call.cwd.filter(|path| !inside(path)) {
            let agents = in_place.entry(path).or_default();
            if !agents.iter().any(|a| a == call.label) {
                agents.push(call.label.to_owned());
            }
        }
    }
    found.extend(in_place.into_iter().map(|(path, agents)| LeftoverFolder {
        path: path.into(),
        changed_files: 0,
        saved_branch: None,
        agents,
        in_place: true,
    }));
    found.sort_by(|a, b| a.path.cmp(&b.path));
    (found, problems)
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
    if folder.in_place {
        return format!(
            "agents ran in {} without worktree isolation ({}): anything they changed is already there, and nothing was saved",
            folder.path,
            folder.agents.join(", ")
        );
    }
    format!(
        "uncommitted changes left in an agent working folder: {} ({} changed file{}){}",
        folder.path,
        folder.changed_files,
        if folder.changed_files == 1 { "" } else { "s" },
        folder
            .saved_branch
            .as_ref()
            .map(|b| format!("; a copy is saved on branch {b}"))
            .unwrap_or_default()
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
        call(cwd, 'k', "a", None)
    }
    /// The intent of call `id` (one character, repeated into its key and digest).
    fn call(cwd: &Path, id: char, label: &str, isolation: Option<Isolation>) -> RecordLine {
        RecordLine {
            v: 1,
            seq: 1,
            at_ms: 0,
            record: Record::CallIntent {
                key: id.to_string().repeat(32),
                attempt: 0,
                call_id: crate::engine::CallId {
                    digest: id.to_string().repeat(64),
                    occurrence: 0,
                },
                label: label.into(),
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
                    isolation,
                    cwd: Some(cwd.to_string_lossy().into_owned()),
                },
                request_id: "r".into(),
            },
        }
    }
    fn finished(id: char, outcome: CallOutcome) -> RecordLine {
        RecordLine {
            v: 1,
            seq: 2,
            at_ms: 0,
            record: Record::CallFinished {
                key: id.to_string().repeat(32),
                attempt: 0,
                outcome,
                turns: 1,
            },
        }
    }
    fn completed() -> CallOutcome {
        CallOutcome::Completed {
            value: Some(serde_json::json!("done")),
            value_ref: None,
            cut_short: None,
        }
    }
    fn failed() -> CallOutcome {
        CallOutcome::Failed {
            reason: "stopped in Hermes".into(),
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
        // Both agents completed, so nothing is saved and the folder run in without isolation is not
        // listed.
        let records = [
            call(&own, 'a', "own", None),
            finished('a', completed()),
            call(&foreign, 'b', "foreign", None),
            finished('b', completed()),
        ];
        let (found, problems) = leftover_folders(&records, temp.path(), "run1");
        assert!(problems.is_empty(), "{problems:?}");
        assert_eq!(
            found,
            vec![LeftoverFolder {
                path: own.to_string_lossy().into_owned(),
                changed_files: 1,
                saved_branch: None,
                agents: Vec::new(),
                in_place: false,
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
        let records = [
            call(&lookalike, 'c', "c", Some(Isolation::Worktree)),
            call(&worktrees, 'd', "d", Some(Isolation::Worktree)),
        ];
        assert!(leftover_folders(&records, temp.path(), "run1").0.is_empty());
        assert!(!intent(&own).record.kind().is_empty());
    }

    #[test]
    fn agents_without_isolation_in_a_folder_that_is_not_a_git_repository_lose_nothing() {
        let temp = tempfile::tempdir().unwrap();
        let plain = temp.path().join("project");
        std::fs::create_dir_all(&plain).unwrap();
        std::fs::write(plain.join("draft.txt"), "x").unwrap();
        let records = [
            call(&plain, 'a', "writer", None),
            finished('a', failed()),
            call(&plain, 'b', "checker", None),
            call(&plain, 'c', "done", None),
            finished('c', completed()),
        ];
        let (found, problems) = leftover_folders(&records, temp.path(), "run1");
        assert!(problems.is_empty(), "{problems:?}");
        assert_eq!(found.len(), 1, "{found:?}");
        let folder = &found[0];
        assert!(folder.in_place && folder.saved_branch.is_none());
        assert_eq!(
            folder.agents,
            vec!["writer".to_string(), "checker".to_string()]
        );
        let said = describe(folder);
        // Nothing checked whether they changed anything, so the sentence does not say they did.
        assert!(!said.contains("edited"), "{said}");
        assert!(
            said.starts_with(&format!(
                "agents ran in {} without worktree isolation (writer, checker): ",
                plain.display()
            )),
            "{said}"
        );
        assert!(
            said.ends_with("anything they changed is already there, and nothing was saved"),
            "{said}"
        );
        assert!(!said.contains("uncommitted changes left"), "{said}");
        let body = serde_json::to_value(folder).unwrap();
        assert_eq!(body["in_place"], true);
        // The folder itself was not touched.
        assert!(!plain.join(".git").exists() && plain.join("draft.txt").exists());
    }

    #[test]
    fn an_unfinished_isolated_agent_has_its_work_saved_when_the_run_ends() {
        let temp = tempfile::tempdir().unwrap();
        let repo = temp.path().join("repo");
        std::fs::create_dir_all(&repo).unwrap();
        git(&repo, &["init", "-q", "-b", "main"]);
        git(&repo, &["config", "user.name", "Fixture"]);
        git(&repo, &["config", "user.email", "fixture@example.invalid"]);
        git(&repo, &["commit", "-q", "--allow-empty", "-m", "base"]);
        let run = temp.path().join("run1");
        std::fs::create_dir_all(&run).unwrap();
        let tree = crate::worktree::GitWorktrees::new(run.clone(), repo.clone(), "run1".into())
            .for_call(&"e".repeat(64), 0)
            .unwrap();
        std::fs::write(tree.join("half.txt"), "x").unwrap();
        let records = [call(&tree, 'e', "builder", Some(Isolation::Worktree))];
        let (found, problems) = leftover_folders(&records, &run, "run1");
        assert!(problems.is_empty(), "{problems:?}");
        let expected = format!("talaria/wf/run1/{}-0-saved", "e".repeat(64));
        assert_eq!(found[0].saved_branch.as_deref(), Some(expected.as_str()));
        assert_eq!(found[0].changed_files, 1);
        assert!(describe(&found[0]).contains(&format!("a copy is saved on branch {expected}")));
        // The folder still holds its change; asking again names the same branch.
        assert!(tree.join("half.txt").exists());
        let (again, _) = leftover_folders(&records, &run, "run1");
        assert_eq!(again[0].saved_branch, found[0].saved_branch);
        // An agent that completed in full is listed but not saved.
        let done = [
            call(&tree, 'e', "builder", Some(Isolation::Worktree)),
            finished('e', completed()),
        ];
        assert_eq!(
            leftover_folders(&done, &run, "run1").0[0].saved_branch,
            None
        );
    }
}
