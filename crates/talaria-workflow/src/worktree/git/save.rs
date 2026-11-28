//! Saving an agent worktree's uncommitted work to a branch without disturbing the worktree.
//!
//! The folder's own branch, index and files are left exactly as they are: the snapshot is
//! built in a temporary index file, written as a tree, committed with the folder's HEAD as its
//! parent and published under a new branch name. Nothing is stashed, staged or checked out.

use super::*;

/// Added to a worktree's own branch name to name the branch its saved work goes to; a second
/// save of different work gets `-saved-2`, and so on.
pub const SAVED_SUFFIX: &str = "-saved";

/// Who a saved commit names when the repository has no identity configured.
const NEUTRAL_NAME: &str = "Talaria workflow";
const NEUTRAL_EMAIL: &str = "talaria-workflow@localhost.invalid";

/// The folder every command runs in, and environment every command gets (empty outside tests).
struct Folder<'a> {
    path: &'a Path,
    env: &'a [(&'a str, &'a OsStr)],
}

fn git_text(folder: &Folder, args: &[&OsStr], env: &[(&str, &OsStr)]) -> Result<String, String> {
    let mut all = folder.env.to_vec();
    all.extend_from_slice(env);
    let output = run_with(folder.path, args, &all)?;
    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
    } else {
        let message = reason(&output);
        Err(if message.is_empty() {
            format!("git exited with {}", output.status)
        } else {
            message
        })
    }
}

/// Whether git would take an author and a committer from the configuration or environment,
/// without guessing one from the host name.
fn identity_configured(folder: &Folder) -> bool {
    ["GIT_AUTHOR_IDENT", "GIT_COMMITTER_IDENT"]
        .iter()
        .all(|var| {
            git_text(
                folder,
                &[
                    "-c".as_ref(),
                    "user.useConfigOnly=true".as_ref(),
                    "var".as_ref(),
                    var.as_ref(),
                ],
                &[],
            )
            .is_ok()
        })
}

/// What one save did.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SavedWork {
    /// The branch that holds the snapshot, or `None` when nothing git could read differs from
    /// the folder's HEAD.
    pub branch: Option<String>,
    /// Paths git could not read (a file it may not open, a folder it may not list, a nested
    /// repository with no commit). They are not in the snapshot and stay in the folder.
    pub skipped: Vec<String>,
}

/// How many skipped paths a saved commit's message names; the rest are counted.
const SKIPPED_IN_MESSAGE: usize = 50;
/// How many skipped paths a journal note names; the rest are counted.
const SKIPPED_IN_NOTE: usize = 5;

impl SavedWork {
    /// A plain clause naming the paths git could not read, or `None` when there are none.
    pub fn skipped_note(&self) -> Option<String> {
        if self.skipped.is_empty() {
            return None;
        }
        let n = self.skipped.len();
        Some(format!(
            "git could not read {n} path{}, which {} not saved and {} still in the worktree: {}",
            if n == 1 { "" } else { "s" },
            if n == 1 { "is" } else { "are" },
            if n == 1 { "is" } else { "are" },
            listed(&self.skipped, SKIPPED_IN_NOTE, ", ")
        ))
    }
}

/// The first `shown` of `paths`, control characters escaped, then a count of the rest.
fn listed(paths: &[String], shown: usize, separator: &str) -> String {
    let mut text = paths
        .iter()
        .take(shown)
        .map(|path| {
            path.chars()
                .flat_map(|c| {
                    if c.is_control() {
                        c.escape_default().collect::<Vec<_>>()
                    } else {
                        vec![c]
                    }
                })
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join(separator);
    if paths.len() > shown {
        text.push_str(&format!("{separator}and {} more", paths.len() - shown));
    }
    text
}

/// The paths `git add --ignore-errors` reports it could not read, from its untranslated
/// error output: "error: unable to index file '<path>'" and "warning: could not open directory
/// '<path>/'". A name can hold a newline, so a path ends at a quote that closes its line and is
/// followed by the next message or the end.
fn unreadable_paths(stderr: &str) -> Vec<String> {
    const OPENINGS: [&str; 2] = [
        "error: unable to index file '",
        "warning: could not open directory '",
    ];
    let mut found = Vec::new();
    let mut rest = stderr;
    while let Some((at, opening)) = OPENINGS
        .iter()
        .filter_map(|opening| rest.find(opening).map(|at| (at, *opening)))
        .min()
    {
        let after = &rest[at + opening.len()..];
        let mut end = None;
        let mut from = 0;
        while let Some(quote) = after[from..].find('\'') {
            let quote = from + quote;
            let tail = &after[quote + 1..];
            let line_end = tail.find('\n').unwrap_or(tail.len());
            let next = tail.get(line_end + 1..).unwrap_or("");
            // The directory warning goes on after the quote (": Permission denied").
            let closes = if opening.starts_with("warning") {
                tail.starts_with(": ")
            } else {
                line_end == 0
            };
            if closes
                && (next.is_empty()
                    || ["error: ", "warning: ", "fatal: "]
                        .iter()
                        .any(|p| next.starts_with(p)))
            {
                end = Some(quote);
                break;
            }
            from = quote + 1;
        }
        let Some(end) = end else { break };
        let path = after[..end].trim_end_matches('/');
        if !path.is_empty() && !found.iter().any(|p| p == path) {
            found.push(path.to_owned());
        }
        rest = &after[end..];
    }
    found
}

/// Save `folder`'s uncommitted changes, tracked and untracked (ignored files stay out), as a
/// commit on a new branch named `branch` (or `branch-2`, `branch-3` and so on when that name is
/// taken by other work). The branch is `None` when there is nothing to save. When a branch of
/// that series already holds exactly this work on this HEAD, its name is returned and nothing
/// new is made. A path git cannot read is left out, named in the commit's message and in
/// [`SavedWork::skipped`], and everything else is still saved; only a snapshot that cannot be
/// written at all fails the save.
pub fn save_uncommitted(folder: &Path, branch: &str, message: &str) -> Result<SavedWork, String> {
    save_in(
        &Folder {
            path: folder,
            env: &[],
        },
        branch,
        message,
    )
}

fn save_in(folder: &Folder, branch: &str, message: &str) -> Result<SavedWork, String> {
    let head = git_text(
        folder,
        &[
            "rev-parse".as_ref(),
            "--verify".as_ref(),
            "HEAD^{commit}".as_ref(),
        ],
        &[],
    )
    .map_err(|e| format!("cannot read the worktree's HEAD: {e}"))?;
    let head_tree = git_text(
        folder,
        &["rev-parse".as_ref(), format!("{head}^{{tree}}").as_ref()],
        &[],
    )?;
    let index_name = format!("talaria-save-{}.index", std::process::id());
    let index = PathBuf::from(git_text(
        folder,
        &[
            "rev-parse".as_ref(),
            "--path-format=absolute".as_ref(),
            "--git-path".as_ref(),
            index_name.as_ref(),
        ],
        &[],
    )?);
    let mut skipped = Vec::new();
    let tree = (|| {
        // Every command on the temporary index goes through the shared repository hardening:
        // writing an index can re-read a racily clean file through its filter.
        let env = vec![("GIT_INDEX_FILE", index.as_os_str())];
        git_text(folder, &["read-tree".as_ref(), head.as_ref()], &env)?;
        let mut add_env = folder.env.to_vec();
        add_env.extend_from_slice(&env);
        add_env.push(("LC_ALL", OsStr::new("C")));
        let added = run_with(
            folder.path,
            &[
                "add".as_ref(),
                "--all".as_ref(),
                "--ignore-errors".as_ref(),
                "--".as_ref(),
                ":/".as_ref(),
            ],
            &add_env,
        )?;
        // With --ignore-errors git indexes everything it can read and exits 1 when it could
        // not read something; the snapshot still holds the rest.
        skipped = unreadable_paths(&String::from_utf8_lossy(&added.stderr));
        if !added.status.success() && added.status.code() != Some(1) {
            let message = reason(&added);
            return Err(if message.is_empty() {
                format!("git exited with {}", added.status)
            } else {
                message
            });
        }
        git_text(folder, &["write-tree".as_ref()], &env)
    })();
    let _ = fs::remove_file(&index);
    let _ = fs::remove_file(index.with_extension("index.lock"));
    let tree = tree.map_err(|e| format!("cannot build a snapshot of the folder: {e}"))?;
    if tree == head_tree {
        return Ok(SavedWork {
            branch: None,
            skipped,
        });
    }
    let message = if skipped.is_empty() {
        message.to_owned()
    } else {
        format!(
            "{message}\n\nGit could not read these paths, so this commit does not hold them; they are still in the worktree:\n{}",
            listed(&skipped, SKIPPED_IN_MESSAGE, "\n")
        )
    };
    let saved = |name: String| SavedWork {
        branch: Some(name),
        skipped: skipped.clone(),
    };
    let neutral = [
        ("GIT_AUTHOR_NAME", OsStr::new(NEUTRAL_NAME)),
        ("GIT_AUTHOR_EMAIL", OsStr::new(NEUTRAL_EMAIL)),
        ("GIT_COMMITTER_NAME", OsStr::new(NEUTRAL_NAME)),
        ("GIT_COMMITTER_EMAIL", OsStr::new(NEUTRAL_EMAIL)),
    ];
    let identity: &[(&str, &OsStr)] = if identity_configured(folder) {
        &[]
    } else {
        &neutral
    };
    let mut commit: Option<String> = None;
    for number in 1_u64.. {
        let name = if number == 1 {
            branch.to_string()
        } else {
            format!("{branch}-{number}")
        };
        let reference = format!("refs/heads/{name}");
        let existing = git_text(
            folder,
            &[
                "rev-parse".as_ref(),
                "--verify".as_ref(),
                "--quiet".as_ref(),
                format!("{reference}^{{commit}}").as_ref(),
            ],
            &[],
        )
        .ok()
        .filter(|tip| !tip.is_empty());
        if let Some(tip) = existing {
            let same = git_text(
                folder,
                &[
                    "rev-parse".as_ref(),
                    format!("{tip}^{{tree}}").as_ref(),
                    format!("{tip}^").as_ref(),
                ],
                &[],
            )
            .is_ok_and(|both| both.lines().eq([tree.as_str(), head.as_str()]));
            if same {
                return Ok(saved(name));
            }
            continue;
        }
        let made = match &commit {
            Some(made) => made.clone(),
            None => {
                let made = git_text(
                    folder,
                    &[
                        "commit-tree".as_ref(),
                        "--no-gpg-sign".as_ref(),
                        tree.as_ref(),
                        "-p".as_ref(),
                        head.as_ref(),
                        "-m".as_ref(),
                        message.as_str().as_ref(),
                    ],
                    identity,
                )
                .map_err(|e| format!("cannot write the saved commit: {e}"))?;
                commit = Some(made.clone());
                made
            }
        };
        // An empty old value makes the update fail if the branch appeared in the meantime.
        match git_text(
            folder,
            &[
                "update-ref".as_ref(),
                "-m".as_ref(),
                "talaria: save uncommitted work".as_ref(),
                reference.as_ref(),
                made.as_ref(),
                "".as_ref(),
            ],
            &[],
        ) {
            Ok(_) => return Ok(saved(name)),
            Err(_)
                if git_text(
                    folder,
                    &[
                        "rev-parse".as_ref(),
                        "--verify".as_ref(),
                        "--quiet".as_ref(),
                        reference.as_ref(),
                    ],
                    &[],
                )
                .is_ok_and(|tip| !tip.is_empty()) =>
            {
                continue;
            }
            Err(error) => return Err(format!("cannot create branch {name}: {error}")),
        }
    }
    unreachable!("the branch numbers never run out")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn git(dir: &Path, args: &[&str]) -> String {
        let args: Vec<&OsStr> = args.iter().map(|a| OsStr::new(*a)).collect();
        text(dir, &args).unwrap_or_else(|e| panic!("git {args:?}: {e}"))
    }

    fn raw_git(dir: &Path, args: &[&str]) -> String {
        let output = Command::new("git")
            .arg("-C")
            .arg(dir)
            .args(args)
            .output()
            .unwrap();
        assert!(output.status.success(), "git {args:?}: {output:?}");
        String::from_utf8_lossy(&output.stdout).trim().to_owned()
    }

    /// A repository with a local identity, one commit, an ignore rule and a linked worktree.
    fn fixture() -> (tempfile::TempDir, PathBuf, PathBuf) {
        let temp = tempfile::tempdir().unwrap();
        let repo = temp.path().join("repo");
        fs::create_dir(&repo).unwrap();
        git(&repo, &["init", "-q", "-b", "main"]);
        git(&repo, &["config", "user.name", "Fixture Person"]);
        git(&repo, &["config", "user.email", "fixture@example.invalid"]);
        fs::write(repo.join("base.txt"), "base\n").unwrap();
        fs::write(repo.join(".gitignore"), "secret.env\n").unwrap();
        git(&repo, &["add", "."]);
        git(&repo, &["commit", "-q", "-m", "base"]);
        let tree = temp.path().join("tree");
        git(
            &repo,
            &[
                "worktree",
                "add",
                "-q",
                "-b",
                "agent",
                tree.to_str().unwrap(),
            ],
        );
        (temp, repo, tree)
    }

    #[test]
    fn uncommitted_work_is_saved_without_touching_the_folder() {
        let (_temp, repo, tree) = fixture();
        fs::write(tree.join("base.txt"), "edited\n").unwrap();
        fs::write(tree.join("staged.txt"), "staged\n").unwrap();
        git(&tree, &["add", "staged.txt"]);
        fs::write(tree.join("new.txt"), "new\n").unwrap();
        fs::write(tree.join("secret.env"), "TOKEN=1\n").unwrap();
        let status_before = git(&tree, &["status", "--porcelain"]);
        let head_before = git(&tree, &["rev-parse", "HEAD"]);
        let saved = save_uncommitted(&tree, "agent-saved", "saved work")
            .unwrap()
            .branch
            .unwrap();
        assert_eq!(saved, "agent-saved");
        // The folder, its branch and its index are exactly as they were.
        assert_eq!(git(&tree, &["status", "--porcelain"]), status_before);
        assert_eq!(git(&tree, &["rev-parse", "HEAD"]), head_before);
        assert_eq!(git(&tree, &["branch", "--show-current"]), "agent");
        assert_eq!(git(&tree, &["stash", "list"]), "");
        // The saved commit holds every change except the ignored file, on top of HEAD.
        let files = git(&repo, &["ls-tree", "-r", "--name-only", "agent-saved"]);
        for name in ["base.txt", "staged.txt", "new.txt"] {
            assert!(files.lines().any(|f| f == name), "{files}");
        }
        assert!(!files.contains("secret.env"), "{files}");
        assert_eq!(git(&repo, &["show", "agent-saved:base.txt"]), "edited");
        assert_eq!(git(&repo, &["rev-parse", "agent-saved^"]), head_before);
        // The repository's own identity is used.
        assert_eq!(
            git(&repo, &["log", "-1", "--format=%an <%ae>", "agent-saved"]),
            "Fixture Person <fixture@example.invalid>"
        );
        // No temporary index is left behind.
        let gitdir = PathBuf::from(git(&tree, &["rev-parse", "--absolute-git-dir"]));
        assert!(!fs::read_dir(&gitdir).unwrap().any(|e| {
            e.unwrap()
                .file_name()
                .to_string_lossy()
                .starts_with("talaria-save")
        }),);
    }

    #[test]
    fn a_clean_folder_saves_nothing_and_the_same_work_is_not_saved_twice() {
        let (_temp, repo, tree) = fixture();
        let branch = |tree: &Path| save_uncommitted(tree, "agent-saved", "m").unwrap().branch;
        assert_eq!(branch(&tree), None);
        fs::write(tree.join("secret.env"), "only ignored\n").unwrap();
        assert_eq!(branch(&tree), None);
        fs::write(tree.join("new.txt"), "one\n").unwrap();
        let first = branch(&tree);
        assert_eq!(first.as_deref(), Some("agent-saved"));
        assert_eq!(branch(&tree), first);
        // Different work later gets the next name; the first branch is untouched.
        let tip = git(&repo, &["rev-parse", "agent-saved"]);
        fs::write(tree.join("new.txt"), "two\n").unwrap();
        let second = branch(&tree);
        assert_eq!(second.as_deref(), Some("agent-saved-2"));
        assert_eq!(git(&repo, &["rev-parse", "agent-saved"]), tip);
    }

    #[test]
    fn a_neutral_identity_is_used_when_none_is_configured() {
        let (temp, repo, tree) = fixture();
        git(&repo, &["config", "--unset", "user.name"]);
        git(&repo, &["config", "--unset", "user.email"]);
        fs::write(tree.join("new.txt"), "x\n").unwrap();
        // Git reads no global or system configuration, so the machine's identity is not seen.
        let empty = temp.path().join("empty.gitconfig");
        fs::write(&empty, "").unwrap();
        let env = [
            ("GIT_CONFIG_GLOBAL", empty.as_os_str()),
            ("GIT_CONFIG_NOSYSTEM", OsStr::new("1")),
        ];
        let folder = Folder {
            path: &tree,
            env: &env,
        };
        assert!(!identity_configured(&folder));
        let saved = save_in(&folder, "agent-saved", "m")
            .unwrap()
            .branch
            .unwrap();
        assert_eq!(
            git(
                &repo,
                &["log", "-1", "--format=%an <%ae>|%cn <%ce>", &saved]
            ),
            format!("{NEUTRAL_NAME} <{NEUTRAL_EMAIL}>|{NEUTRAL_NAME} <{NEUTRAL_EMAIL}>")
        );
        // The repository's configuration was not changed to get there.
        assert!(
            text(
                &repo,
                &["config".as_ref(), "--local".as_ref(), "user.name".as_ref()]
            )
            .is_err()
        );
    }

    #[test]
    fn a_repository_hook_does_not_run_when_the_branch_is_made() {
        let (temp, repo, tree) = fixture();
        let marker = temp.path().join("hook-ran");
        let hooks = temp.path().join("hooks");
        fs::create_dir(&hooks).unwrap();
        let hook = hooks.join("reference-transaction");
        fs::write(&hook, format!("#!/bin/sh\ntouch '{}'\n", marker.display())).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&hook, fs::Permissions::from_mode(0o755)).unwrap();
        }
        git(
            &repo,
            &["config", "core.hooksPath", hooks.to_str().unwrap()],
        );
        fs::write(tree.join("new.txt"), "x\n").unwrap();
        assert!(
            save_uncommitted(&tree, "agent-saved", "m")
                .unwrap()
                .branch
                .is_some()
        );
        assert!(!marker.exists(), "a repository hook ran");
    }

    #[test]
    fn a_path_git_cannot_read_is_left_out_and_everything_else_is_saved() {
        let (_temp, repo, tree) = fixture();
        fs::write(tree.join("work.txt"), "kept\n").unwrap();
        // A nested repository with no commit cannot be indexed, whoever runs the save.
        let nested = tree.join("nested");
        fs::create_dir(&nested).unwrap();
        git(&nested, &["init", "-q"]);
        // A file nobody may read, unless the tests run with every permission.
        let locked = tree.join("locked.txt");
        fs::write(&locked, "secret\n").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&locked, fs::Permissions::from_mode(0o000)).unwrap();
        }
        let unreadable = fs::read(&locked).is_err();
        let saved = save_uncommitted(&tree, "agent-saved", "saved work").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&locked, fs::Permissions::from_mode(0o644)).unwrap();
        }
        let branch = saved.branch.clone().expect("the readable work is saved");
        assert_eq!(git(&repo, &["show", &format!("{branch}:work.txt")]), "kept");
        let files = git(&repo, &["ls-tree", "-r", "--name-only", &branch]);
        assert!(!files.contains("nested"), "{files}");
        let mut expected = vec!["nested".to_string()];
        if unreadable {
            assert!(!files.contains("locked.txt"), "{files}");
            expected.insert(0, "locked.txt".into());
        }
        assert_eq!(saved.skipped, expected);
        // The commit's message and the note both name what was left out.
        let body = git(&repo, &["log", "-1", "--format=%B", &branch]);
        assert!(
            body.starts_with("saved work\n\nGit could not read these paths"),
            "{body}"
        );
        assert!(body.lines().any(|l| l == "nested"), "{body}");
        let note = saved.skipped_note().unwrap();
        assert!(
            note.contains("not saved and are still in the worktree: ")
                || note.contains("not saved and is still in the worktree: nested"),
            "{note}"
        );
        // When nothing readable changed, no branch is made, but the skipped path is still named.
        let (_temp, _repo, tree) = fixture();
        let nested = tree.join("nested");
        fs::create_dir(&nested).unwrap();
        git(&nested, &["init", "-q"]);
        let saved = save_uncommitted(&tree, "agent-saved", "m").unwrap();
        assert_eq!(saved.branch, None);
        assert_eq!(saved.skipped, vec!["nested".to_string()]);
    }

    #[test]
    fn unreadable_paths_are_read_from_gits_messages_even_with_odd_names() {
        let stderr = "warning: could not open directory 'locked/': Permission denied\n\
            error: open(\"we'ird\nname\"): Permission denied\n\
            error: unable to index file 'we'ird\nname'\n\
            error: 'sub/' does not have a commit checked out\n\
            error: unable to index file 'sub/'\n\
            fatal: adding files failed\n";
        assert_eq!(
            unreadable_paths(stderr),
            vec!["locked", "we'ird\nname", "sub"]
        );
        assert!(unreadable_paths("").is_empty());
        let note = SavedWork {
            branch: None,
            skipped: (0..7)
                .map(|n| format!("f{n}"))
                .chain(["a\nb".into()])
                .collect(),
        }
        .skipped_note()
        .unwrap();
        assert_eq!(
            note,
            "git could not read 8 paths, which are not saved and are still in the worktree: f0, f1, f2, f3, f4, and 3 more"
        );
        assert_eq!(listed(&["a\nb".into()], 5, ", "), "a\\nb");
    }

    #[test]
    fn a_filter_the_repository_configures_does_not_run_but_the_users_own_does() {
        let (temp, repo, tree) = fixture();
        let marker = temp.path().join("filter-ran");
        let script = temp.path().join("filter.sh");
        fs::write(
            &script,
            format!("#!/bin/sh\ntouch '{}'\ncat\n", marker.display()),
        )
        .unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&script, fs::Permissions::from_mode(0o755)).unwrap();
        }
        // What an agent could write: a filter for every file, in both forms, required.
        let program = script.to_str().unwrap();
        git(&repo, &["config", "filter.agentfilter.clean", program]);
        git(&repo, &["config", "filter.agentfilter.process", program]);
        git(&repo, &["config", "filter.agentfilter.required", "true"]);
        git(&repo, &["config", "filter.spaced name.clean", program]);
        fs::write(
            tree.join(".gitattributes"),
            "* filter=agentfilter\n*.sp filter=spaced\\ name\n*.up filter=upper\n",
        )
        .unwrap();
        // A filter of the user's own configuration, as Git LFS installs one, still runs.
        let global = temp.path().join("user.gitconfig");
        fs::write(&global, "[filter \"upper\"]\n\tclean = tr a-z A-Z\n").unwrap();
        let env = [("GIT_CONFIG_GLOBAL", global.as_os_str())];
        let folder = Folder {
            path: &tree,
            env: &env,
        };
        fs::write(tree.join("new.txt"), "raw\n").unwrap();
        fs::write(tree.join("odd.sp"), "raw\n").unwrap();
        fs::write(tree.join("shout.up"), "quiet\n").unwrap();
        let saved = save_in(&folder, "agent-saved", "m").unwrap();
        assert!(!marker.exists(), "a filter the repository names ran");
        let branch = saved.branch.expect("the work is saved");
        assert!(saved.skipped.is_empty(), "{:?}", saved.skipped);
        assert_eq!(git(&repo, &["show", &format!("{branch}:new.txt")]), "raw");
        assert_eq!(git(&repo, &["show", &format!("{branch}:odd.sp")]), "raw");
        assert_eq!(
            git(&repo, &["show", &format!("{branch}:shout.up")]),
            "QUIET"
        );
        // The repository's settings are as the agent left them.
        assert_eq!(
            raw_git(&repo, &["config", "filter.agentfilter.clean"]),
            program
        );
    }

    #[test]
    fn a_required_filter_with_no_clean_command_does_not_stop_the_save() {
        let (_temp, repo, tree) = fixture();
        // Git refuses to add a file whose required filter has no clean command, even with
        // --ignore-errors, so the whole save would fail.
        git(&repo, &["config", "filter.x.required", "true"]);
        git(&repo, &["config", "filter.x.smudge", "cat"]);
        fs::write(tree.join(".gitattributes"), "* filter=x\n").unwrap();
        fs::write(tree.join("new.txt"), "raw\n").unwrap();
        let folder = Folder {
            path: &tree,
            env: &[],
        };
        let saved = save_in(&folder, "agent-saved", "m").unwrap();
        let branch = saved.branch.expect("the work is saved");
        assert!(saved.skipped.is_empty(), "{:?}", saved.skipped);
        assert_eq!(git(&repo, &["show", &format!("{branch}:new.txt")]), "raw");
    }
}
