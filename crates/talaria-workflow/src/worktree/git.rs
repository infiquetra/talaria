use super::provenance::{Sidecar, canonical, legacy_paths, valid_commit, valid_digest};
use std::{
    ffi::{OsStr, OsString},
    fs,
    path::{Path, PathBuf},
    process::{Command, Output},
    sync::{Mutex, MutexGuard},
};

/// One provider owns a run's provenance and serialises its local Git operations.
pub struct GitWorktrees {
    run_dir: PathBuf,
    source_cwd: PathBuf,
    run_id: String,
    initial_error: Option<String>,
    state: Mutex<State>,
}

#[derive(Default)]
struct State {
    repo_root: Option<PathBuf>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WorktreeInfo {
    pub call: String,
    pub path: PathBuf,
    pub branch: String,
    pub base_commit: String,
    pub dirty: bool,
}

fn run(repo: &Path, args: &[&OsStr]) -> Result<Output, String> {
    Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(args)
        .output()
        .map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                format!("worktree isolation needs git on PATH: {e}")
            } else {
                format!("cannot run git: {e}")
            }
        })
}

fn reason(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr)
        .lines()
        .next()
        .unwrap_or("")
        .trim()
        .to_string()
}

fn checked(repo: &Path, args: &[&OsStr]) -> Result<Vec<u8>, String> {
    let output = run(repo, args)?;
    if output.status.success() {
        Ok(output.stdout)
    } else {
        let message = reason(&output);
        Err(if message.is_empty() {
            format!("git exited with {}", output.status)
        } else {
            message
        })
    }
}

fn text(repo: &Path, args: &[&OsStr]) -> Result<String, String> {
    String::from_utf8(checked(repo, args)?)
        .map(|text| text.trim().to_string())
        .map_err(|e| e.to_string())
}

fn path_output(bytes: &[u8]) -> Result<PathBuf, String> {
    // Remove Git's terminator, never whitespace belonging to the actual pathname.
    let bytes = bytes.strip_suffix(b"\n").unwrap_or(bytes);
    #[cfg(windows)]
    let bytes = bytes.strip_suffix(b"\r").unwrap_or(bytes);
    #[cfg(unix)]
    let path = {
        use std::os::unix::ffi::OsStringExt;
        PathBuf::from(OsString::from_vec(bytes.to_vec()))
    };
    #[cfg(not(unix))]
    let path = PathBuf::from(String::from_utf8(bytes.to_vec()).map_err(|e| e.to_string())?);
    canonical(&path)
}

fn present(path: &Path) -> bool {
    path.symlink_metadata().is_ok()
}

fn dirty(path: &Path) -> bool {
    checked(path, &["status".as_ref(), "--porcelain".as_ref()])
        .map(|out| !out.is_empty())
        .unwrap_or(true)
}

impl GitWorktrees {
    pub fn new(run_dir: PathBuf, source_cwd: PathBuf, run_id: String) -> Self {
        let run = canonical(&run_dir);
        let source = canonical(&source_cwd);
        Self {
            initial_error: run.as_ref().err().or(source.as_ref().err()).cloned(),
            run_dir: run.unwrap_or(run_dir),
            source_cwd: source.unwrap_or(source_cwd),
            run_id,
            state: Mutex::new(State::default()),
        }
    }

    fn lock(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn repo(&self, state: &mut State) -> Result<PathBuf, String> {
        if let Some(error) = &self.initial_error {
            return Err(error.clone());
        }
        if let Some(repo) = &state.repo_root {
            return Ok(repo.clone());
        }
        let output = run(
            &self.source_cwd,
            &["rev-parse".as_ref(), "--show-toplevel".as_ref()],
        )?;
        if !output.status.success() {
            let message = reason(&output);
            return Err(format!(
                "worktree isolation needs a git repository: '{}' is not inside one{}",
                self.source_cwd.display(),
                if message.is_empty() {
                    String::new()
                } else {
                    format!(": {message}")
                }
            ));
        }
        let repo = path_output(&output.stdout)?;
        state.repo_root = Some(repo.clone());
        Ok(repo)
    }

    fn branch(&self, call: &str) -> String {
        format!("talaria/wf/{}/{call}", self.run_id)
    }
    fn directory(&self) -> PathBuf {
        self.run_dir.join("worktrees")
    }
    fn path(&self, call: &str) -> Result<PathBuf, String> {
        canonical(&self.directory().join(call))
    }
    fn sidecar(&self, call: &str) -> PathBuf {
        self.directory().join(format!("{call}.json"))
    }

    fn valid_sidecar(&self, side: &Sidecar, file: &Path, repo: &Path) -> bool {
        valid_digest(&side.call_digest)
            && valid_commit(&side.base_commit)
            && side.branch == self.branch(&side.call())
            && file == self.sidecar(&side.call())
            && side.repo_root.is_absolute()
            && canonical(&side.repo_root).as_deref() == Ok(repo)
    }

    fn owned(&self, repo: &Path) -> Vec<(PathBuf, Sidecar)> {
        let Ok(entries) = fs::read_dir(self.directory()) else {
            return Vec::new();
        };
        let mut rows: Vec<_> = entries
            .filter_map(Result::ok)
            .filter_map(|entry| {
                let path = entry.path();
                if path.extension() != Some(OsStr::new("json")) {
                    return None;
                }
                let side = Sidecar::read(&path).ok()?;
                self.valid_sidecar(&side, &path, repo)
                    .then_some((path, side))
            })
            .collect();
        rows.sort_by_key(|(_, side)| side.call());
        rows
    }

    fn common(repo: &Path) -> Result<PathBuf, String> {
        let path = checked(
            repo,
            &[
                "rev-parse".as_ref(),
                "--path-format=absolute".as_ref(),
                "--git-common-dir".as_ref(),
            ],
        )?;
        path_output(&path)
    }

    fn owns_folder(&self, folder: &Path, repo: &Path, branch: &str) -> bool {
        folder.is_dir()
            && checked(folder, &["rev-parse".as_ref(), "--show-toplevel".as_ref()])
                .ok()
                .and_then(|root| path_output(&root).ok())
                .as_deref()
                == Some(folder)
            && Self::common(folder)
                .ok()
                .zip(Self::common(repo).ok())
                .is_some_and(|(a, b)| a == b)
            && text(folder, &["branch".as_ref(), "--show-current".as_ref()]).as_deref()
                == Ok(branch)
    }

    fn branch_exists(repo: &Path, branch: &str) -> Result<bool, String> {
        let reference = format!("refs/heads/{branch}");
        let output = run(
            repo,
            &[
                "show-ref".as_ref(),
                "--verify".as_ref(),
                "--quiet".as_ref(),
                reference.as_ref(),
            ],
        )?;
        match output.status.code() {
            Some(0) => Ok(true),
            Some(1) => Ok(false),
            _ => Err(reason(&output)),
        }
    }

    fn prune_if_registered(repo: &Path, folder: &Path) -> Result<(), String> {
        let output = run(
            repo,
            &[
                "worktree".as_ref(),
                "list".as_ref(),
                "--porcelain".as_ref(),
                "-z".as_ref(),
            ],
        )?;
        let paths: Vec<Vec<u8>> = if output.status.success() {
            output
                .stdout
                .split(|&b| b == 0)
                .filter_map(|entry| entry.strip_prefix(b"worktree "))
                .map(<[u8]>::to_vec)
                .collect()
        } else {
            // Use older line porcelain when NUL output is unavailable.
            let lines = checked(
                repo,
                &["worktree".as_ref(), "list".as_ref(), "--porcelain".as_ref()],
            )?;
            legacy_paths(&lines, folder)?
        };
        let registered = paths.iter().any(|bytes| {
            #[cfg(unix)]
            let path = {
                use std::os::unix::ffi::OsStringExt;
                PathBuf::from(OsString::from_vec(bytes.to_vec()))
            };
            #[cfg(not(unix))]
            let path = PathBuf::from(String::from_utf8_lossy(bytes).as_ref());
            canonical(&path).as_deref() == Ok(folder)
        });
        if registered {
            checked(repo, &["worktree".as_ref(), "prune".as_ref()])?;
        }
        Ok(())
    }

    /// Resolve only stable call identity; labels, prompts and launch options never name a tree.
    pub fn for_call(&self, digest: &str, occurrence: u32) -> Result<PathBuf, String> {
        if !valid_digest(digest) {
            return Err(
                "invalid call identity: digest must be 64 lower-case hexadecimal characters".into(),
            );
        }
        let mut state = self.lock();
        let repo = self.repo(&mut state)?;
        let call = format!("{digest}-{occurrence}");
        let folder = self.path(&call)?;
        let file = self.sidecar(&call);
        let branch = self.branch(&call);
        let foreign = || {
            format!(
                "worktree path exists but is not this run's worktree: {}",
                folder.display()
            )
        };
        if folder == self.source_cwd {
            return Err(foreign());
        }
        let existing = if present(&file) {
            let side = Sidecar::read(&file).map_err(|_| foreign())?;
            if !self.valid_sidecar(&side, &file, &repo) {
                return Err(foreign());
            }
            Some(side)
        } else {
            None
        };
        let recover = |side: &Sidecar| {
            side.publish(&file).map_err(|reason| {
                format!(
                    "worktree provenance cannot be recovered for {}: {reason}",
                    folder.display()
                )
            })
        };
        if present(&folder) {
            if !self.owns_folder(&folder, &repo, &branch) {
                return Err(foreign());
            }
            if existing.is_none() {
                let side = Sidecar {
                    call_digest: digest.into(),
                    occurrence,
                    branch,
                    base_commit: text(&folder, &["rev-parse".as_ref(), "HEAD".as_ref()])?,
                    repo_root: repo.clone(),
                    base_dirty: dirty(&repo),
                };
                recover(&side)?;
            }
        } else {
            let branch_exists = Self::branch_exists(&repo, &branch)?;
            if existing.is_some() && !branch_exists {
                return Err(format!(
                    "worktree for call {call} was lost: folder and branch are both missing (sidecar at {})",
                    file.display()
                ));
            }
            fs::create_dir_all(self.directory()).map_err(|e| e.to_string())?;
            let side = match existing {
                Some(side) => side,
                None => Sidecar {
                    call_digest: digest.into(),
                    occurrence,
                    branch: branch.clone(),
                    base_commit: text(
                        &repo,
                        &[
                            "rev-parse".as_ref(),
                            if branch_exists {
                                branch.as_ref()
                            } else {
                                "HEAD".as_ref()
                            },
                        ],
                    )?,
                    repo_root: repo.clone(),
                    base_dirty: dirty(&repo),
                },
            };
            let result = if branch_exists {
                Self::prune_if_registered(&repo, &folder)?;
                checked(
                    &repo,
                    &[
                        "worktree".as_ref(),
                        "add".as_ref(),
                        folder.as_os_str(),
                        branch.as_ref(),
                    ],
                )
            } else {
                side.publish(&file)?;
                checked(
                    &repo,
                    &[
                        "worktree".as_ref(),
                        "add".as_ref(),
                        "-b".as_ref(),
                        branch.as_ref(),
                        folder.as_os_str(),
                        side.base_commit.as_ref(),
                    ],
                )
            };
            if let Err(error) = result {
                if !branch_exists && !present(&folder) {
                    fs::remove_file(&file).map_err(|e| {
                        format!("cannot remove provisional provenance: {e}; git: {error}")
                    })?;
                }
                return Err(format!(
                    "cannot create worktree {}: {error}",
                    folder.display()
                ));
            }
            recover(&side)?;
        }
        let folder = canonical(&folder)?;
        if !folder.is_absolute() || !folder.is_dir() || folder == self.source_cwd {
            return Err(
                "worktree isolation did not produce an independent absolute directory".into(),
            );
        }
        Ok(folder)
    }

    pub fn list(&self) -> Vec<WorktreeInfo> {
        let mut state = self.lock();
        let Ok(repo) = self.repo(&mut state) else {
            return Vec::new();
        };
        self.owned(&repo)
            .into_iter()
            .filter_map(|(_, side)| {
                let path = self.path(&side.call()).ok()?;
                Some(WorktreeInfo {
                    call: side.call(),
                    dirty: present(&path) && dirty(&path),
                    path,
                    branch: side.branch,
                    base_commit: side.base_commit,
                })
            })
            .collect()
    }

    fn find_owned(&self, repo: &Path, input: &Path) -> Result<(PathBuf, Sidecar, PathBuf), String> {
        let path = canonical(input)?;
        self.owned(repo)
            .into_iter()
            .find_map(|(file, side)| {
                (self.path(&side.call()).as_deref() == Ok(path.as_path())).then_some((
                    file,
                    side,
                    path.clone(),
                ))
            })
            .ok_or_else(|| format!("not a workflow worktree: {}", path.display()))
    }

    pub fn remove(&self, path: &Path, force: bool) -> Result<(), String> {
        let mut state = self.lock();
        let repo = self.repo(&mut state)?;
        let (file, side, path) = self.find_owned(&repo, path)?;
        if present(&path) {
            if !force && dirty(&path) {
                return Err(format!(
                    "worktree has uncommitted changes: {} (pass force to remove anyway)",
                    path.display()
                ));
            }
            let mut args = vec![OsStr::new("worktree"), OsStr::new("remove")];
            if force {
                args.push(OsStr::new("--force"));
            }
            args.push(path.as_os_str());
            checked(&repo, &args)?;
        } else {
            Self::prune_if_registered(&repo, &path)?;
        }
        if force && Self::branch_exists(&repo, &side.branch)? {
            checked(
                &repo,
                &["branch".as_ref(), "-D".as_ref(), side.branch.as_ref()],
            )?;
        }
        fs::remove_file(file).map_err(|e| e.to_string())
    }

    /// Collect tracked changes and non-ignored new files as a binary-capable Git patch.
    pub fn diff(&self, path: &Path) -> Result<Vec<u8>, String> {
        let mut state = self.lock();
        let repo = self.repo(&mut state)?;
        let (_, side, path) = self.find_owned(&repo, path)?;
        if !self.owns_folder(&path, &repo, &side.branch) {
            return Err(format!(
                "worktree path exists but is not this run's worktree: {}",
                path.display()
            ));
        }
        let mut patch = checked(
            &path,
            &[
                "diff".as_ref(),
                "--no-ext-diff".as_ref(),
                "--no-textconv".as_ref(),
                "--binary".as_ref(),
                side.base_commit.as_ref(),
                "--".as_ref(),
            ],
        )?;
        let untracked = checked(
            &path,
            &[
                "ls-files".as_ref(),
                "--others".as_ref(),
                "--exclude-standard".as_ref(),
                "-z".as_ref(),
            ],
        )?;
        for name in untracked.split(|&b| b == 0).filter(|name| !name.is_empty()) {
            #[cfg(unix)]
            let name = {
                use std::os::unix::ffi::OsStringExt;
                OsString::from_vec(name.to_vec())
            };
            #[cfg(not(unix))]
            let name = OsString::from(String::from_utf8_lossy(name).as_ref());
            let null = if cfg!(windows) { "NUL" } else { "/dev/null" };
            let output = run(
                &path,
                &[
                    "diff".as_ref(),
                    "--no-ext-diff".as_ref(),
                    "--no-textconv".as_ref(),
                    "--no-index".as_ref(),
                    "--binary".as_ref(),
                    "--".as_ref(),
                    null.as_ref(),
                    name.as_os_str(),
                ],
            )?;
            if !matches!(output.status.code(), Some(0 | 1)) {
                return Err(reason(&output));
            }
            patch.extend(output.stdout);
        }
        Ok(patch)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(unix)]
    #[test]
    fn line_porcelain_matches_real_git_and_refuses_ambiguous_paths() {
        use std::os::unix::ffi::OsStrExt;
        let temp = tempfile::tempdir().unwrap();
        let repo = temp.path().join("repo");
        fs::create_dir(&repo).unwrap();
        checked(&repo, &["init".as_ref(), "-b".as_ref(), "main".as_ref()]).unwrap();
        checked(
            &repo,
            &[
                "-c".as_ref(),
                "user.name=Test".as_ref(),
                "-c".as_ref(),
                "user.email=test@example.invalid".as_ref(),
                "commit".as_ref(),
                "--allow-empty".as_ref(),
                "-m".as_ref(),
                "base".as_ref(),
            ],
        )
        .unwrap();
        let folder = temp.path().join("tree \"é\\");
        checked(
            &repo,
            &[
                "worktree".as_ref(),
                "add".as_ref(),
                "-b".as_ref(),
                "fixture".as_ref(),
                folder.as_os_str(),
            ],
        )
        .unwrap();
        let output = checked(
            &repo,
            &[
                "-c".as_ref(),
                "core.quotePath=true".as_ref(),
                "worktree".as_ref(),
                "list".as_ref(),
                "--porcelain".as_ref(),
            ],
        )
        .unwrap();
        let canonical = folder.canonicalize().unwrap();
        assert!(
            legacy_paths(&output, &canonical)
                .unwrap()
                .iter()
                .any(|path| path == canonical.as_os_str().as_bytes())
        );
        let ambiguous = temp.path().join("tree\nwith a newline");
        assert!(legacy_paths(&output, &ambiguous).is_err());
    }

    #[test]
    fn creation_recovers_after_the_mutex_is_actually_poisoned() {
        let temp = tempfile::tempdir().unwrap();
        let repo = temp.path().join("repo");
        fs::create_dir(&repo).unwrap();
        checked(&repo, &["init".as_ref(), "-b".as_ref(), "main".as_ref()]).unwrap();
        checked(
            &repo,
            &[
                "-c".as_ref(),
                "user.name=Test".as_ref(),
                "-c".as_ref(),
                "user.email=test@example.invalid".as_ref(),
                "commit".as_ref(),
                "--allow-empty".as_ref(),
                "-m".as_ref(),
                "base".as_ref(),
            ],
        )
        .unwrap();
        let provider = GitWorktrees::new(temp.path().join("run"), repo, "run1".into());
        let panic = std::panic::catch_unwind(|| {
            let _guard = provider.lock();
            panic!("simulate setup panic");
        });
        assert!(panic.is_err());
        assert!(provider.state.is_poisoned());
        assert!(provider.for_call(&"a".repeat(64), 0).unwrap().is_dir());
        assert_eq!(provider.list().len(), 1);
    }
}
