//! Git-operation red tests for the review. Issue #245.
//!
//! Git is never stubbed: every test uses real git in a temporary folder.

#[path = "support/git_fixture.rs"]
mod git_fixture;

use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use git_fixture::{GitRepo, git_available};
use talaria_app::review::git_ops::{commit_context, default_branch, file_diff, list, run_action};
use talaria_app::worktree::GitRunner;
use talaria_core::review::{ChangedFile, FileStatus, GitAction, RecordView, ReviewTarget, Scope};

fn skip_unless_git() -> bool {
    if git_available() {
        return false;
    }
    eprintln!("git not found on PATH");
    true
}

fn review_target(repo: &Path, record: Option<RecordView>) -> ReviewTarget {
    ReviewTarget {
        folder: repo.to_owned(),
        main_root: repo.to_owned(),
        session: None,
        record,
        title: "repo".to_owned(),
    }
}

fn record(base_commit: &str) -> RecordView {
    RecordView {
        branch: "talaria/work".to_owned(),
        base_ref: "origin/main".to_owned(),
        base_commit: base_commit.to_owned(),
    }
}

fn write(repo: &Path, name: &str, text: &str) {
    std::fs::write(repo.join(name), text).expect("write file");
}

fn commit(git: &GitRepo, repo: &Path, message: &str) {
    git.ok(
        repo,
        &[
            "-c",
            "user.name=fixture",
            "-c",
            "user.email=fixture@example.invalid",
            "commit",
            "-m",
            message,
        ],
    );
}

/// A wrapper that logs every argument list, then runs git.
fn logging_git(dir: &Path) -> (PathBuf, PathBuf) {
    let log = dir.join("git-args");
    std::fs::create_dir(&log).expect("create argument log directory");
    let body = format!(
        "#!/bin/sh\nlog_dir='{}'\nlog=$(mktemp \"$log_dir/call.XXXXXXXX\") || exit 1\nfirst=1\nfor arg in \"$@\"; do\n  if [ \"$first\" = 1 ]; then\n    first=0\n  else\n    printf '\\037' >> \"$log\"\n  fi\n  printf '%s' \"$arg\" >> \"$log\"\ndone\nprintf '\\n' >> \"$log\"\nexec git \"$@\"\n",
        log.to_string_lossy().replace('\'', "'\\''")
    );
    let path = dir.join("logging-git");
    std::fs::write(&path, body).expect("wrapper script");
    let mut permissions = std::fs::metadata(&path)
        .expect("wrapper metadata")
        .permissions();
    permissions.set_mode(0o755);
    std::fs::set_permissions(&path, permissions).expect("wrapper mode");
    (path, log)
}

/// A wrapper that runs git with the fixture's home and global configuration, so a commit
/// never picks up the operator's identity, signing or hooks settings. With `identity` it
/// commits as the fixture; without, it forbids guessing one from the machine.
fn isolated_git(git: &GitRepo, identity: bool) -> PathBuf {
    let root = git.path().to_string_lossy().replace('\'', "'\\''");
    let who = if identity {
        "export GIT_AUTHOR_NAME=fixture GIT_AUTHOR_EMAIL=fixture@example.invalid\nexport GIT_COMMITTER_NAME=fixture GIT_COMMITTER_EMAIL=fixture@example.invalid\n"
    } else {
        "export GIT_CONFIG_COUNT=1 GIT_CONFIG_KEY_0=user.useConfigOnly GIT_CONFIG_VALUE_0=true\n"
    };
    let body = format!(
        "#!/bin/sh\nexport HOME='{root}/home'\nexport GIT_CONFIG_GLOBAL='{root}/gitconfig'\nexport GIT_CONFIG_NOSYSTEM=1\n{who}exec git \"$@\"\n"
    );
    let path = git.path().join(if identity {
        "isolated-git"
    } else {
        "isolated-git-anon"
    });
    std::fs::write(&path, body).expect("wrapper script");
    let mut permissions = std::fs::metadata(&path)
        .expect("wrapper metadata")
        .permissions();
    permissions.set_mode(0o755);
    std::fs::set_permissions(&path, permissions).expect("wrapper mode");
    path
}

fn read_log(log: &Path) -> Vec<Vec<String>> {
    std::fs::read_dir(log)
        .expect("read argument log directory")
        .map(|entry| {
            let text = std::fs::read_to_string(entry.expect("log entry").path())
                .expect("read argument log");
            text.strip_suffix('\n')
                .unwrap_or(&text)
                .split('\x1f')
                .map(str::to_owned)
                .collect()
        })
        .collect()
}

/// Both scopes list a recorded tree; the Branch base is the record's commit.
#[tokio::test]
async fn lists_both_scopes_for_a_recorded_tree() {
    if skip_unless_git() {
        return;
    }
    let git = GitRepo::new();
    let repo = git.repo.clone();
    let base = git.ok(&repo, &["rev-parse", "main"]);
    git.ok(&repo, &["checkout", "-q", "-b", "talaria/work"]);
    write(&repo, "work.txt", "work\n");
    git.ok(&repo, &["add", "work.txt"]);

    let runner = GitRunner::from_env();
    let target = review_target(&repo, Some(record(&base)));
    let rows = list(&runner, &target, Scope::Uncommitted)
        .await
        .expect("uncommitted lists");
    assert!(
        rows.files.iter().any(|row| row.path == "work.txt"),
        "the staged file lists"
    );

    let rows = list(&runner, &target, Scope::Branch)
        .await
        .expect("the branch lists");
    assert_eq!(rows.base.as_deref(), Some(base.as_str()));
    assert!(
        rows.files.iter().all(|row| !row.staged && !row.unstaged),
        "branch rows are neither staged nor unstaged"
    );
}

/// No attribute driver and no fsmonitor program runs, on any read.
#[tokio::test]
async fn diff_never_runs_attribute_drivers() {
    if skip_unless_git() {
        return;
    }
    let git = GitRepo::new();
    let repo = git.repo.clone();
    let mark = |name: &str| {
        let marker = git.path().join(format!("marker-{name}"));
        let script = git.path().join(format!("driver-{name}.sh"));
        std::fs::write(
            &script,
            format!("#!/bin/sh\ntouch '{}'\ncat\n", marker.to_string_lossy()),
        )
        .expect("driver script");
        let mut permissions = std::fs::metadata(&script)
            .expect("driver metadata")
            .permissions();
        permissions.set_mode(0o755);
        std::fs::set_permissions(&script, permissions).expect("driver mode");
        (script, marker)
    };
    let (textconv, textconv_mark) = mark("textconv");
    let (command, command_mark) = mark("command");
    let (fsmonitor, fsmonitor_mark) = mark("fsmonitor");
    git.ok(
        &repo,
        &[
            "config",
            "--local",
            "diff.marker.textconv",
            &textconv.to_string_lossy(),
        ],
    );
    git.ok(
        &repo,
        &[
            "config",
            "--local",
            "diff.cmddriver.command",
            &command.to_string_lossy(),
        ],
    );
    git.ok(
        &repo,
        &[
            "config",
            "--local",
            "core.fsmonitor",
            &fsmonitor.to_string_lossy(),
        ],
    );
    write(
        &repo,
        ".gitattributes",
        "*.dat diff=marker\n*.bin diff=cmddriver\n",
    );
    write(&repo, "file.dat", "data\n");
    write(&repo, "file.bin", "binary\n");

    let runner = GitRunner::from_env();
    let target = review_target(&repo, None);
    let rows = list(&runner, &target, Scope::Uncommitted)
        .await
        .expect("the listing works");
    assert!(!textconv_mark.exists(), "numstat never runs textconv");
    assert!(!fsmonitor_mark.exists(), "status never runs fsmonitor");
    let row = rows
        .files
        .iter()
        .find(|row| row.path == "file.dat")
        .expect("the data file lists");
    let _ = file_diff(&runner, &target, Scope::Uncommitted, None, row)
        .await
        .expect("the file diff works");
    assert!(!textconv_mark.exists(), "the diff never runs textconv");
    assert!(!command_mark.exists(), "nothing runs the command driver");
}

/// Stage, unstage and discard touch exactly one path.
#[tokio::test]
async fn stage_unstage_discard_touch_one_path() {
    if skip_unless_git() {
        return;
    }
    let git = GitRepo::new();
    let repo = git.repo.clone();
    write(&repo, "same.txt", "same\n");
    write(&repo, "same.txt.bak", "backup\n");
    git.ok(&repo, &["add", "same.txt", "same.txt.bak"]);
    commit(&git, &repo, "add twins");
    write(&repo, "same.txt", "changed\n");

    let runner = GitRunner::from_env();
    let target = review_target(&repo, None);
    let notice = run_action(
        &runner,
        &target,
        &GitAction::Stage(vec!["same.txt".to_owned()]),
    )
    .await
    .expect("staging works");
    assert_eq!(notice, "Staged same.txt.");
    let staged = git.ok(&repo, &["diff", "--cached", "--name-only"]);
    assert_eq!(staged, "same.txt", "the twin stays unstaged");

    let notice = run_action(
        &runner,
        &target,
        &GitAction::Unstage(vec!["same.txt".to_owned()]),
    )
    .await
    .expect("unstaging works");
    assert_eq!(notice, "Unstaged same.txt.");
    let staged = git.ok(&repo, &["diff", "--cached", "--name-only"]);
    assert!(staged.is_empty(), "nothing stays staged");

    // A folder row, a glob and a dash: discard touches one path.
    std::fs::create_dir(repo.join("folder")).expect("folder");
    write(&repo, "folder/keep.txt", "keep\n");
    write(&repo, "*", "glob\n");
    write(&repo, "-rf", "dash\n");
    for path in ["folder/", "*", "-rf"] {
        let row = ChangedFile {
            path: path.to_owned(),
            orig_path: None,
            status: FileStatus::Untracked,
            staged: false,
            unstaged: true,
            added: None,
            removed: None,
            is_dir: path.ends_with('/'),
        };
        let notice = run_action(&runner, &target, &GitAction::Discard(row))
            .await
            .expect("discard works");
        assert_eq!(notice, format!("Deleted untracked {path}."));
    }
    assert!(!repo.join("folder").exists(), "the folder is gone");
    assert!(!repo.join("*").exists(), "the glob is gone");
    assert!(!repo.join("-rf").exists(), "the dash is gone");
    assert!(repo.join("same.txt").exists(), "the twin survives");
    assert!(repo.join("same.txt.bak").exists(), "the backup survives");
}

/// A commit stages all when nothing is staged; the stored message is git's own
/// cleanup, with no trailer.
#[tokio::test]
async fn commit_stages_all_when_nothing_staged_and_keeps_message() {
    if skip_unless_git() {
        return;
    }
    let git = GitRepo::new();
    let repo = git.repo.clone();
    write(&repo, "commit.txt", "commit\n");

    let runner = GitRunner::with_program(isolated_git(&git, true));
    let target = review_target(&repo, None);
    let notice = run_action(
        &runner,
        &target,
        &GitAction::Commit {
            message: "feat: x\n\n\nbody  ".to_owned(),
            stage_all: true,
        },
    )
    .await
    .expect("the commit works");
    assert!(
        notice.starts_with("Committed ") && notice.contains("feat: x"),
        "the commit notice names the subject: {notice}"
    );
    let stored = git.ok(&repo, &["log", "-1", "--format=%B"]);
    assert_eq!(
        stored, "feat: x\n\nbody",
        "git's whitespace cleanup applies, and nothing is added"
    );
}

/// Push names the ref exactly and never forces; a diverged origin is rejected
/// and left unchanged.
#[tokio::test]
async fn push_names_ref_and_never_forces() {
    if skip_unless_git() {
        return;
    }
    let git = GitRepo::new();
    let repo = git.repo.clone();
    git.ok(&repo, &["checkout", "-q", "-b", "talaria/work"]);
    write(&repo, "push.txt", "push\n");
    git.ok(&repo, &["add", "push.txt"]);
    commit(&git, &repo, "push work");

    let (wrapper, log) = logging_git(git.path());
    let runner = GitRunner::with_program(wrapper);
    let target = review_target(&repo, None);
    let notice = run_action(&runner, &target, &GitAction::Push)
        .await
        .expect("the push works");
    assert_eq!(notice, "Pushed talaria/work to origin.");
    let calls = read_log(&log);
    let pushes: Vec<_> = calls
        .iter()
        .filter(|argv| argv.iter().any(|word| word == "push"))
        .collect();
    assert_eq!(pushes.len(), 1, "one push runs: {calls:?}");
    assert_eq!(
        pushes[0],
        &vec![
            "-c".to_owned(),
            "core.fsmonitor=false".to_owned(),
            "push".to_owned(),
            "--porcelain".to_owned(),
            "--set-upstream".to_owned(),
            "origin".to_owned(),
            "refs/heads/talaria/work:refs/heads/talaria/work".to_owned(),
        ],
        "the push names the ref exactly"
    );
    for argv in &calls {
        for word in argv.iter() {
            assert!(
                !["-f", "--force", "--force-with-lease"].contains(&word.as_str())
                    && !word.starts_with('+'),
                "never forced: {argv:?}"
            );
        }
    }

    // Diverge the origin; the push is rejected and the origin is unchanged.
    let before = git.ok(&repo, &["rev-parse", "origin/talaria/work"]);
    git.ok(git.path(), &["clone", "-q", "origin.git", "diverge"]);
    let other = git.path().join("diverge");
    git.ok(&other, &["checkout", "-q", "talaria/work"]);
    write(&other, "remote.txt", "remote\n");
    git.ok(&other, &["add", "remote.txt"]);
    commit(&git, &other, "remote change");
    git.ok(&other, &["push", "-q", "origin", "talaria/work"]);
    write(&repo, "local.txt", "local\n");
    git.ok(&repo, &["add", "local.txt"]);
    commit(&git, &repo, "local change");
    let tip = |git: &GitRepo| {
        git.ok(
            git.path(),
            &["--git-dir=origin.git", "rev-parse", "talaria/work"],
        )
    };
    let origin_before = tip(&git);
    assert_ne!(before, origin_before, "the test setup diverged");
    let failure = run_action(&runner, &target, &GitAction::Push).await;
    assert_eq!(
        failure,
        Err("origin has commits this branch lacks. Talaria never force-pushes: pull or rebase in a shell.".to_owned())
    );
    assert_eq!(
        tip(&git),
        origin_before,
        "the rejected push changes nothing"
    );
}

/// Detached, default, or unknown: no `push` argument list ever appears.
#[tokio::test]
async fn refuses_default_branch_and_detached() {
    if skip_unless_git() {
        return;
    }
    let git = GitRepo::new();
    let (wrapper, log) = logging_git(git.path());
    let runner = GitRunner::with_program(wrapper);

    // Detached.
    let repo = git.repo.clone();
    git.ok(&repo, &["checkout", "-q", "--detach"]);
    let target = review_target(&repo, None);
    assert_eq!(
        run_action(&runner, &target, &GitAction::Push).await,
        Err("HEAD is detached: check out a branch first.".to_owned())
    );

    // The default branch by name.
    git.ok(&repo, &["checkout", "-q", "main"]);
    write(&repo, "main.txt", "main\n");
    git.ok(&repo, &["add", "main.txt"]);
    commit(&git, &repo, "main change");
    assert_eq!(
        run_action(&runner, &target, &GitAction::Push).await,
        Err(
            "Talaria does not push or open pull requests from main, the default branch.".to_owned()
        )
    );

    // A default the listing never names: an empty bare repository whose default
    // is `trunk`, found by `ls-remote` at push time.
    git.ok(git.path(), &["init", "--bare", "-b", "trunk", "empty.git"]);
    git.ok(git.path(), &["clone", "-q", "empty.git", "twork"]);
    let twork = git.path().join("twork");
    write(&twork, "trunk.txt", "trunk\n");
    git.ok(&twork, &["add", "trunk.txt"]);
    commit(&git, &twork, "trunk change");
    // Git cannot name the default of a remote with no commits (`ls-remote` prints
    // nothing for an unborn `HEAD`), so the first push gives it one.
    git.ok(&twork, &["push", "-q", "origin", "trunk"]);
    let branch = git.ok(&twork, &["branch", "--show-current"]);
    assert_eq!(branch, "trunk");
    let target = review_target(&twork, None);
    assert_eq!(
        run_action(&runner, &target, &GitAction::Push).await,
        Err(
            "Talaria does not push or open pull requests from trunk, the default branch."
                .to_owned()
        )
    );

    // An origin that points at a missing folder: the default is unknown.
    git.ok(&repo, &["checkout", "-q", "-b", "feature"]);
    // The fixture's fetch made `origin/HEAD`; take it away, as in a clone of an empty
    // repository that never fetched.
    git.ok(&repo, &["remote", "set-head", "origin", "--delete"]);
    git.ok(
        &repo,
        &[
            "remote",
            "set-url",
            "origin",
            git.path().join("missing.git").to_string_lossy().as_ref(),
        ],
    );
    let target = review_target(&repo, None);
    assert_eq!(
        run_action(&runner, &target, &GitAction::Push).await,
        Err("Talaria cannot tell origin's default branch, so it will not push.".to_owned())
    );

    let calls = read_log(&log);
    assert!(
        !calls
            .iter()
            .any(|argv| argv.iter().any(|word| word == "push")),
        "no push runs in any refused case: {calls:?}"
    );
}

/// Overgrown untracked files show `+?`; unborn branches list; `.worktrees/`
/// is never committed by stage-all.
#[tokio::test]
async fn untracked_counts_unborn_and_worktrees_folder() {
    if skip_unless_git() {
        return;
    }
    let git = GitRepo::new();
    let repo = git.repo.clone();
    let big = "line\n".repeat(300_000);
    std::fs::write(repo.join("big.txt"), &big).expect("big file");
    assert!(big.len() > 1_048_576, "the file is over 1 MiB");

    let runner = GitRunner::with_program(isolated_git(&git, true));
    let target = review_target(&repo, None);
    let rows = list(&runner, &target, Scope::Uncommitted)
        .await
        .expect("the listing works");
    let row = rows
        .files
        .iter()
        .find(|row| row.path == "big.txt")
        .expect("the big file lists");
    assert_eq!(
        (row.added, row.removed),
        (None, None),
        "over 1 MiB shows +?"
    );

    // Unborn: a fresh repository with no commits lists.
    git.ok(git.path(), &["init", "-q", "-b", "main", "fresh"]);
    let fresh = git.path().join("fresh");
    write(&fresh, "first.txt", "first\n");
    let target = review_target(&fresh, None);
    let rows = list(&runner, &target, Scope::Uncommitted)
        .await
        .expect("an unborn branch lists");
    assert!(
        rows.files.iter().any(|row| row.path == "first.txt"),
        "the unborn file lists"
    );

    // `.worktrees/` holds other checkouts: stage-all leaves it out.
    let trees = repo.join(".worktrees").join("talaria-side");
    std::fs::create_dir_all(&trees).expect("trees folder");
    std::fs::write(trees.join("side.txt"), "side\n").expect("side file");
    let rows = list(&runner, &review_target(&repo, None), Scope::Uncommitted)
        .await
        .expect("the listing works");
    assert!(
        rows.files.iter().any(|row| row.path == ".worktrees/"),
        "the trees folder lists as one row"
    );
    let notice = run_action(
        &runner,
        &review_target(&repo, None),
        &GitAction::Commit {
            message: "feat: all".to_owned(),
            stage_all: true,
        },
    )
    .await
    .expect("the commit works");
    assert!(notice.starts_with("Committed "), "{notice}");
    let committed = git.ok(&repo, &["show", "--name-only", "--format="]);
    assert!(
        !committed.contains(".worktrees/"),
        "stage-all never commits the trees folder: {committed}"
    );
}

/// A staged rename discarded restores the old path; a staged new file removed.
#[tokio::test]
async fn discard_rename_and_staged_new_file() {
    if skip_unless_git() {
        return;
    }
    let git = GitRepo::new();
    let repo = git.repo.clone();
    write(&repo, "rename-old.txt", "old\n");
    write(&repo, "staged-new.txt", "new\n");
    git.ok(&repo, &["add", "rename-old.txt", "staged-new.txt"]);
    commit(&git, &repo, "add both");
    git.ok(&repo, &["mv", "rename-old.txt", "rename-new.txt"]);
    write(&repo, "extra-new.txt", "extra\n");
    git.ok(&repo, &["add", "extra-new.txt"]);

    let runner = GitRunner::from_env();
    let target = review_target(&repo, None);
    let renamed = ChangedFile {
        path: "rename-new.txt".to_owned(),
        orig_path: Some("rename-old.txt".to_owned()),
        status: FileStatus::Renamed,
        staged: true,
        unstaged: false,
        added: Some(0),
        removed: Some(0),
        is_dir: false,
    };
    let notice = run_action(&runner, &target, &GitAction::Discard(renamed))
        .await
        .expect("the rename discard works");
    assert_eq!(
        notice,
        "Discarded the rename rename-old.txt → rename-new.txt."
    );
    assert!(repo.join("rename-old.txt").exists(), "the old path is back");
    assert!(
        !repo.join("rename-new.txt").exists(),
        "the new path is gone"
    );

    let added = ChangedFile {
        path: "extra-new.txt".to_owned(),
        orig_path: None,
        status: FileStatus::Added,
        staged: true,
        unstaged: false,
        added: Some(1),
        removed: Some(0),
        is_dir: false,
    };
    let notice = run_action(&runner, &target, &GitAction::Discard(added))
        .await
        .expect("the added discard works");
    assert_eq!(notice, "Discarded changes to extra-new.txt.");
    assert!(!repo.join("extra-new.txt").exists(), "the new file is gone");
    assert!(repo.join("staged-new.txt").exists(), "the twin survives");
}

/// Git failures show git's first error line.
#[tokio::test]
async fn git_failures_show_first_line() {
    if skip_unless_git() {
        return;
    }
    let git = GitRepo::new();
    let repo = git.repo.clone();
    write(&repo, "locked.txt", "locked\n");

    let runner = GitRunner::with_program(isolated_git(&git, true));
    let target = review_target(&repo, None);

    // A stale lock file: git's line plus the retry hint.
    std::fs::write(repo.join(".git").join("index.lock"), "").expect("lock file");
    let failure = run_action(
        &runner,
        &target,
        &GitAction::Stage(vec!["locked.txt".to_owned()]),
    )
    .await;
    let text = failure.expect_err("a held lock fails");
    assert!(
        text.contains("index.lock") && text.contains("Try again in a moment."),
        "the lock line plus the hint: {text}"
    );
    std::fs::remove_file(repo.join(".git").join("index.lock")).expect("unlock");

    // A rejecting hook: its first line, and the commit does not land.
    let hook = repo.join(".git").join("hooks").join("pre-commit");
    std::fs::write(&hook, "#!/bin/sh\necho 'hook says no' >&2\nexit 1\n").expect("hook");
    let mut permissions = std::fs::metadata(&hook)
        .expect("hook metadata")
        .permissions();
    permissions.set_mode(0o755);
    std::fs::set_permissions(&hook, permissions).expect("hook mode");
    let failure = run_action(
        &runner,
        &target,
        &GitAction::Commit {
            message: "feat: hooked".to_owned(),
            stage_all: true,
        },
    )
    .await;
    assert_eq!(failure, Err("Commit failed: hook says no".to_owned()));
    let log = git.ok(&repo, &["log", "--format=%s", "-1"]);
    assert_ne!(log, "feat: hooked", "the rejected commit never lands");
    std::fs::remove_file(&hook).expect("unhook");

    // No identity anywhere: git's line, and Talaria never sets one.
    git.ok(git.path(), &["init", "-q", "-b", "main", "noident"]);
    let noident = git.path().join("noident");
    write(&noident, "who.txt", "who\n");
    let target = review_target(&noident, None);
    let runner = GitRunner::with_program(isolated_git(&git, false));
    let failure = run_action(
        &runner,
        &target,
        &GitAction::Commit {
            message: "feat: who".to_owned(),
            stage_all: true,
        },
    )
    .await;
    let text = failure.expect_err("no identity fails");
    assert!(
        text.contains("Author identity unknown"),
        "git's identity line shows: {text}"
    );
    let config = git.ok(&noident, &["config", "--local", "--list"]);
    assert!(
        !config.contains("user.name") && !config.contains("user.email"),
        "no identity is written: {config}"
    );
}

/// A pull request never starts from its own base or the default branch; otherwise the
/// push runs and the notice names the branch.
#[tokio::test]
async fn pull_request_guard_then_push() {
    if skip_unless_git() {
        return;
    }
    let git = GitRepo::new();
    let repo = git.repo.clone();
    git.ok(&repo, &["checkout", "-q", "-b", "talaria/work"]);
    write(&repo, "pr.txt", "pr\n");
    git.ok(&repo, &["add", "pr.txt"]);
    commit(&git, &repo, "pr work");

    let (wrapper, log) = logging_git(git.path());
    let runner = GitRunner::with_program(wrapper);
    let target = review_target(&repo, None);
    let same = GitAction::CreatePr {
        base: "talaria/work".to_owned(),
        branch: "talaria/work".to_owned(),
    };
    assert_eq!(
        run_action(&runner, &target, &same).await,
        Err("talaria/work is the pull request's base: Talaria does not push or open pull requests from it.".to_owned())
    );
    let on_main = GitAction::CreatePr {
        base: "develop".to_owned(),
        branch: "main".to_owned(),
    };
    assert_eq!(
        run_action(&runner, &target, &on_main).await,
        Err(
            "Talaria does not push or open pull requests from main, the default branch.".to_owned()
        )
    );
    assert!(
        !read_log(&log)
            .iter()
            .any(|argv| argv.iter().any(|word| word == "push")),
        "the refusals spawn no push"
    );
    let ok = GitAction::CreatePr {
        base: "main".to_owned(),
        branch: "talaria/work".to_owned(),
    };
    assert_eq!(
        run_action(&runner, &target, &ok).await,
        Ok("Pushed talaria/work to origin.".to_owned())
    );
    assert_eq!(
        run_action(&runner, &target, &ok).await,
        Ok("Already up to date on origin.".to_owned()),
        "a second push is up to date"
    );
}

/// Each row kind gets its own diff; odd names stay literal.
#[tokio::test]
async fn file_diffs_by_row_kind() {
    if skip_unless_git() {
        return;
    }
    let git = GitRepo::new();
    let repo = git.repo.clone();
    write(&repo, "old.txt", "line one\nline two\nline three\n");
    write(&repo, "gone.txt", "gone\n");
    write(&repo, "mod.txt", "before\n");
    git.ok(&repo, &["add", "old.txt", "gone.txt", "mod.txt"]);
    commit(&git, &repo, "base files");
    git.ok(&repo, &["mv", "old.txt", "new.txt"]);
    std::fs::remove_file(repo.join("gone.txt")).expect("remove");
    write(&repo, "mod.txt", "after\n");
    write(&repo, "*", "glob\n");
    std::fs::create_dir(repo.join("dir")).expect("dir");
    write(&repo, "dir/inner.txt", "inner\n");

    let runner = GitRunner::from_env();
    let target = review_target(&repo, None);
    let listing = list(&runner, &target, Scope::Uncommitted)
        .await
        .expect("lists");
    let diff = |name: &str| {
        let row = listing
            .files
            .iter()
            .find(|row| row.path == name)
            .unwrap_or_else(|| panic!("row {name}"))
            .clone();
        let runner = runner.clone();
        let target = target.clone();
        async move {
            file_diff(&runner, &target, Scope::Uncommitted, None, &row)
                .await
                .expect("diff")
        }
    };
    assert!(diff("mod.txt").await.contains("+after"));
    assert!(diff("gone.txt").await.contains("-gone"));
    assert!(diff("*").await.contains("+glob"), "a glob is a file name");
    assert!(
        diff("new.txt").await.contains("rename from old.txt"),
        "the rename header shows"
    );
    assert_eq!(
        diff("dir/").await,
        "Untracked folder: stage it to see its files."
    );

    // Branch scope with no base is refused with the base text.
    let row = listing.files[0].clone();
    assert_eq!(
        file_diff(&runner, &target, Scope::Branch, None, &row).await,
        Err("No base to compare with: origin has no default branch here.".to_owned())
    );
}

/// The draft context lists untracked names first, then what will be committed.
#[tokio::test]
async fn commit_context_orders_names_then_diff() {
    if skip_unless_git() {
        return;
    }
    let git = GitRepo::new();
    let repo = git.repo.clone();
    write(&repo, "tracked.txt", "one\n");
    git.ok(&repo, &["add", "tracked.txt"]);
    commit(&git, &repo, "feat: tracked");
    write(&repo, "tracked.txt", "two\n");
    write(&repo, "fresh.txt", "fresh\n");

    let runner = GitRunner::from_env();
    let target = review_target(&repo, None);
    let (diff, recent) = commit_context(&runner, &target).await.expect("context");
    assert!(
        diff.starts_with("Untracked files:\nfresh.txt\n\n"),
        "names first: {diff}"
    );
    assert!(
        diff.contains("+two"),
        "unstaged work is what will commit: {diff}"
    );
    assert!(
        recent.contains("feat: tracked"),
        "recent subjects: {recent}"
    );

    git.ok(&repo, &["add", "tracked.txt"]);
    write(&repo, "tracked.txt", "three\n");
    let (diff, _) = commit_context(&runner, &target).await.expect("context");
    assert!(
        diff.contains("+two") && !diff.contains("+three"),
        "staged work is what will commit: {diff}"
    );
}

/// Without a record the Branch base is the first well-known ref sharing history, and a
/// subfolder lists the whole tree.
#[tokio::test]
async fn branch_base_falls_back_to_origin_main_and_subfolders_list() {
    if skip_unless_git() {
        return;
    }
    let git = GitRepo::new();
    let repo = git.repo.clone();
    assert_eq!(
        git.ok(
            &repo,
            &["symbolic-ref", "--short", "refs/remotes/origin/HEAD"]
        ),
        "origin/main"
    );
    git.ok(&repo, &["checkout", "-q", "-b", "side"]);
    write(&repo, "side.txt", "side\n");
    git.ok(&repo, &["add", "side.txt"]);
    commit(&git, &repo, "side work");
    std::fs::create_dir(repo.join("sub")).expect("sub");

    let runner = GitRunner::from_env();
    let mut target = review_target(&repo, None);
    target.folder = repo.join("sub");
    let rows = list(&runner, &target, Scope::Branch).await.expect("lists");
    assert_eq!(rows.base.as_deref(), Some("origin/HEAD"));
    assert!(rows.files.iter().any(|row| row.path == "side.txt"));
    assert_eq!(rows.head.head.as_deref(), Some("side"));
    git.ok(&repo, &["remote", "set-head", "origin", "--delete"]);
    let fallback = list(&runner, &target, Scope::Branch)
        .await
        .expect("lists without origin/HEAD");
    assert_eq!(fallback.base.as_deref(), Some("origin/main"));
}

#[tokio::test]
async fn default_branch_cache_tracks_the_current_origin_including_unknown_answers() {
    if skip_unless_git() {
        return;
    }
    let git = GitRepo::new();
    let repo = &git.repo;
    let _ = git.run(repo, &["remote", "set-head", "origin", "--delete"]);
    let (wrapper, log) = logging_git(git.path());
    let runner = GitRunner::with_program(wrapper);

    assert_eq!(
        default_branch(&runner, repo, None).await.as_deref(),
        Some("main")
    );
    assert_eq!(
        default_branch(&runner, repo, None).await.as_deref(),
        Some("main")
    );
    let count_remote = || {
        read_log(&log)
            .iter()
            .filter(|args| args.iter().any(|arg| arg == "ls-remote"))
            .count()
    };
    assert_eq!(count_remote(), 1, "same-origin answers are cached");

    git.ok(git.path(), &["init", "--bare", "-b", "trunk", "trunk.git"]);
    let trunk = git.path().join("trunk.git");
    git.ok(repo, &["push", trunk.to_str().unwrap(), "main:trunk"]);
    git.ok(
        repo,
        &["remote", "set-url", "origin", trunk.to_str().unwrap()],
    );
    assert_eq!(
        default_branch(&runner, repo, None).await.as_deref(),
        Some("trunk")
    );
    assert_eq!(count_remote(), 2);

    let missing = git.path().join("missing.git");
    git.ok(
        repo,
        &["remote", "set-url", "origin", missing.to_str().unwrap()],
    );
    assert_eq!(default_branch(&runner, repo, None).await, None);
    assert_eq!(default_branch(&runner, repo, None).await, None);
    let old_record = record("old-base");
    assert_eq!(
        default_branch(&runner, repo, Some(&old_record)).await,
        None,
        "a record from the previous origin cannot authorize a push to the new origin"
    );
    git.ok(repo, &["checkout", "-q", "-b", "feature"]);
    let target = review_target(repo, Some(old_record));
    assert_eq!(
        run_action(&runner, &target, &GitAction::Push).await,
        Err("Talaria cannot tell origin's default branch, so it will not push.".into())
    );
    assert!(
        !read_log(&log)
            .iter()
            .any(|args| args.iter().any(|arg| arg == "push"))
    );
    assert_eq!(
        count_remote(),
        3,
        "unknown answers are cached for the same origin"
    );
    git.ok(
        repo,
        &["remote", "set-url", "origin", trunk.to_str().unwrap()],
    );
    assert_eq!(
        default_branch(&runner, repo, None).await.as_deref(),
        Some("trunk")
    );
    assert_eq!(
        count_remote(),
        4,
        "changing the origin invalidates a negative answer"
    );
}

#[tokio::test]
async fn changing_origin_does_not_trust_the_previous_origins_symbolic_head() {
    if skip_unless_git() {
        return;
    }
    let git = GitRepo::new();
    let repo = &git.repo;
    let runner = GitRunner::from_env();
    assert_eq!(
        default_branch(&runner, repo, None).await.as_deref(),
        Some("main")
    );
    git.ok(git.path(), &["init", "--bare", "-b", "trunk", "trunk.git"]);
    let trunk = git.path().join("trunk.git");
    git.ok(repo, &["push", trunk.to_str().unwrap(), "main:trunk"]);
    git.ok(
        repo,
        &["remote", "set-url", "origin", trunk.to_str().unwrap()],
    );
    assert_eq!(
        git.ok(
            repo,
            &["symbolic-ref", "--short", "refs/remotes/origin/HEAD"]
        ),
        "origin/main"
    );
    assert_eq!(
        default_branch(&runner, repo, None).await.as_deref(),
        Some("trunk")
    );
    let missing = git.path().join("missing.git");
    git.ok(
        repo,
        &["remote", "set-url", "origin", missing.to_str().unwrap()],
    );
    assert_eq!(default_branch(&runner, repo, None).await, None);
}

#[tokio::test]
async fn listing_shares_one_origin_lookup_and_never_contacts_the_remote() {
    if skip_unless_git() {
        return;
    }
    let git = GitRepo::new();
    let (wrapper, log) = logging_git(git.path());
    let runner = GitRunner::with_program(wrapper);
    let listing = list(&runner, &review_target(&git.repo, None), Scope::Uncommitted)
        .await
        .expect("list changes");
    assert!(listing.has_origin);
    assert_eq!(listing.default_branch.as_deref(), Some("main"));
    let calls = read_log(&log);
    assert_eq!(
        calls
            .iter()
            .filter(|args| args
                .windows(3)
                .any(|args| args == ["remote", "get-url", "origin"]))
            .count(),
        1
    );
    assert!(
        !calls
            .iter()
            .any(|args| args.iter().any(|arg| arg == "ls-remote"))
    );
}
