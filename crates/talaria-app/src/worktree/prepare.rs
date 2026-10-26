//! Create a worktree cut from a freshly fetched `origin`, and describe it in git config.

use std::collections::BTreeSet;
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::bg::outcome::LaunchError;
use crate::bg::request::{WorktreeNames, pick_names};
use crate::bg::service::CancelToken;
use crate::worktree::git::{
    GitError, GitOutput, GitRunner, error_line, interrupted_error, missing_error, run_failed,
    safe_text,
};
use crate::worktree::include::apply_include;
use crate::worktree::record::{WorktreeRecord, tree_is_contained, write_record};

const LOCAL: Duration = Duration::from_secs(10);
const NETWORK: Duration = Duration::from_secs(30);
const ADD: Duration = Duration::from_secs(120);

/// The line added to the repository's local exclude list.
const EXCLUDE_LINE: &str = "/.worktrees/";

const FIX_AND_RETRY: &str = "Fix what git reports, then try again; nothing was started.";

/// Git calls for one preparation, stopped early when the launch is cancelled.
struct Calls<'a> {
    git: &'a GitRunner,
    cancel: Option<&'a CancelToken>,
}

impl Calls<'_> {
    async fn read(
        &self,
        cwd: &Path,
        args: &[&str],
        timeout: Duration,
    ) -> Result<GitOutput, GitError> {
        match self.cancel {
            Some(cancel) => {
                self.git
                    .run_read_only_until(cwd, args, timeout, cancel)
                    .await
            }
            None => self.git.run_read_only(cwd, args, timeout).await,
        }
    }

    async fn write(
        &self,
        cwd: &Path,
        args: &[&str],
        timeout: Duration,
    ) -> Result<GitOutput, GitError> {
        match self.cancel {
            Some(cancel) => self.git.run_until(cwd, args, timeout, cancel).await,
            None => self.git.run(cwd, args, timeout).await,
        }
    }

    fn ensure_running(&self) -> Result<(), LaunchError> {
        if self.cancel.is_some_and(CancelToken::is_cancelled) {
            Err(interrupted_error())
        } else {
            Ok(())
        }
    }
}

/// Create the worktree, copy the include list, and write the record.
///
/// A refusal before the tree exists writes nothing in the repository beyond what a fetch
/// writes. A cancelled or failed `worktree add` removes whatever it started.
pub async fn prepare_worktree(
    git: &GitRunner,
    folder: &Path,
    slug: &str,
    profile: &str,
    now_unix: i64,
    cancel: &CancelToken,
) -> Result<(WorktreeRecord, Vec<String>), LaunchError> {
    let calls = Calls {
        git,
        cancel: Some(cancel),
    };
    calls.ensure_running()?;
    let main_root = find_repository(&calls, folder).await?;
    require_origin(&calls, &main_root).await?;
    calls.ensure_running()?;
    let default_branch = default_branch(&calls, &main_root).await?;
    let (base_commit, base_fetched, mut warnings) =
        fresh_base(&calls, &main_root, &default_branch).await?;

    calls.ensure_running()?;
    refuse_linked_trees_folder(&main_root)?;
    let names = choose_names(&calls, &main_root, slug).await?;
    ensure_excluded(&calls, &main_root, &names.dir_name).await?;
    let trees = main_root.join(".worktrees");
    std::fs::create_dir_all(&trees).map_err(|error| LaunchError {
        what: format!(
            "could not create the .worktrees folder: {}.",
            safe_text(&error.to_string())
        ),
        next_step: FIX_AND_RETRY.to_owned(),
    })?;
    let (names, path) = add_worktree(&calls, &main_root, slug, names, &base_commit).await?;
    let landed = WorktreeRecord {
        main_root: main_root.clone(),
        path: path.clone(),
        branch: names.branch.clone(),
        base_ref: String::new(),
        base_commit: base_commit.clone(),
        base_fetched: false,
        profile: String::new(),
        session_id: None,
        created_at: 0,
    };
    if !tree_is_contained(&landed) {
        clean_partial(git, &landed).await;
        return Err(add_failed(
            "the new tree is not inside the repository's .worktrees folder",
        ));
    }

    let record = WorktreeRecord {
        main_root,
        path,
        branch: names.branch,
        base_ref: format!("origin/{default_branch}"),
        base_commit,
        base_fetched,
        profile: profile.to_owned(),
        session_id: None,
        created_at: now_unix,
    };
    if let Err(error) = write_record(git, &record).await {
        clean_partial(git, &record).await;
        return Err(error);
    }
    warnings.extend(apply_include(git, &record.main_root, &record.path).await);
    Ok((record, warnings))
}

/// The main worktree that contains `folder`.
pub async fn main_root_of(git: &GitRunner, folder: &Path) -> Result<PathBuf, LaunchError> {
    let calls = Calls { git, cancel: None };
    find_repository(&calls, folder).await
}

fn not_inside(folder: &Path) -> LaunchError {
    LaunchError {
        what: format!("{} is not inside a git repository.", folder.display()),
        next_step: "Run talaria --bg without --worktree, or start it from inside a repository."
            .to_owned(),
    }
}

fn git_refused(what: &str, output: &GitOutput) -> LaunchError {
    LaunchError {
        what: format!("{what}: {}.", error_line(output)),
        next_step: FIX_AND_RETRY.to_owned(),
    }
}

/// Refuse a `.worktrees` that is a symbolic link: trees would be made wherever it points.
fn refuse_linked_trees_folder(main_root: &Path) -> Result<(), LaunchError> {
    match std::fs::symlink_metadata(main_root.join(".worktrees")) {
        Ok(meta) if meta.file_type().is_symlink() => {
            Err(add_failed(".worktrees is a symbolic link"))
        }
        _ => Ok(()),
    }
}

/// Section "Finding the repository": the main worktree holding `folder`.
async fn find_repository(calls: &Calls<'_>, folder: &Path) -> Result<PathBuf, LaunchError> {
    let inside = calls
        .read(folder, &["rev-parse", "--is-inside-work-tree"], LOCAL)
        .await
        .map_err(|error| run_failed(error, "inspect the folder"))?;
    if inside.status != 0 {
        return Err(not_inside(folder));
    }
    let listing = calls
        .read(folder, &["worktree", "list", "--porcelain"], LOCAL)
        .await
        .map_err(|error| run_failed(error, "list the worktrees"))?;
    if listing.status != 0 {
        return Err(git_refused("git could not list the worktrees", &listing));
    }
    let first: Vec<&str> = listing
        .stdout
        .lines()
        .take_while(|line| !line.trim().is_empty())
        .collect();
    // Inside a bare repository the first check prints `false` with a zero status, so this
    // comes before that answer is judged.
    if first.contains(&"bare") {
        return Err(LaunchError {
            what: format!(
                "{} belongs to a bare repository, which has no working tree to branch from.",
                folder.display()
            ),
            next_step: "Start it from a normal checkout.".to_owned(),
        });
    }
    if inside.stdout.trim() != "true" {
        return Err(not_inside(folder));
    }
    first
        .iter()
        .find_map(|line| line.strip_prefix("worktree "))
        .map(PathBuf::from)
        .ok_or_else(|| not_inside(folder))
}

async fn require_origin(calls: &Calls<'_>, main_root: &Path) -> Result<(), LaunchError> {
    let origin = calls
        .read(main_root, &["remote", "get-url", "origin"], LOCAL)
        .await
        .map_err(|error| run_failed(error, "look up the origin remote"))?;
    if origin.status == 0 {
        Ok(())
    } else {
        Err(LaunchError {
            what: "this repository has no remote named origin.".to_owned(),
            next_step: "Add one with git remote add origin <url>, or run without --worktree."
                .to_owned(),
        })
    }
}

/// The default branch of `origin`: its local `HEAD` pointer, else what the remote reports.
///
/// The name comes from the remote, so it must be a valid branch name without control
/// characters before it is used anywhere.
async fn default_branch(calls: &Calls<'_>, main_root: &Path) -> Result<String, LaunchError> {
    let local = calls
        .read(
            main_root,
            &[
                "symbolic-ref",
                "--quiet",
                "--short",
                "refs/remotes/origin/HEAD",
            ],
            LOCAL,
        )
        .await
        .map_err(|error| run_failed(error, "read the default branch"))?;
    if local.status == 0 {
        let named = local.stdout.trim().strip_prefix("origin/");
        if let Some(branch) = named.filter(|branch| !branch.is_empty())
            && usable_branch(calls, main_root, branch).await?
        {
            return Ok(branch.to_owned());
        }
    }
    let remote = calls
        .read(
            main_root,
            &["ls-remote", "--symref", "origin", "HEAD"],
            NETWORK,
        )
        .await
        .map_err(|error| run_failed(error, "ask origin for its default branch"))?;
    let found = if remote.status == 0 {
        remote.stdout.lines().find_map(|line| {
            let (target, name) = line.split_once('\t')?;
            let branch = target.strip_prefix("ref: refs/heads/")?;
            (name.trim() == "HEAD" && !branch.is_empty()).then(|| branch.to_owned())
        })
    } else {
        None
    };
    if let Some(branch) = found
        && usable_branch(calls, main_root, &branch).await?
    {
        return Ok(branch);
    }
    Err(LaunchError {
        what: "could not find the default branch of origin.".to_owned(),
        next_step: "Run git remote set-head origin --auto, then try again.".to_owned(),
    })
}

/// Whether a branch name that origin reported is safe to use: no control characters, no
/// leading dash, and accepted by git's own ref-name rules.
async fn usable_branch(
    calls: &Calls<'_>,
    main_root: &Path,
    branch: &str,
) -> Result<bool, LaunchError> {
    if branch.starts_with('-') || branch.chars().any(char::is_control) {
        return Ok(false);
    }
    let reference = format!("refs/heads/{branch}");
    let checked = calls
        .read(main_root, &["check-ref-format", &reference], LOCAL)
        .await
        .map_err(|error| run_failed(error, "check the default branch name"))?;
    Ok(checked.status == 0)
}

/// Fetch the default branch and resolve the commit to branch from.
///
/// A failed fetch falls back to the copy fetched earlier, with a warning.
async fn fresh_base(
    calls: &Calls<'_>,
    main_root: &Path,
    branch: &str,
) -> Result<(String, bool, Vec<String>), LaunchError> {
    let shown = safe_text(branch);
    let refspec = format!("+refs/heads/{branch}:refs/remotes/origin/{branch}");
    let fetched = calls
        .write(
            main_root,
            &["fetch", "--no-tags", "--quiet", "origin", &refspec],
            NETWORK,
        )
        .await;
    let failure = match fetched {
        Ok(output) if output.status == 0 => None,
        Ok(output) => Some(error_line(&output)),
        Err(GitError::TimedOut) => Some("timed out after 30 seconds".to_owned()),
        Err(GitError::Io(reason)) => Some(safe_text(&reason)),
        Err(error) => return Err(run_failed(error, "fetch origin")),
    };
    let remote_ref = format!("refs/remotes/origin/{branch}^{{commit}}");
    let cached = calls
        .read(
            main_root,
            &["rev-parse", "--verify", "--quiet", &remote_ref],
            LOCAL,
        )
        .await
        .map_err(|error| run_failed(error, "read the origin branch"))?;
    let commit = cached.stdout.trim().to_owned();
    let have_commit = cached.status == 0 && !commit.is_empty();
    match (failure, have_commit) {
        (None, true) => Ok((commit, true, Vec::new())),
        (Some(reason), true) => {
            let short: String = commit.chars().take(12).collect();
            let warning = format!(
                "could not fetch origin/{shown} ({reason}); branched from the copy fetched earlier at {short}."
            );
            Ok((commit, false, vec![warning]))
        }
        (Some(reason), false) => Err(LaunchError {
            what: format!(
                "could not fetch origin/{shown} ({reason}), and there is no earlier copy to branch from."
            ),
            next_step: "Check the network and your access to origin, then try again.".to_owned(),
        }),
        (None, false) => Err(LaunchError {
            what: format!("git could not read origin/{shown} after fetching it."),
            next_step: FIX_AND_RETRY.to_owned(),
        }),
    }
}

/// The folder names under `.worktrees/` and the branch names already in use.
async fn taken_names(
    calls: &Calls<'_>,
    main_root: &Path,
) -> Result<(BTreeSet<String>, BTreeSet<String>), LaunchError> {
    let dirs: BTreeSet<String> = std::fs::read_dir(main_root.join(".worktrees"))
        .map(|entries| {
            entries
                .flatten()
                .map(|entry| entry.file_name().to_string_lossy().into_owned())
                .collect()
        })
        .unwrap_or_default();
    let mut branches = BTreeSet::new();
    for (prefix, strip) in [
        ("refs/heads/talaria/", ""),
        ("refs/remotes/origin/talaria/", "origin/"),
    ] {
        let listed = calls
            .read(
                main_root,
                &["for-each-ref", "--format=%(refname:short)", prefix],
                LOCAL,
            )
            .await
            .map_err(|error| run_failed(error, "list the existing branches"))?;
        if listed.status != 0 {
            return Err(git_refused("git could not list the branches", &listed));
        }
        for line in listed.stdout.lines().map(str::trim) {
            let name = line.strip_prefix(strip).unwrap_or(line);
            if !name.is_empty() {
                branches.insert(name.to_owned());
            }
        }
    }
    Ok((dirs, branches))
}

async fn choose_names(
    calls: &Calls<'_>,
    main_root: &Path,
    slug: &str,
) -> Result<WorktreeNames, LaunchError> {
    let (dirs, branches) = taken_names(calls, main_root).await?;
    pick_names(slug, &dirs, &branches).ok_or_else(|| LaunchError {
        what: {
            let slug = safe_text(slug);
            format!("the names talaria-{slug} to talaria-{slug}-99 are all taken.")
        },
        next_step: "Pick another name with --name.".to_owned(),
    })
}

/// Make sure git ignores `.worktrees/` without touching any tracked file.
async fn ensure_excluded(
    calls: &Calls<'_>,
    main_root: &Path,
    dir_name: &str,
) -> Result<(), LaunchError> {
    let probe = format!(".worktrees/{dir_name}");
    let checked = calls
        .read(main_root, &["check-ignore", "-q", "--", &probe], LOCAL)
        .await
        .map_err(|error| run_failed(error, "check whether .worktrees/ is ignored"))?;
    match checked.status {
        0 => return Ok(()),
        1 => {}
        _ => {
            return Err(git_refused(
                "git could not check whether .worktrees/ is ignored",
                &checked,
            ));
        }
    }
    let common = calls
        .read(main_root, &["rev-parse", "--git-common-dir"], LOCAL)
        .await
        .map_err(|error| run_failed(error, "find the repository folder"))?;
    if common.status != 0 {
        return Err(git_refused(
            "git could not find the repository folder",
            &common,
        ));
    }
    let exclude = main_root
        .join(common.stdout.trim())
        .join("info")
        .join("exclude");
    append_exclude(&exclude).map_err(|error| LaunchError {
        what: format!(
            "could not add .worktrees/ to the repository's local exclude list: {}.",
            safe_text(&error.to_string())
        ),
        next_step: FIX_AND_RETRY.to_owned(),
    })
}

fn append_exclude(exclude: &Path) -> std::io::Result<()> {
    if let Some(parent) = exclude.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let existing = std::fs::read(exclude).unwrap_or_default();
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(exclude)?;
    if !existing.is_empty() && !existing.ends_with(b"\n") {
        file.write_all(b"\n")?;
    }
    file.write_all(EXCLUDE_LINE.as_bytes())?;
    file.write_all(b"\n")
}

fn add_failed(reason: &str) -> LaunchError {
    LaunchError {
        what: format!("git could not create the worktree: {reason}."),
        next_step: FIX_AND_RETRY.to_owned(),
    }
}

/// Run `git worktree add`. A name taken by another launch since the names were listed is
/// picked again, once. Any other failure removes what the call started.
async fn add_worktree(
    calls: &Calls<'_>,
    main_root: &Path,
    slug: &str,
    mut names: WorktreeNames,
    base_commit: &str,
) -> Result<(WorktreeNames, PathBuf), LaunchError> {
    let mut retried = false;
    loop {
        calls.ensure_running()?;
        let path = main_root.join(".worktrees").join(&names.dir_name);
        let target = path.to_string_lossy().into_owned();
        let added = calls
            .write(
                main_root,
                &[
                    "worktree",
                    "add",
                    "--no-track",
                    "-b",
                    &names.branch,
                    &target,
                    base_commit,
                ],
                ADD,
            )
            .await;
        let partial = WorktreeRecord {
            main_root: main_root.to_path_buf(),
            path: path.clone(),
            branch: names.branch.clone(),
            base_ref: String::new(),
            base_commit: base_commit.to_owned(),
            base_fetched: false,
            profile: String::new(),
            session_id: None,
            created_at: 0,
        };
        match added {
            Ok(output) if output.status == 0 => return Ok((names, path)),
            Ok(output) if output.stderr.contains("already exists") => {
                // Git checks both names before it writes anything, so nothing here is ours.
                if retried {
                    return Err(add_failed(&error_line(&output)));
                }
                retried = true;
                names = choose_names(calls, main_root, slug).await?;
            }
            Ok(output) => {
                clean_partial(calls.git, &partial).await;
                return Err(add_failed(&error_line(&output)));
            }
            Err(GitError::NotFound) => return Err(missing_error()),
            Err(GitError::Cancelled) => {
                clean_partial(calls.git, &partial).await;
                return Err(interrupted_error());
            }
            Err(GitError::TimedOut) => {
                clean_partial(calls.git, &partial).await;
                return Err(add_failed("timed out after 120 seconds"));
            }
            Err(GitError::Io(reason)) => {
                clean_partial(calls.git, &partial).await;
                return Err(add_failed(&safe_text(&reason)));
            }
        }
    }
}

/// Remove the folder, registration and branch of a tree this launch started.
///
/// Errors are ignored: this runs on a path that is already failing. The branch is deleted
/// only while it still points at the base commit, so a branch of someone else survives.
/// The `/.worktrees/` exclude line and the empty `.worktrees/` folder stay on purpose.
async fn clean_partial(git: &GitRunner, record: &WorktreeRecord) {
    let root = record.main_root.as_path();
    let path = record.path.to_string_lossy().into_owned();
    let _ = git
        .run(root, &["worktree", "remove", "--force", &path], NETWORK)
        .await;
    if tree_is_contained(record) && record.path.symlink_metadata().is_ok() {
        let _ = std::fs::remove_dir_all(&record.path);
    }
    let _ = git.run(root, &["worktree", "prune"], LOCAL).await;
    let reference = format!("refs/heads/{}", record.branch);
    let tip = git
        .run_read_only(
            root,
            &["rev-parse", "--verify", "--quiet", &reference],
            LOCAL,
        )
        .await;
    if matches!(tip, Ok(ref output) if output.status == 0 && output.stdout.trim() == record.base_commit)
    {
        let _ = git
            .run(root, &["branch", "-D", &record.branch], LOCAL)
            .await;
    }
}
