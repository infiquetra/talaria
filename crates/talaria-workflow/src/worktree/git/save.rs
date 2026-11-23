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

/// Options for every command here, beyond those `run` always sets: no hook runs (a reference
/// update would run `reference-transaction`) and no signing program is started.
fn hardened<'a>(args: &[&'a OsStr]) -> Vec<&'a OsStr> {
    let mut all: Vec<&OsStr> = vec![
        "-c".as_ref(),
        if cfg!(windows) {
            "core.hooksPath=NUL".as_ref()
        } else {
            "core.hooksPath=/dev/null".as_ref()
        },
        "-c".as_ref(),
        "commit.gpgSign=false".as_ref(),
    ];
    all.extend_from_slice(args);
    all
}

/// The folder every command runs in, and environment every command gets (empty outside tests).
struct Folder<'a> {
    path: &'a Path,
    env: &'a [(&'a str, &'a OsStr)],
}

fn git_text(folder: &Folder, args: &[&OsStr], env: &[(&str, &OsStr)]) -> Result<String, String> {
    let mut all = folder.env.to_vec();
    all.extend_from_slice(env);
    let output = run_with(folder.path, &hardened(args), &all)?;
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

/// Save `folder`'s uncommitted changes, tracked and untracked (ignored files stay out), as a
/// commit on a new branch named `branch` (or `branch-2`, `branch-3` and so on when that name is
/// taken by other work). Returns the branch name, or `None` when there is nothing to save.
/// When a branch of that series already holds exactly this work on this HEAD, its name is
/// returned and nothing new is made.
pub fn save_uncommitted(
    folder: &Path,
    branch: &str,
    message: &str,
) -> Result<Option<String>, String> {
    save_in(
        &Folder {
            path: folder,
            env: &[],
        },
        branch,
        message,
    )
}

fn save_in(folder: &Folder, branch: &str, message: &str) -> Result<Option<String>, String> {
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
    let tree = (|| {
        let env = [("GIT_INDEX_FILE", index.as_os_str())];
        git_text(folder, &["read-tree".as_ref(), head.as_ref()], &env)?;
        git_text(
            folder,
            &[
                "add".as_ref(),
                "--all".as_ref(),
                "--ignore-errors".as_ref(),
                "--".as_ref(),
                ":/".as_ref(),
            ],
            &env,
        )?;
        git_text(folder, &["write-tree".as_ref()], &env)
    })();
    let _ = fs::remove_file(&index);
    let _ = fs::remove_file(index.with_extension("index.lock"));
    let tree = tree.map_err(|e| format!("cannot build a snapshot of the folder: {e}"))?;
    if tree == head_tree {
        return Ok(None);
    }
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
                return Ok(Some(name));
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
                        message.as_ref(),
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
            Ok(_) => return Ok(Some(name)),
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
        assert_eq!(save_uncommitted(&tree, "agent-saved", "m").unwrap(), None);
        fs::write(tree.join("secret.env"), "only ignored\n").unwrap();
        assert_eq!(save_uncommitted(&tree, "agent-saved", "m").unwrap(), None);
        fs::write(tree.join("new.txt"), "one\n").unwrap();
        let first = save_uncommitted(&tree, "agent-saved", "m").unwrap();
        assert_eq!(first.as_deref(), Some("agent-saved"));
        assert_eq!(save_uncommitted(&tree, "agent-saved", "m").unwrap(), first);
        // Different work later gets the next name; the first branch is untouched.
        let tip = git(&repo, &["rev-parse", "agent-saved"]);
        fs::write(tree.join("new.txt"), "two\n").unwrap();
        let second = save_uncommitted(&tree, "agent-saved", "m").unwrap();
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
        let saved = save_in(&folder, "agent-saved", "m").unwrap().unwrap();
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
                .is_some()
        );
        assert!(!marker.exists(), "a repository hook ran");
    }
}
