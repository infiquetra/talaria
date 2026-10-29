//! The git a review runs. Issue #245.
//!
//! Every call goes through the launch unit's `GitRunner`, in the work tree root, with
//! `core.fsmonitor` switched off first. Diffs never run a repository's diff drivers.

use std::collections::{BTreeMap, HashMap};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use talaria_core::review::{
    BranchHead, ChangedFile, DIFF_MAX_FILE_BYTES, DRAFT_DIFF_MAX_CHARS, FileStatus, GitAction,
    Listing, REVIEW_FILE_CAP, RecordView, ReviewTarget, Scope, UNTRACKED_COUNT_MAX_BYTES,
    count_lines, merge_counts, parse_name_status_z, parse_numstat_z, parse_push_porcelain,
    parse_status_v2,
};

use crate::worktree::git::{error_line, safe_text};
use crate::worktree::{GitError, GitOutput, GitRunner, git_env, git_prefix_args};

const READ: Duration = Duration::from_secs(10);
const WRITE: Duration = Duration::from_secs(30);
const COMMIT: Duration = Duration::from_secs(120);
const PUSH: Duration = Duration::from_secs(60);
const LS_REMOTE: Duration = Duration::from_secs(30);

/// How long an `ls-remote` answer for a repository is reused.
const DEFAULT_BRANCH_TTL: Duration = Duration::from_secs(600);

/// Most untracked file names sent to the drafting model.
const DRAFT_UNTRACKED_NAMES: usize = 80;

const HOOK_NAMES: [&str; 4] = [
    "pre-commit",
    "prepare-commit-msg",
    "commit-msg",
    "post-commit",
];

const WORKTREES_FOLDER: &str = ".worktrees/";

const NO_BASE: &str = "No base to compare with: origin has no default branch here.";

/// Read a listing for a scope.
pub async fn list(git: &GitRunner, target: &ReviewTarget, scope: Scope) -> Result<Listing, String> {
    let root = work_tree_root(git, &target.folder).await?;
    let status_args: &[&str] = match scope {
        Scope::Uncommitted => &[
            "status",
            "--porcelain=v2",
            "-z",
            "--branch",
            "--untracked-files=normal",
        ],
        Scope::Branch => &[
            "status",
            "--porcelain=v2",
            "-z",
            "--branch",
            "--untracked-files=no",
        ],
    };
    let status = read(git, &root, status_args).await?;
    ensure_ok(&status)?;
    let (head, mut files) = parse_status_v2(status.stdout.as_bytes()).map_err(|e| e.0)?;

    let mut total = 0;
    let mut base = None;
    match scope {
        Scope::Uncommitted => {
            let cached = read(
                git,
                &root,
                &[
                    "diff",
                    "--cached",
                    "--numstat",
                    "-z",
                    "--no-ext-diff",
                    "--no-textconv",
                ],
            );
            let unstaged = read(
                git,
                &root,
                &["diff", "--numstat", "-z", "--no-ext-diff", "--no-textconv"],
            );
            let (cached, unstaged) = tokio::join!(cached, unstaged);
            let (cached, unstaged) = (cached?, unstaged?);
            ensure_ok(&cached)?;
            ensure_ok(&unstaged)?;
            let staged = parse_numstat_z(cached.stdout.as_bytes()).map_err(|e| e.0)?;
            let unstaged = parse_numstat_z(unstaged.stdout.as_bytes()).map_err(|e| e.0)?;
            files.sort_by(|a, b| a.path.as_bytes().cmp(b.path.as_bytes()));
            total = files.len();
            files.truncate(REVIEW_FILE_CAP);
            merge_counts(&mut files, &staged, &unstaged);
            count_untracked(&root, &mut files).await;
        }
        Scope::Branch => {
            base = branch_base(git, &root, target.record.as_ref()).await;
            files.clear();
            if let Some(base) = &base {
                files = branch_files(git, &root, base).await?;
                total = files.len();
                files.truncate(REVIEW_FILE_CAP);
            }
        }
    }

    let identities = async {
        let origin = origin_identity(git, &target.main_root).await;
        let has_origin = if root == target.main_root {
            origin.is_some()
        } else {
            origin_identity(git, &root).await.is_some()
        };
        (origin, has_origin)
    };
    let ((origin, has_origin), remote_head, hooks) =
        tokio::join!(identities, origin_head(git, &root), count_hooks(git, &root),);
    let default = default_branch_local(
        &target.main_root,
        target.record.as_ref(),
        origin,
        remote_head,
    );
    Ok(Listing {
        head,
        files,
        total,
        base,
        default_branch: default,
        has_origin,
        hooks,
    })
}

/// Whether `text` is a commit id: 7 to 64 lower-case hex digits.
fn is_object_id(text: &str) -> bool {
    (7..=64).contains(&text.len())
        && text
            .bytes()
            .all(|byte| matches!(byte, b'0'..=b'9' | b'a'..=b'f'))
}

/// The Branch scope's base: the record's commit, else the first well-known ref that shares
/// history with `HEAD`.
async fn branch_base(git: &GitRunner, root: &Path, record: Option<&RecordView>) -> Option<String> {
    // The record lives in the repository's own config, which the session's agent can write:
    // only a commit id is used, so a value like `--output=x` never reaches git as an option.
    if let Some(record) = record.filter(|record| is_object_id(&record.base_commit)) {
        return Some(record.base_commit.clone());
    }
    for candidate in [
        "origin/HEAD",
        "origin/main",
        "origin/master",
        "main",
        "master",
    ] {
        let found = read(git, root, &["merge-base", "HEAD", candidate]).await;
        if found.is_ok_and(|out| out.status == 0) {
            return Some(candidate.to_owned());
        }
    }
    None
}

/// The files committed on this branch since the base, with their counts.
async fn branch_files(
    git: &GitRunner,
    root: &Path,
    base: &str,
) -> Result<Vec<ChangedFile>, String> {
    let range = format!("{base}...HEAD");
    let name_args = [
        "diff",
        "--name-status",
        "-z",
        "--no-ext-diff",
        "--no-textconv",
        "-M",
        &range,
    ];
    let count_args = [
        "diff",
        "--numstat",
        "-z",
        "--no-ext-diff",
        "--no-textconv",
        "-M",
        &range,
    ];
    let names = read(git, root, &name_args);
    let counts = read(git, root, &count_args);
    let (names, counts) = tokio::join!(names, counts);
    let (names, counts) = (names?, counts?);
    ensure_ok(&names)?;
    ensure_ok(&counts)?;
    let rows = parse_name_status_z(names.stdout.as_bytes()).map_err(|e| e.0)?;
    let counts = parse_numstat_z(counts.stdout.as_bytes()).map_err(|e| e.0)?;
    let mut files: Vec<ChangedFile> = rows
        .into_iter()
        .map(|(status, path, orig_path)| ChangedFile {
            path,
            orig_path,
            status,
            staged: false,
            unstaged: false,
            added: None,
            removed: None,
            is_dir: false,
        })
        .collect();
    files.sort_by(|a, b| a.path.as_bytes().cmp(b.path.as_bytes()));
    merge_counts(&mut files, &counts, &BTreeMap::new());
    Ok(files)
}

/// Count the lines of untracked files of at most 1 MiB; folders and larger files stay `None`.
async fn count_untracked(root: &Path, files: &mut [ChangedFile]) {
    let wanted: Vec<(usize, PathBuf)> = files
        .iter()
        .enumerate()
        .filter(|(_, file)| file.status == FileStatus::Untracked && !file.is_dir)
        .map(|(index, file)| (index, root.join(&file.path)))
        .collect();
    let counted = tokio::task::spawn_blocking(move || {
        wanted
            .into_iter()
            .map(|(index, path)| (index, count_file(&path)))
            .collect::<Vec<_>>()
    })
    .await
    .unwrap_or_default();
    for (index, lines) in counted {
        files[index].added = lines;
        files[index].removed = lines.map(|_| 0);
    }
}

fn count_file(path: &Path) -> Option<u32> {
    let meta = std::fs::symlink_metadata(path).ok()?;
    if !meta.is_file() || meta.len() > UNTRACKED_COUNT_MAX_BYTES {
        return None;
    }
    count_lines(&std::fs::read(path).ok()?)
}

/// How many commit hooks are executable in the effective hooks folder.
async fn count_hooks(git: &GitRunner, root: &Path) -> u32 {
    let Ok(out) = read(git, root, &["rev-parse", "--git-path", "hooks"]).await else {
        return 0;
    };
    let line = out.stdout.trim_end_matches('\n');
    if out.status != 0 || line.is_empty() {
        return 0;
    }
    let folder = root.join(line);
    let active = HOOK_NAMES
        .iter()
        .filter(|name| {
            std::fs::metadata(folder.join(name))
                .is_ok_and(|meta| meta.is_file() && meta.permissions().mode() & 0o111 != 0)
        })
        .count();
    u32::try_from(active).unwrap_or(0)
}

/// Read one file's diff.
///
/// The text is the whole diff: the panel cuts it. A file too large to diff, and an
/// untracked folder, answer with the text the panel shows instead.
pub async fn file_diff(
    git: &GitRunner,
    target: &ReviewTarget,
    scope: Scope,
    base: Option<&str>,
    file: &ChangedFile,
) -> Result<String, String> {
    if file.is_dir {
        return Ok("Untracked folder: stage it to see its files.".to_owned());
    }
    let root = work_tree_root(git, &target.folder).await?;
    if let Some(size) = oversized(git, &root, scope, base, file).await {
        return Ok(format!(
            "File too large to show here ({}). Run git diff in a shell.",
            format_size(size)
        ));
    }
    let mut pathspecs = vec![literal(&file.path)];
    pathspecs.extend(file.orig_path.as_deref().map(literal));
    match scope {
        Scope::Branch => {
            let Some(base) = base.filter(|base| !base.starts_with('-')) else {
                return Err(NO_BASE.to_owned());
            };
            let range = format!("{base}...HEAD");
            let mut args = vec![
                "diff",
                "--no-ext-diff",
                "--no-textconv",
                "--no-color",
                "-M",
                &range,
                "--",
            ];
            args.extend(pathspecs.iter().map(String::as_str));
            let out = read(git, &root, &args).await?;
            ensure_ok(&out)?;
            Ok(out.stdout)
        }
        Scope::Uncommitted if file.status == FileStatus::Untracked => {
            let out = read(
                git,
                &root,
                &[
                    "diff",
                    "--no-ext-diff",
                    "--no-textconv",
                    "--no-color",
                    "--no-index",
                    "--",
                    "/dev/null",
                    &file.path,
                ],
            )
            .await?;
            if out.status > 1 || !out.stderr.trim().is_empty() {
                return Err(error_line(&out));
            }
            Ok(out.stdout)
        }
        Scope::Uncommitted => {
            if has_head(git, &root).await {
                let mut args = vec![
                    "diff",
                    "--no-ext-diff",
                    "--no-textconv",
                    "--no-color",
                    "-M",
                    "HEAD",
                    "--",
                ];
                args.extend(pathspecs.iter().map(String::as_str));
                let out = read(git, &root, &args).await?;
                ensure_ok(&out)?;
                Ok(out.stdout)
            } else {
                // No commit yet: staged changes, then unstaged ones.
                let mut text = String::new();
                for cached in [true, false] {
                    let mut args = vec!["diff", "--no-ext-diff", "--no-textconv", "--no-color"];
                    if cached {
                        args.push("--cached");
                    }
                    args.push("--");
                    args.extend(pathspecs.iter().map(String::as_str));
                    let out = read(git, &root, &args).await?;
                    ensure_ok(&out)?;
                    text.push_str(&out.stdout);
                }
                Ok(text)
            }
        }
    }
}

/// The size of a file the diff would read, when it is over the limit.
async fn oversized(
    git: &GitRunner,
    root: &Path,
    scope: Scope,
    base: Option<&str>,
    file: &ChangedFile,
) -> Option<u64> {
    let on_disk = match scope {
        Scope::Uncommitted => std::fs::symlink_metadata(root.join(&file.path))
            .ok()
            .filter(|meta| meta.is_file())
            .map(|meta| meta.len()),
        Scope::Branch => None,
    };
    let size = match on_disk {
        Some(size) => Some(size),
        None => {
            let mut sizes = vec![format!("HEAD:{}", file.path)];
            if let Some(base) = base {
                sizes.push(format!("{base}:{}", file.path));
            }
            let mut found = None;
            for object in sizes {
                let out = read(git, root, &["cat-file", "-s", &object]).await.ok()?;
                if out.status == 0 {
                    found = out.stdout.trim().parse::<u64>().ok();
                    break;
                }
            }
            found
        }
    };
    size.filter(|size| *size > DIFF_MAX_FILE_BYTES)
}

fn format_size(bytes: u64) -> String {
    #[allow(clippy::cast_precision_loss)]
    let mib = bytes as f64 / 1_048_576.0;
    format!("{mib:.1} MiB")
}

/// Run one git action. The fail-closed push and pull-request guards live here.
///
/// An empty path list in `Stage` or `Unstage` means every file.
pub async fn run_action(
    git: &GitRunner,
    target: &ReviewTarget,
    action: &GitAction,
) -> Result<String, String> {
    let root = work_tree_root(git, &target.folder).await?;
    let done = match action {
        GitAction::Stage(paths) => stage(git, &root, paths).await,
        GitAction::Unstage(paths) => unstage(git, &root, paths).await,
        GitAction::Discard(file) => discard(git, &root, file).await,
        GitAction::Commit { message, stage_all } => commit(git, &root, message, *stage_all).await,
        GitAction::Push => {
            let Some(branch) = current_branch(git, &root).await else {
                return Err(DETACHED.to_owned());
            };
            guard_ship(git, target, &root, &branch, None).await?;
            push(git, &root, &branch).await
        }
        GitAction::CreatePr { base, branch } => {
            guard_ship(git, target, &root, branch, Some(base)).await?;
            push(git, &root, branch).await
        }
    };
    done.map_err(add_lock_hint)
}

const DETACHED: &str = "HEAD is detached: check out a branch first.";

/// A lock held by the agent's own git is worth one more try.
fn add_lock_hint(text: String) -> String {
    if text.contains("index.lock") && !text.contains("Try again") {
        format!("{text} Try again in a moment.")
    } else {
        text
    }
}

fn under_worktrees(path: &str) -> bool {
    path.starts_with(WORKTREES_FOLDER) || path == WORKTREES_FOLDER.trim_end_matches('/')
}

const WORKTREES_REFUSAL: &str =
    "Talaria does not stage or discard .worktrees/: it holds other checkouts.";

async fn stage(git: &GitRunner, root: &Path, paths: &[String]) -> Result<String, String> {
    if paths.is_empty() {
        let listed = status_rows(git, root).await?;
        let count = listed
            .iter()
            .filter(|file| file.unstaged && !under_worktrees(&file.path))
            .count();
        let out = write(
            git,
            root,
            WRITE,
            &["add", "-A", "--", ":(top)", ":(top,exclude).worktrees/"],
        )
        .await?;
        ensure_ok(&out)?;
        return Ok(format!("Staged {count} files."));
    }
    if paths.iter().any(|path| under_worktrees(path)) {
        return Err(WORKTREES_REFUSAL.to_owned());
    }
    let mut args = vec!["add", "--"];
    let literals: Vec<String> = paths.iter().map(|path| literal(path)).collect();
    args.extend(literals.iter().map(String::as_str));
    let out = write(git, root, WRITE, &args).await?;
    ensure_ok(&out)?;
    Ok(match paths {
        [only] => format!("Staged {only}."),
        _ => format!("Staged {} files.", paths.len()),
    })
}

async fn unstage(git: &GitRunner, root: &Path, paths: &[String]) -> Result<String, String> {
    let unborn = !has_head(git, root).await;
    let literals: Vec<String> = paths.iter().map(|path| literal(path)).collect();
    let mut args: Vec<&str> = match (paths.is_empty(), unborn) {
        (true, false) => vec!["reset", "-q", "HEAD"],
        (true, true) => vec!["rm", "-r", "-q", "--cached", "--", "."],
        (false, false) => vec!["reset", "-q", "HEAD", "--"],
        (false, true) => vec!["rm", "-q", "--cached", "--"],
    };
    args.extend(literals.iter().map(String::as_str));
    let count = if paths.is_empty() {
        status_rows(git, root)
            .await?
            .iter()
            .filter(|file| file.staged)
            .count()
    } else {
        paths.len()
    };
    let out = write(git, root, WRITE, &args).await?;
    ensure_ok(&out)?;
    Ok(match paths {
        [only] => format!("Unstaged {only}."),
        _ => format!("Unstaged {count} files."),
    })
}

async fn discard(git: &GitRunner, root: &Path, file: &ChangedFile) -> Result<String, String> {
    if under_worktrees(&file.path) {
        return Err(WORKTREES_REFUSAL.to_owned());
    }
    let path = &file.path;
    let here = literal(path);
    match file.status {
        FileStatus::Conflicted => Err("Resolve the 1 conflicted files first.".to_owned()),
        FileStatus::Untracked => {
            let mut args = vec!["clean", "-f"];
            if file.is_dir {
                args.push("-d");
            }
            args.extend(["--", &here]);
            ensure_ok(&write(git, root, WRITE, &args).await?)?;
            Ok(format!("Deleted untracked {path}."))
        }
        FileStatus::Renamed if file.staged => {
            let orig = file.orig_path.as_deref().unwrap_or(path);
            let there = literal(orig);
            let steps: [&[&str]; 3] = [
                &["reset", "-q", "HEAD", "--", &here, &there],
                &["checkout", "HEAD", "--", &there],
                &["clean", "-f", "--", &here],
            ];
            for step in steps {
                ensure_ok(&write(git, root, WRITE, step).await?)?;
            }
            Ok(format!("Discarded the rename {orig} → {path}."))
        }
        FileStatus::Added | FileStatus::Copied if file.staged => {
            ensure_ok(&write(git, root, WRITE, &["rm", "-q", "-f", "--", &here]).await?)?;
            Ok(format!("Discarded changes to {path}."))
        }
        _ => {
            ensure_ok(&write(git, root, WRITE, &["checkout", "HEAD", "--", &here]).await?)?;
            Ok(format!("Discarded changes to {path}."))
        }
    }
}

/// The message as git will see it: trailing whitespace off every line, blank lines off both
/// ends. Git's own cleanup then applies.
fn clean_message(message: &str) -> String {
    let lines: Vec<&str> = message.lines().map(str::trim_end).collect();
    let start = lines.iter().position(|line| !line.is_empty());
    let end = lines.iter().rposition(|line| !line.is_empty());
    match (start, end) {
        (Some(start), Some(end)) => lines[start..=end].join("\n"),
        _ => String::new(),
    }
}

async fn commit(
    git: &GitRunner,
    root: &Path,
    message: &str,
    stage_all: bool,
) -> Result<String, String> {
    let message = clean_message(message);
    if message.is_empty() {
        return Err("Write a commit message first.".to_owned());
    }
    if stage_all {
        let out = write(
            git,
            root,
            WRITE,
            &["add", "-A", "--", ":(top)", ":(top,exclude).worktrees/"],
        )
        .await?;
        ensure_ok(&out)?;
    }
    let out = write(git, root, COMMIT, &["commit", "-m", &message]).await?;
    if out.status != 0 {
        return Err(format!("Commit failed: {}", failure_line(&out)));
    }
    let sha = read(git, root, &["rev-parse", "--short", "HEAD"]).await?;
    let branch = current_branch(git, root)
        .await
        .unwrap_or_else(|| "HEAD".to_owned());
    let subject = message.lines().next().unwrap_or_default();
    Ok(format!(
        "Committed {} on {branch}: {}",
        sha.stdout.trim(),
        safe_text(subject)
    ))
}

/// The refusals that come before any push or pull request, in the order the specification
/// gives them. An unknown default branch refuses too: the guard fails closed.
async fn guard_ship(
    git: &GitRunner,
    target: &ReviewTarget,
    root: &Path,
    branch: &str,
    base: Option<&str>,
) -> Result<(), String> {
    let origin = read(git, root, &["remote", "get-url", "origin"]).await?;
    if origin.status != 0 {
        return Err("This repository has no remote named origin.".to_owned());
    }
    let named_default = branch == "main" || branch == "master";
    let default = default_branch(git, &target.main_root, target.record.as_ref()).await;
    if named_default || default.as_deref() == Some(branch) {
        return Err(format!(
            "Talaria does not push or open pull requests from {branch}, the default branch."
        ));
    }
    if base == Some(branch) {
        return Err(format!(
            "{branch} is the pull request's base: Talaria does not push or open pull requests from it."
        ));
    }
    if default.is_none() {
        return Err("Talaria cannot tell origin's default branch, so it will not push.".to_owned());
    }
    Ok(())
}

/// Push the branch under its own name. Never forced.
async fn push(git: &GitRunner, root: &Path, branch: &str) -> Result<String, String> {
    let refspec = format!("refs/heads/{branch}:refs/heads/{branch}");
    let out = write(
        git,
        root,
        PUSH,
        &["push", "--porcelain", "--set-upstream", "origin", &refspec],
    )
    .await?;
    let report = parse_push_porcelain(&out.stdout);
    if report.rejected {
        let diverged = out.stdout.lines().any(|line| {
            line.starts_with('!')
                && ["fetch first", "non-fast-forward", "stale info"]
                    .iter()
                    .any(|reason| line.contains(reason))
        });
        return Err(if diverged {
            "origin has commits this branch lacks. Talaria never force-pushes: pull or rebase in a shell."
                .to_owned()
        } else {
            failure_line(&out)
        });
    }
    if out.status != 0 {
        return Err(failure_line(&out));
    }
    let up_to_date = !report.refs.is_empty() && report.refs.iter().all(|(flag, _)| *flag == '=');
    Ok(if up_to_date {
        "Already up to date on origin.".to_owned()
    } else {
        format!("Pushed {branch} to origin.")
    })
}

/// The draft prompt's inputs: the diff text and recent subjects.
///
/// The diff text starts with the names of up to 80 untracked files, then holds what will be
/// committed, and is cut to the number of characters Hermes reads.
pub async fn commit_context(
    git: &GitRunner,
    target: &ReviewTarget,
) -> Result<(String, String), String> {
    let root = work_tree_root(git, &target.folder).await?;
    let unborn = !has_head(git, &root).await;
    let rows = status_rows(git, &root).await?;
    let untracked: Vec<&str> = rows
        .iter()
        .filter(|file| file.status == FileStatus::Untracked)
        .take(DRAFT_UNTRACKED_NAMES)
        .map(|file| file.path.as_str())
        .collect();
    let staged = rows.iter().any(|file| file.staged);
    let flags = ["diff", "--no-ext-diff", "--no-textconv", "--no-color"];
    let mut args = flags.to_vec();
    if unborn || staged {
        args.push("--cached");
    } else {
        args.push("HEAD");
    }
    let diff = read(git, &root, &args).await?;
    ensure_ok(&diff)?;
    let mut text = String::new();
    if !untracked.is_empty() {
        text.push_str("Untracked files:\n");
        for name in &untracked {
            text.push_str(name);
            text.push('\n');
        }
        text.push('\n');
    }
    text.push_str(&diff.stdout);
    let text: String = text.chars().take(DRAFT_DIFF_MAX_CHARS).collect();
    let recent = if unborn {
        String::new()
    } else {
        let log = read(git, &root, &["log", "-n", "10", "--pretty=format:%s"]).await?;
        if log.status == 0 {
            log.stdout
        } else {
            String::new()
        }
    };
    Ok((text, recent))
}

/// The environment of one review git call: the launch git environment plus
/// `GIT_PAGER=cat`, `PAGER=cat`, `GIT_EDITOR=true`.
pub fn review_env(base: &BTreeMap<String, String>) -> BTreeMap<String, String> {
    let mut env = git_env(base, false);
    env.insert("GIT_PAGER".to_owned(), "cat".to_owned());
    env.insert("PAGER".to_owned(), "cat".to_owned());
    env.insert("GIT_EDITOR".to_owned(), "true".to_owned());
    env
}

struct CachedDefault {
    origin: String,
    at: Instant,
    answer: Option<String>,
    trust_head: bool,
}

type DefaultCache = Mutex<HashMap<PathBuf, CachedDefault>>;

fn default_cache() -> &'static DefaultCache {
    static CACHE: OnceLock<DefaultCache> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Forget the remembered `ls-remote` answer for a repository, so the next lookup asks again.
pub fn forget_default_branch(main_root: &Path) {
    if let Ok(mut cache) = default_cache().lock() {
        cache.remove(main_root);
    }
}

fn cached_default(main_root: &Path, origin: &str) -> Option<Option<String>> {
    let cache = default_cache().lock().ok()?;
    let entry = cache.get(main_root)?;
    (entry.origin == origin && entry.at.elapsed() < DEFAULT_BRANCH_TTL)
        .then(|| entry.answer.clone())
}

fn may_use_origin_head(main_root: &Path, origin: &str) -> bool {
    default_cache().lock().ok().is_some_and(|cache| {
        cache
            .get(main_root)
            .is_none_or(|entry| entry.origin == origin && entry.trust_head)
    })
}

fn remember_default(main_root: &Path, origin: String, answer: Option<String>, trust_head: bool) {
    if let Ok(mut cache) = default_cache().lock() {
        cache.insert(
            main_root.to_path_buf(),
            CachedDefault {
                origin,
                at: Instant::now(),
                answer,
                trust_head,
            },
        );
    }
}

async fn origin_identity(git: &GitRunner, cwd: &Path) -> Option<String> {
    let out = read(git, cwd, &["remote", "get-url", "origin"])
        .await
        .ok()?;
    let origin = out.stdout.trim();
    (out.status == 0 && !origin.is_empty()).then(|| origin.to_owned())
}

/// The default branch of `origin`: `origin/HEAD`, else `ls-remote`, else the
/// record's base ref, else unknown (`None`).
pub async fn default_branch(
    git: &GitRunner,
    main_root: &Path,
    record: Option<&RecordView>,
) -> Option<String> {
    let origin = origin_identity(git, main_root).await?;
    let trust_head = may_use_origin_head(main_root, &origin);
    if trust_head && let Some(branch) = origin_head(git, main_root).await {
        if origin_identity(git, main_root).await.as_deref() != Some(origin.as_str()) {
            return None;
        }
        remember_default(main_root, origin, Some(branch.clone()), true);
        return Some(branch);
    }
    let remote = match cached_default(main_root, &origin) {
        Some(answer) => answer,
        None => {
            let answer = ls_remote_default(git, main_root).await;
            if origin_identity(git, main_root).await.as_deref() != Some(origin.as_str()) {
                return None;
            }
            remember_default(main_root, origin, answer.clone(), trust_head);
            answer
        }
    };
    if trust_head {
        remote.or_else(|| record_default(record))
    } else {
        remote
    }
}

/// [`default_branch`] without the network step: a listing stays fast.
fn default_branch_local(
    main_root: &Path,
    record: Option<&RecordView>,
    origin: Option<String>,
    head: Option<String>,
) -> Option<String> {
    let origin = origin?;
    if may_use_origin_head(main_root, &origin)
        && let Some(branch) = head
    {
        remember_default(main_root, origin, Some(branch.clone()), true);
        return Some(branch);
    }
    let remote = cached_default(main_root, &origin).flatten();
    if may_use_origin_head(main_root, &origin) {
        remote.or_else(|| record_default(record))
    } else {
        remote
    }
}

fn record_default(record: Option<&RecordView>) -> Option<String> {
    let name = record?.base_ref.strip_prefix("origin/")?;
    usable_branch(name).then(|| name.to_owned())
}

fn usable_branch(name: &str) -> bool {
    !name.is_empty() && !name.chars().any(char::is_control)
}

async fn origin_head(git: &GitRunner, cwd: &Path) -> Option<String> {
    let out = read(
        git,
        cwd,
        &[
            "symbolic-ref",
            "--quiet",
            "--short",
            "refs/remotes/origin/HEAD",
        ],
    )
    .await
    .ok()?;
    let name = out.stdout.trim().strip_prefix("origin/")?;
    (out.status == 0 && usable_branch(name)).then(|| name.to_owned())
}

/// Ask `origin` what its `HEAD` points at. An empty repository has no answer.
async fn ls_remote_default(git: &GitRunner, cwd: &Path) -> Option<String> {
    let configured = ssh_configured(git, cwd).await;
    let prefix = git_prefix_args(configured);
    let mut args: Vec<&str> = vec!["-c", "core.fsmonitor=false"];
    args.extend(prefix.iter().map(String::as_str));
    args.extend(["ls-remote", "--symref", "origin", "HEAD"]);
    let out = git.run_read_only(cwd, &args, LS_REMOTE).await.ok()?;
    if out.status != 0 {
        return None;
    }
    out.stdout.lines().find_map(|line| {
        let name = line
            .strip_prefix("ref: refs/heads/")?
            .strip_suffix("\tHEAD")?;
        usable_branch(name).then(|| name.to_owned())
    })
}

/// Whether the operator already set an ssh command, so batch mode is not forced over it.
async fn ssh_configured(git: &GitRunner, cwd: &Path) -> bool {
    let set = |key: &str| std::env::var_os(key).is_some_and(|value| !value.is_empty());
    if set("GIT_SSH_COMMAND") || set("GIT_SSH") {
        return true;
    }
    let found = read(git, cwd, &["config", "--get", "core.sshCommand"]).await;
    found.is_ok_and(|out| out.status == 0 && !out.stdout.trim().is_empty())
}

async fn current_branch(git: &GitRunner, cwd: &Path) -> Option<String> {
    let out = read(git, cwd, &["symbolic-ref", "--quiet", "--short", "HEAD"])
        .await
        .ok()?;
    let name = out.stdout.trim();
    (out.status == 0 && !name.is_empty()).then(|| name.to_owned())
}

async fn has_head(git: &GitRunner, cwd: &Path) -> bool {
    let out = read(git, cwd, &["rev-parse", "--verify", "--quiet", "HEAD"]).await;
    out.is_ok_and(|out| out.status == 0)
}

/// The rows `git status` reports now, uncapped.
async fn status_rows(git: &GitRunner, root: &Path) -> Result<Vec<ChangedFile>, String> {
    let out = read(
        git,
        root,
        &[
            "status",
            "--porcelain=v2",
            "-z",
            "--branch",
            "--untracked-files=normal",
        ],
    )
    .await?;
    ensure_ok(&out)?;
    let (_head, files): (BranchHead, Vec<ChangedFile>) =
        parse_status_v2(out.stdout.as_bytes()).map_err(|e| e.0)?;
    Ok(files)
}

/// The work tree root holding the folder.
async fn work_tree_root(git: &GitRunner, folder: &Path) -> Result<PathBuf, String> {
    let out = read(git, folder, &["rev-parse", "--show-toplevel"]).await?;
    let line = out.stdout.trim_end_matches('\n');
    if out.status != 0 || line.is_empty() {
        return Err(format!(
            "{} is not inside a git repository.",
            folder.to_string_lossy()
        ));
    }
    Ok(PathBuf::from(line))
}

/// A path as a literal pathspec, so `*` and `?` are never patterns.
fn literal(path: &str) -> String {
    format!(":(literal){path}")
}

fn ensure_ok(out: &GitOutput) -> Result<(), String> {
    if out.status == 0 {
        Ok(())
    } else {
        Err(failure_line(out))
    }
}

/// Git's first error line. Some failures speak on standard output only.
fn failure_line(out: &GitOutput) -> String {
    if out.stderr.trim().is_empty()
        && let Some(line) = out.stdout.lines().map(str::trim).find(|l| !l.is_empty())
    {
        return safe_text(line);
    }
    error_line(out)
}

/// A read: `core.fsmonitor` off, optional locks off.
async fn read(git: &GitRunner, cwd: &Path, args: &[&str]) -> Result<GitOutput, String> {
    let mut full = vec!["-c", "core.fsmonitor=false"];
    full.extend_from_slice(args);
    git.run_read_only(cwd, &full, READ).await.map_err(describe)
}

/// A call that may write.
async fn write(
    git: &GitRunner,
    cwd: &Path,
    timeout: Duration,
    args: &[&str],
) -> Result<GitOutput, String> {
    let mut full = vec!["-c", "core.fsmonitor=false"];
    full.extend_from_slice(args);
    git.run(cwd, &full, timeout).await.map_err(describe)
}

fn describe(error: GitError) -> String {
    match error {
        GitError::NotFound => "git was not found on this machine.".to_owned(),
        GitError::TimedOut => "git took too long.".to_owned(),
        GitError::Cancelled => "git was stopped.".to_owned(),
        GitError::Io(reason) => safe_text(&reason),
    }
}
