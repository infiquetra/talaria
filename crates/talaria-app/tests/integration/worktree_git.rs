//! Git worktrees. Each test uses its own temporary repository and expects the real outcome.
//! The stub runner reports git missing, so the outcome assertions fail.

use crate::git_fixture;

use std::collections::BTreeMap;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::time::Duration;

use git_fixture::GitRepo;
use talaria_app::bg::{CancelToken, LaunchError};
use talaria_app::worktree::git::{git_env, git_prefix_args};
use talaria_app::worktree::include::apply_include;
use talaria_app::worktree::{
    GitRunner, WorktreeRecord, main_root_of, prepare_worktree, read_record, record_session,
    records_in, rollback,
};

fn block<F: std::future::Future>(future: F) -> F::Output {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("runtime");
    runtime.block_on(future)
}

fn scrub(text: &str) -> String {
    text.split_whitespace()
        .map(|word| {
            if word.contains('/') || word.contains('\\') {
                "<path>"
            } else {
                word
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn prepare(
    _repo: &GitRepo,
    folder: &Path,
    slug: &str,
) -> Result<(WorktreeRecord, Vec<String>), LaunchError> {
    let git = GitRunner::from_env();
    block(prepare_worktree(
        &git,
        folder,
        slug,
        "fixture",
        1_700_000_000,
        &CancelToken::new(),
    ))
}

fn require_tree(repo: &GitRepo, folder: &Path, slug: &str) -> (WorktreeRecord, Vec<String>) {
    match prepare(repo, folder, slug) {
        Ok(pair) => pair,
        Err(error) => panic!("no tree and no record: {} {}", error.what, error.next_step),
    }
}

fn require_refusal(repo: &GitRepo, folder: &Path) -> LaunchError {
    match prepare(repo, folder, "fix") {
        Ok(_) => panic!("a refusal case created a tree"),
        Err(error) => error,
    }
}

fn same(actual: &str, expected: &str) {
    assert_eq!(scrub(actual), scrub(expected), "refusal text");
}

#[test]
fn git_env_never_prompts() {
    git_fixture::require_git();
    let mut base = BTreeMap::new();
    base.insert("PATH".to_owned(), "place".to_owned());
    let env = git_env(&base, true);
    assert_eq!(
        env.get("GIT_TERMINAL_PROMPT").map(String::as_str),
        Some("0")
    );
    assert_eq!(
        env.get("GCM_INTERACTIVE").map(String::as_str),
        Some("Never")
    );
    assert_eq!(env.get("GIT_OPTIONAL_LOCKS").map(String::as_str), Some("0"));
    assert_eq!(env.get("PATH").map(String::as_str), Some("place"));
    let writing = git_env(&base, false);
    assert!(!writing.contains_key("GIT_OPTIONAL_LOCKS"));
    let prefix = git_prefix_args(false);
    let batched = prefix.iter().any(|arg| arg.contains("BatchMode=yes"));
    assert!(batched, "fetch prefix missing BatchMode");
    assert!(git_prefix_args(true).is_empty());
}

#[test]
fn tree_starts_at_fresh_origin_not_local_head() {
    let repo = GitRepo::new();
    let (record, _) = require_tree(&repo, &repo.repo, "fix");
    assert_eq!(record.base_commit, repo.origin_main);
    assert_ne!(record.base_commit, repo.local_main);
    assert!(record.base_fetched);
    assert_eq!(record.base_ref, "origin/main");
    let head = repo.ok(&record.path, &["rev-parse", "HEAD"]);
    assert_eq!(head, repo.origin_main);
}

#[test]
fn failed_fetch_uses_cached_ref_with_warning() {
    let repo = GitRepo::new();
    repo.ok(&repo.repo, &["remote", "set-url", "origin", "missing.git"]);
    let (record, warnings) = require_tree(&repo, &repo.repo, "fix");
    assert!(!record.base_fetched);
    assert_eq!(record.base_commit, repo.origin_main);
    let wanted = format!(
        "could not fetch origin/main (network); branched from the copy fetched earlier at {}.",
        &repo.origin_main[..12]
    );
    assert!(
        warnings
            .iter()
            .any(|warning| warning.contains(&wanted)
                || warning.contains("could not fetch origin/main")),
        "missing cached-ref warning"
    );
}

#[test]
fn failed_fetch_without_cache_refuses() {
    let repo = GitRepo::new();
    repo.ok(&repo.repo, &["remote", "set-url", "origin", "missing.git"]);
    repo.ok(
        &repo.repo,
        &["update-ref", "-d", "refs/remotes/origin/main"],
    );
    let error = require_refusal(&repo, &repo.repo);
    let wanted = "could not fetch origin/main";
    let rest = "and there is no earlier copy to branch from.";
    assert!(
        error.what.contains(wanted) && error.what.contains(rest),
        "no refusal text"
    );
    assert_eq!(
        error.next_step,
        "Check the network and your access to origin, then try again."
    );
    let trees = repo.repo.join(".worktrees");
    let wrote = trees.read_dir().map(|entries| entries.count()).unwrap_or(0);
    assert_eq!(wrote, 0, "a failed fetch wrote a tree");
}

#[test]
fn refuses_outside_repo_bare_repo_and_no_origin() {
    let repo = GitRepo::new();
    let outside = repo.path().join("outside");
    std::fs::create_dir(&outside).expect("outside");
    let error = require_refusal(&repo, &outside);
    same(
        &error.what,
        &format!("{} is not inside a git repository.", outside.display()),
    );
    assert_eq!(
        error.next_step,
        "Run talaria --bg without --worktree, or start it from inside a repository."
    );

    let bare = repo.path().join("bare.git");
    repo.ok(repo.path(), &["init", "--bare", "-b", "main", "bare.git"]);
    let error = require_refusal(&repo, &bare);
    same(
        &error.what,
        &format!(
            "{} belongs to a bare repository, which has no working tree to branch from.",
            bare.display()
        ),
    );
    assert_eq!(error.next_step, "Start it from a normal checkout.");

    repo.ok(&repo.repo, &["remote", "remove", "origin"]);
    let error = require_refusal(&repo, &repo.repo);
    assert_eq!(error.what, "this repository has no remote named origin.");
    assert_eq!(
        error.next_step,
        "Add one with git remote add origin <url>, or run without --worktree."
    );
}

#[test]
fn default_branch_from_ls_remote_when_origin_head_unset() {
    let repo = GitRepo::new();
    repo.ok(&repo.repo, &["remote", "set-head", "origin", "--delete"]);
    let (record, _) = require_tree(&repo, &repo.repo, "fix");
    assert_eq!(record.base_ref, "origin/main");
    assert_eq!(record.base_commit, repo.origin_main);
}

#[test]
fn linked_worktree_launch_uses_main_root() {
    let repo = GitRepo::new();
    repo.ok(
        &repo.repo,
        &[
            "worktree",
            "add",
            "--no-track",
            "-b",
            "side",
            "side",
            &repo.local_main,
        ],
    );
    let side = repo.repo.join("side");
    let (record, _) = require_tree(&repo, &side, "fix");
    assert_eq!(record.main_root, repo.repo);
    assert!(record.path.starts_with(repo.repo.join(".worktrees")));
    assert!(!record.path.starts_with(&side));

    let nested = repo.repo.join("nested");
    std::fs::create_dir(&nested).expect("nested");
    let (record, _) = require_tree(&repo, &nested, "nested-fix");
    let git = GitRunner::from_env();
    let root = block(main_root_of(&git, &nested)).expect("main root");
    assert_eq!(root, repo.repo);
    let request = talaria_app::bg::LaunchRequest {
        prompt: "hello".to_owned(),
        profile: "fixture".to_owned(),
        folder: nested,
        model: None,
        provider: None,
        effort: None,
        worktree: Some(talaria_app::bg::WorktreeWanted { name: None }),
        origin: talaria_app::bg::LaunchOrigin::CommandLine,
    };
    let ids = talaria_app::bg::LaunchIds {
        request_id: "req-1".to_owned(),
        submission_id: "sub-1".to_owned(),
    };
    let (mut machine, mut effects) = talaria_app::bg::LaunchMachine::start(request, ids);
    effects.extend(machine.step(talaria_app::bg::LaunchInput::Described(
        talaria_app::bg::DescribeSummary {
            sources: vec!["tui".to_owned()],
            parameters: vec!["cwd".to_owned()],
        },
    )));
    effects.extend(machine.step(talaria_app::bg::LaunchInput::WorktreeReady(record.clone())));
    let cwd = effects.iter().find_map(|effect| match effect {
        talaria_app::bg::LaunchEffect::Create { new } => Some(new.cwd.clone()),
        _ => None,
    });
    assert_eq!(cwd, Some(record.path));
}

#[test]
fn excludes_without_touching_tracked_files() {
    let repo = GitRepo::new();
    let gitignore = repo.repo.join(".gitignore");
    std::fs::write(&gitignore, "target\n").expect("gitignore");
    repo.ok(&repo.repo, &["add", ".gitignore"]);
    repo.ok(
        &repo.repo,
        &[
            "-c",
            "user.name=fixture",
            "-c",
            "user.email=fixture@example.invalid",
            "commit",
            "-m",
            "ignore",
        ],
    );
    let before = std::fs::read(&gitignore).expect("read gitignore");
    let _ = require_tree(&repo, &repo.repo, "fix");
    let after = std::fs::read(&gitignore).expect("read gitignore");
    assert_eq!(before, after);
    let exclude = repo.repo.join(".git/info/exclude");
    let text = std::fs::read_to_string(&exclude).unwrap_or_default();
    assert_eq!(text.matches("/.worktrees/").count(), 1);
}

#[test]
fn worktreeinclude_copies_links_and_skips_escapes() {
    let repo = GitRepo::new();
    std::fs::write(repo.repo.join("kept.txt"), "kept").expect("file");
    std::fs::create_dir(repo.repo.join("linkdir")).expect("dir");
    std::fs::write(repo.repo.join("linkdir/inside.txt"), "in").expect("inside");
    let outside = repo.path().join("secret-target");
    std::fs::write(&outside, "secret").expect("target");
    std::os::unix::fs::symlink(&outside, repo.repo.join("linked")).expect("symlink");
    std::fs::write(
        repo.repo.join(".worktreeinclude"),
        "kept.txt\nlinkdir\n../x\nlinked\n",
    )
    .expect("include");
    let (record, warnings) = require_tree(&repo, &repo.repo, "fix");
    let copied = std::fs::read_to_string(record.path.join("kept.txt")).unwrap_or_default();
    assert_eq!(copied, "kept");
    let meta = std::fs::symlink_metadata(record.path.join("linkdir")).expect("linked dir");
    assert!(meta.file_type().is_symlink());
    assert!(!record.path.join("x").exists());
    assert!(!record.path.join("linked").exists());
    let skipped = warnings.iter().any(|warning| warning.contains("../x"))
        && warnings.iter().any(|warning| warning.contains("linked"));
    assert!(skipped, "missing skip warnings");
    let git = GitRunner::from_env();
    // The list is still read in full, so the same two entries are skipped again, and the
    // copy and the link that already exist are left alone.
    let extra = block(apply_include(&git, &record.main_root, &record.path));
    assert_eq!(extra, warnings);
    let copied = std::fs::read_to_string(record.path.join("kept.txt")).unwrap_or_default();
    assert_eq!(copied, "kept");
}

#[test]
fn branch_has_no_upstream() {
    let repo = GitRepo::new();
    let (record, _) = require_tree(&repo, &repo.repo, "fix");
    let output: Output = repo.run(&record.path, &["rev-parse", "--abbrev-ref", "@{upstream}"]);
    assert!(!output.status.success(), "branch tracks an upstream");
}

#[test]
fn record_round_trip() {
    let repo = GitRepo::new();
    let (record, _) = require_tree(&repo, &repo.repo, "fix");
    let git = GitRunner::from_env();
    block(record_session(&git, &record, "sess-1")).expect("record session");
    let read = block(read_record(&git, &record.main_root, &record.branch));
    let path = read.as_ref().map(|item| item.path.clone());
    assert_eq!(path, Some(record.path.clone()), "no record");
    let session = read.and_then(|item| item.session_id);
    assert_eq!(session.as_deref(), Some("sess-1"));
    let listed = block(records_in(&git, &record.main_root));
    assert!(listed.iter().any(|item| item.branch == record.branch));
    let keys = repo.ok(
        &record.main_root,
        &[
            "config",
            "--local",
            "--get-regexp",
            r"^branch\..*\.talaria-",
        ],
    );
    for key in [
        "talaria-path",
        "talaria-base",
        "talaria-base-commit",
        "talaria-base-fetched",
        "talaria-profile",
        "talaria-created",
        "talaria-session",
    ] {
        assert!(keys.contains(key), "missing {key}");
    }
}

#[test]
fn rollback_removes_only_its_own_clean_tree() {
    let repo = GitRepo::new();
    std::fs::create_dir_all(repo.repo.join(".worktrees")).expect("trees");
    repo.ok(
        &repo.repo,
        &[
            "worktree",
            "add",
            "--no-track",
            "-b",
            "talaria/clean",
            ".worktrees/talaria-clean",
            &repo.origin_main,
        ],
    );
    let clean_path = repo.repo.join(".worktrees/talaria-clean");
    let clean = WorktreeRecord {
        main_root: repo.repo.clone(),
        path: clean_path.clone(),
        branch: "talaria/clean".to_owned(),
        base_ref: "origin/main".to_owned(),
        base_commit: repo.origin_main.clone(),
        base_fetched: true,
        profile: "fixture".to_owned(),
        session_id: None,
        created_at: 1_700_000_000,
    };
    let git = GitRunner::from_env();
    let removed = block(rollback(&git, &clean));
    assert_eq!(removed, Ok(None), "rollback did not remove the tree");
    assert!(!clean_path.exists());

    repo.ok(
        &repo.repo,
        &[
            "worktree",
            "add",
            "--no-track",
            "-b",
            "talaria/kept",
            ".worktrees/talaria-kept",
            &repo.origin_main,
        ],
    );
    let kept_path = repo.repo.join(".worktrees/talaria-kept");
    std::fs::write(kept_path.join("extra.txt"), "extra").expect("extra");
    repo.ok(&kept_path, &["add", "extra.txt"]);
    repo.ok(
        &kept_path,
        &[
            "-c",
            "user.name=fixture",
            "-c",
            "user.email=fixture@example.invalid",
            "commit",
            "-m",
            "extra",
        ],
    );
    let kept = WorktreeRecord {
        path: kept_path.clone(),
        branch: "talaria/kept".to_owned(),
        ..clean.clone()
    };
    let decision = block(rollback(&git, &kept));
    assert_eq!(
        decision,
        Ok(Some("it has commits beyond its base".to_owned())),
        "extra commit was not kept"
    );
    assert!(kept_path.join("extra.txt").is_file());

    let target = repo.path().join("link-target");
    std::fs::write(&target, "stay").expect("target");
    std::os::unix::fs::symlink(&target, clean_path.join("linked")).ok();
    let _ = block(rollback(&git, &clean));
    let target_text = std::fs::read_to_string(&target).unwrap_or_default();
    assert_eq!(target_text, "stay");
}

#[test]
fn name_race_retries_once() {
    let repo = GitRepo::new();
    let program = git_fixture::race_git(repo.path());
    let git = GitRunner::with_program(program);
    let result = block(prepare_worktree(
        &git,
        &repo.repo,
        "fix",
        "fixture",
        1_700_000_000,
        &CancelToken::new(),
    ));
    match result {
        Ok((record, _)) => assert_eq!(record.branch, "talaria/fix-2"),
        Err(error) => panic!("no tree and no record: {}", error.what),
    }
}

const SSH_URL: &str = "ssh://git@example.invalid/repo.git";
const GIT_LIMIT: Duration = Duration::from_secs(20);

/// A repository-local `core.fsmonitor` program must not run on a launch git child.
#[test]
fn launch_git_fsmonitor_never_runs() {
    git_fixture::require_git();
    let fixture = IsolatedRepo::new();
    let marker = fixture.root.join("fsmonitor-ran");
    let script = fixture.script("fsmonitor.sh", &marker);
    fixture.config(
        &fixture.repo,
        &["config", "--local", "core.fsmonitor", &script],
    );

    let runner = GitRunner::from_env().with_environment(fixture.env(&[]));
    let output = block(runner.run_read_only(&fixture.repo, &["status", "--porcelain"], GIT_LIMIT))
        .expect("git status starts");
    assert!(
        !marker.exists(),
        "repository core.fsmonitor ran on git status"
    );
    assert_eq!(output.status, 0, "git status failed: {}", output.stderr);
}

/// A repository-local or worktree `core.sshCommand` must not run, and BatchMode is forced.
#[test]
fn launch_git_repo_ssh_command_cannot_run() {
    git_fixture::require_git();
    let fixture = IsolatedRepo::new();
    let ssh_marker = fixture.root.join("ssh-ran");
    let fs_marker = fixture.root.join("fsmonitor-ran");
    let ssh = fixture.script("ssh-local.sh", &ssh_marker);
    let fsmonitor = fixture.script("fsmonitor.sh", &fs_marker);
    fixture.config(
        &fixture.repo,
        &["config", "--local", "core.sshCommand", &ssh],
    );
    fixture.config(
        &fixture.repo,
        &["config", "--local", "extensions.worktreeConfig", "true"],
    );
    fixture.config(
        &fixture.repo,
        &["config", "--worktree", "core.sshCommand", &ssh],
    );
    fixture.config(
        &fixture.repo,
        &["config", "--local", "core.fsmonitor", &fsmonitor],
    );
    let (program, log) = logging_git(&fixture.root);
    let runner = GitRunner::with_program(program).with_environment(fixture.env(&[]));

    let _ = block(runner.run(&fixture.repo, &["ls-remote", SSH_URL], GIT_LIMIT));
    assert!(
        !ssh_marker.exists(),
        "repository core.sshCommand ran on a network git call"
    );
    assert!(
        !fs_marker.exists(),
        "repository core.fsmonitor ran during the network git call"
    );
    let network = network_argv(&log);
    assert!(
        has_pair(&network, "-c", "core.fsmonitor=false"),
        "network git call missing fsmonitor hardening: {network:?}"
    );
    assert!(
        has_pair(&network, "-c", "core.sshCommand=ssh -o BatchMode=yes"),
        "network git call did not force BatchMode: {network:?}"
    );
}

/// `GIT_SSH_COMMAND` stays the operator's command, including over a hostile local value.
#[test]
fn launch_git_ssh_command_env_is_preserved() {
    git_fixture::require_git();
    let fixture = IsolatedRepo::new();
    let operator_marker = fixture.root.join("operator-ran");
    let hostile_marker = fixture.root.join("hostile-ran");
    let operator = fixture.script("ssh-operator.sh", &operator_marker);
    let hostile = fixture.script("ssh-hostile.sh", &hostile_marker);
    fixture.config(
        &fixture.repo,
        &["config", "--local", "core.sshCommand", &hostile],
    );
    let (program, log) = logging_git(&fixture.root);
    let runner = GitRunner::with_program(program)
        .with_environment(fixture.env(&[("GIT_SSH_COMMAND", operator)]));

    let _ = block(runner.run(&fixture.repo, &["ls-remote", SSH_URL], GIT_LIMIT));
    assert!(
        operator_marker.exists(),
        "operator GIT_SSH_COMMAND did not run"
    );
    assert!(
        !hostile_marker.exists(),
        "repository core.sshCommand replaced GIT_SSH_COMMAND"
    );
    let network = network_argv(&log);
    assert!(
        !network.iter().any(|arg| arg.contains("BatchMode")),
        "BatchMode was forced over GIT_SSH_COMMAND: {network:?}"
    );
}

/// A global `core.sshCommand` is what runs, not the repository-local one.
#[test]
fn launch_git_global_ssh_command_is_preserved() {
    assert_operator_config_wins(ConfigScope::Global);
}

/// A system `core.sshCommand` is what runs when the operator has no global command.
#[test]
fn launch_git_system_ssh_command_is_preserved() {
    assert_operator_config_wins(ConfigScope::System);
}

/// `GIT_SSH` is the operator's program even when the repository sets `core.sshCommand`.
#[test]
fn launch_git_ssh_program_is_preserved() {
    git_fixture::require_git();
    let fixture = IsolatedRepo::new();
    let operator_marker = fixture.root.join("operator-ran");
    let hostile_marker = fixture.root.join("hostile-ran");
    let operator = fixture.script("ssh-operator.sh", &operator_marker);
    let hostile = fixture.script("ssh-hostile.sh", &hostile_marker);
    fixture.config(
        &fixture.repo,
        &["config", "--local", "core.sshCommand", &hostile],
    );
    let (program, log) = logging_git(&fixture.root);
    let runner = GitRunner::with_program(program)
        .with_environment(fixture.env(&[("GIT_SSH", operator.clone())]));

    let _ = block(runner.run(&fixture.repo, &["ls-remote", SSH_URL], GIT_LIMIT));
    assert!(operator_marker.exists(), "operator GIT_SSH did not run");
    assert!(
        !hostile_marker.exists(),
        "repository core.sshCommand replaced GIT_SSH"
    );
    let network = network_argv(&log);
    let pinned = format!("core.sshCommand='{operator}'");
    assert!(
        has_pair(&network, "-c", &pinned),
        "GIT_SSH was not pinned over repository config: {network:?}"
    );
}

/// `GIT_SSH` is one executable path. A space must stay inside one shell word.
#[test]
fn launch_git_ssh_program_path_with_spaces_is_one_word() {
    git_fixture::require_git();
    let fixture = IsolatedRepo::new();
    let operator_marker = fixture.root.join("operator-ran");
    let hostile_marker = fixture.root.join("hostile-ran");
    let dir = fixture.root.join("ssh dir");
    std::fs::create_dir(&dir).expect("spaced directory");
    let operator = fixture.script_at(&dir.join("ssh.sh"), &operator_marker);
    let hostile = fixture.script("ssh-hostile.sh", &hostile_marker);
    fixture.config(
        &fixture.repo,
        &["config", "--local", "core.sshCommand", &hostile],
    );
    let (program, log) = logging_git(&fixture.root);
    let runner = GitRunner::with_program(program)
        .with_environment(fixture.env(&[("GIT_SSH", operator.clone())]));

    let _ = block(runner.run(&fixture.repo, &["ls-remote", SSH_URL], GIT_LIMIT));
    assert!(operator_marker.exists(), "spaced GIT_SSH path did not run");
    assert!(
        !hostile_marker.exists(),
        "repository core.sshCommand replaced GIT_SSH"
    );
    let network = network_argv(&log);
    let pinned = format!("core.sshCommand='{operator}'");
    assert!(
        has_pair(&network, "-c", &pinned),
        "GIT_SSH was split or left unquoted: {network:?}"
    );
}

/// An included global `core.sshCommand` is the operator command.
#[test]
fn launch_git_included_global_ssh_command_is_preserved() {
    git_fixture::require_git();
    let fixture = IsolatedRepo::new();
    let operator_marker = fixture.root.join("operator-ran");
    let hostile_marker = fixture.root.join("hostile-ran");
    let operator = fixture.script("ssh-operator.sh", &operator_marker);
    let hostile = fixture.script("ssh-hostile.sh", &hostile_marker);
    fixture.config(
        &fixture.repo,
        &["config", "--local", "core.sshCommand", &hostile],
    );
    let included = fixture.root.join("included.gitconfig");
    std::fs::write(&included, format!("[core]\n\tsshCommand = {operator}\n")).expect("include");
    std::fs::write(
        fixture.root.join("global-gitconfig"),
        format!("[include]\n\tpath = {}\n", included.display()),
    )
    .expect("global include");
    let (program, log) = logging_git(&fixture.root);
    let runner = GitRunner::with_program(program).with_environment(fixture.env(&[]));

    let _ = block(runner.run(&fixture.repo, &["ls-remote", SSH_URL], GIT_LIMIT));
    assert!(
        operator_marker.exists(),
        "included global core.sshCommand did not run"
    );
    assert!(
        !hostile_marker.exists(),
        "repository command replaced include"
    );
    let network = network_argv(&log);
    let pinned = format!("core.sshCommand={operator}");
    assert!(
        has_pair(&network, "-c", &pinned),
        "included command was not pinned: {network:?}"
    );
}

/// `~/.gitconfig` overrides `$XDG_CONFIG_HOME/git/config` when both set ssh.
#[test]
fn launch_git_home_gitconfig_overrides_xdg() {
    git_fixture::require_git();
    let fixture = IsolatedRepo::new();
    let home_marker = fixture.root.join("home-ran");
    let xdg_marker = fixture.root.join("xdg-ran");
    let hostile_marker = fixture.root.join("hostile-ran");
    let home_script = fixture.script("ssh-home.sh", &home_marker);
    let xdg_script = fixture.script("ssh-xdg.sh", &xdg_marker);
    let hostile = fixture.script("ssh-hostile.sh", &hostile_marker);
    fixture.config(
        &fixture.repo,
        &["config", "--local", "core.sshCommand", &hostile],
    );
    let xdg = fixture.root.join("xdg");
    std::fs::create_dir_all(xdg.join("git")).expect("xdg git config dir");
    std::fs::write(
        fixture.root.join("home").join(".gitconfig"),
        format!("[core]\n\tsshCommand = {home_script}\n"),
    )
    .expect("home gitconfig");
    std::fs::write(
        xdg.join("git").join("config"),
        format!("[core]\n\tsshCommand = {xdg_script}\n"),
    )
    .expect("xdg gitconfig");
    let mut env = fixture.env(&[]);
    env.remove("GIT_CONFIG_GLOBAL");
    env.insert(
        "XDG_CONFIG_HOME".to_owned(),
        xdg.to_string_lossy().into_owned(),
    );
    let (program, log) = logging_git(&fixture.root);
    let runner = GitRunner::with_program(program).with_environment(env);

    let _ = block(runner.run(&fixture.repo, &["ls-remote", SSH_URL], GIT_LIMIT));
    assert!(
        home_marker.exists(),
        "home gitconfig ssh command did not run"
    );
    assert!(
        !xdg_marker.exists(),
        "XDG git config overrode the home file"
    );
    assert!(!hostile_marker.exists(), "repository command replaced home");
    let network = network_argv(&log);
    let pinned = format!("core.sshCommand={home_script}");
    assert!(
        has_pair(&network, "-c", &pinned),
        "home ssh command was not pinned: {network:?}"
    );
}

/// `GIT_CONFIG_NOSYSTEM` drops the system ssh command.
#[test]
fn launch_git_nosystem_ignores_system_ssh_command() {
    git_fixture::require_git();
    let fixture = IsolatedRepo::new();
    let system_marker = fixture.root.join("system-ran");
    let hostile_marker = fixture.root.join("hostile-ran");
    let system_script = fixture.script("ssh-system.sh", &system_marker);
    let hostile = fixture.script("ssh-hostile.sh", &hostile_marker);
    fixture.config(
        &fixture.repo,
        &["config", "--local", "core.sshCommand", &hostile],
    );
    std::fs::write(
        fixture.root.join("system-gitconfig"),
        format!("[core]\n\tsshCommand = {system_script}\n"),
    )
    .expect("system config");
    let (program, log) = logging_git(&fixture.root);
    let runner = GitRunner::with_program(program).with_environment(fixture.env(&[]));

    let _ = block(runner.run(&fixture.repo, &["ls-remote", SSH_URL], GIT_LIMIT));
    assert!(
        !system_marker.exists(),
        "GIT_CONFIG_NOSYSTEM still ran the system ssh command"
    );
    assert!(
        !hostile_marker.exists(),
        "repository command ran when system config was disabled"
    );
    let network = network_argv(&log);
    assert!(
        has_pair(&network, "-c", "core.sshCommand=ssh -o BatchMode=yes"),
        "BatchMode was not forced with system config disabled: {network:?}"
    );
}

/// A positional `-c core.fsmonitor=false` must not suppress the leading override.
#[test]
fn launch_git_fsmonitor_override_ignores_positional_pair() {
    git_fixture::require_git();
    let fixture = IsolatedRepo::new();
    let marker = fixture.root.join("fsmonitor-ran");
    let script = fixture.script("fsmonitor.sh", &marker);
    fixture.config(
        &fixture.repo,
        &["config", "--local", "core.fsmonitor", &script],
    );
    let (program, log) = logging_git(&fixture.root);
    let runner = GitRunner::with_program(program).with_environment(fixture.env(&[]));

    let _ = block(runner.run_read_only(
        &fixture.repo,
        &["status", "--porcelain", "--", "-c", "core.fsmonitor=false"],
        GIT_LIMIT,
    ));
    assert!(
        !marker.exists(),
        "positional fsmonitor pair suppressed the override"
    );
    let status = command_argv(&log, "status");
    assert_eq!(
        status.first().map(String::as_str),
        Some("-c"),
        "fsmonitor override was not leading: {status:?}"
    );
    assert_eq!(
        status.get(1).map(String::as_str),
        Some("core.fsmonitor=false"),
        "fsmonitor override was not leading: {status:?}"
    );
}

/// A parent `GIT_DIR` must not make the ssh probe read that repository.
#[test]
fn launch_git_probe_ignores_parent_git_dir() {
    if std::env::var_os("TALARIA_PROBE_INNER").is_some() {
        probe_with_parent_git_dir();
        return;
    }
    git_fixture::require_git();
    let fixture = IsolatedRepo::new();
    let hostile_marker = fixture.root.join("hostile-ran");
    let hostile = fixture.script("ssh-hostile.sh", &hostile_marker);
    fixture.config(
        &fixture.repo,
        &["config", "--local", "core.sshCommand", &hostile],
    );
    let (program, _log) = logging_git_probe(&fixture.root);
    let output = Command::new(std::env::current_exe().expect("test executable"))
        .args([
            "--exact",
            "worktree_git::launch_git_probe_ignores_parent_git_dir",
        ])
        .env("TALARIA_PROBE_INNER", "1")
        .env("GIT_DIR", fixture.repo.join(".git"))
        .env("TALARIA_PROBE_ROOT", &fixture.root)
        .env("TALARIA_PROBE_REPO", &fixture.repo)
        .env("TALARIA_PROBE_PROGRAM", &program)
        .output()
        .expect("re-exec probe test");
    assert!(
        output.status.success(),
        "inner probe test failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

fn probe_with_parent_git_dir() {
    let root = PathBuf::from(std::env::var("TALARIA_PROBE_ROOT").expect("root"));
    let repo = PathBuf::from(std::env::var("TALARIA_PROBE_REPO").expect("repo"));
    let program = PathBuf::from(std::env::var("TALARIA_PROBE_PROGRAM").expect("program"));
    let runner = GitRunner::with_program(program).with_environment(isolated_env(&root, &[]));
    let _ = block(runner.run(&repo, &["ls-remote", SSH_URL], GIT_LIMIT));
    let recorded = std::fs::read_to_string(root.join("git-args").join("probe-git-dir"))
        .expect("ssh probe did not run");
    assert_eq!(recorded, "UNSET", "probe inherited GIT_DIR: {recorded}");
    assert!(
        !root.join("hostile-ran").exists(),
        "probe adopted the repository ssh command"
    );
    let network = network_argv(&root.join("git-args"));
    assert!(
        has_pair(&network, "-c", "core.sshCommand=ssh -o BatchMode=yes"),
        "BatchMode was not forced when GIT_DIR pointed at a repository: {network:?}"
    );
}

#[derive(Clone, Copy, Debug)]
enum ConfigScope {
    Global,
    System,
}

fn assert_operator_config_wins(scope: ConfigScope) {
    git_fixture::require_git();
    let fixture = IsolatedRepo::new();
    let operator_marker = fixture.root.join("operator-ran");
    let hostile_marker = fixture.root.join("hostile-ran");
    let operator = fixture.script("ssh-operator.sh", &operator_marker);
    let hostile = fixture.script("ssh-hostile.sh", &hostile_marker);
    fixture.config(
        &fixture.repo,
        &["config", "--local", "core.sshCommand", &hostile],
    );
    let file = match scope {
        ConfigScope::Global => fixture.root.join("global-gitconfig"),
        ConfigScope::System => fixture.root.join("system-gitconfig"),
    };
    let file_arg = file.to_string_lossy().into_owned();
    fixture.config(
        &fixture.root,
        &["config", "--file", &file_arg, "core.sshCommand", &operator],
    );
    let (program, log) = logging_git(&fixture.root);
    let mut env = fixture.env(&[]);
    if matches!(scope, ConfigScope::System) {
        env.remove("GIT_CONFIG_NOSYSTEM");
    }
    let runner = GitRunner::with_program(program).with_environment(env);

    let _ = block(runner.run(&fixture.repo, &["ls-remote", SSH_URL], GIT_LIMIT));
    assert!(
        operator_marker.exists(),
        "operator {scope:?} core.sshCommand did not run"
    );
    assert!(
        !hostile_marker.exists(),
        "repository core.sshCommand replaced the operator command"
    );
    let network = network_argv(&log);
    let pinned = format!("core.sshCommand={operator}");
    assert!(
        has_pair(&network, "-c", &pinned),
        "operator ssh command was not pinned: {network:?}"
    );
}

struct IsolatedRepo {
    _tmp: tempfile::TempDir,
    root: PathBuf,
    repo: PathBuf,
}

impl IsolatedRepo {
    fn new() -> Self {
        let tmp = tempfile::tempdir().expect("temp folder");
        let root = std::fs::canonicalize(tmp.path()).expect("canonical temp folder");
        std::fs::create_dir_all(root.join("home")).expect("home");
        std::fs::write(root.join("global-gitconfig"), "").expect("global config");
        std::fs::write(root.join("system-gitconfig"), "").expect("system config");
        let repo = root.join("repo");
        let fixture = Self {
            _tmp: tmp,
            root,
            repo: repo.clone(),
        };
        let repo_arg = repo.to_string_lossy().into_owned();
        fixture.config(&fixture.root, &["init", "-q", "-b", "main", &repo_arg]);
        fixture
    }

    fn env(&self, extra: &[(&str, String)]) -> BTreeMap<String, String> {
        isolated_env(&self.root, extra)
    }

    fn script(&self, name: &str, marker: &Path) -> String {
        self.script_at(&self.root.join(name), marker)
    }

    fn script_at(&self, path: &Path, marker: &Path) -> String {
        let marker = marker.to_string_lossy().replace('\'', "'\\''");
        std::fs::write(path, format!("#!/bin/sh\ntouch '{marker}'\nexit 1\n")).expect("script");
        let mut permissions = std::fs::metadata(path)
            .expect("script metadata")
            .permissions();
        permissions.set_mode(0o755);
        std::fs::set_permissions(path, permissions).expect("script mode");
        path.to_string_lossy().into_owned()
    }

    fn config(&self, cwd: &Path, args: &[&str]) {
        let output = Command::new("git")
            .args(args)
            .current_dir(cwd)
            .envs(self.env(&[]))
            .stdin(std::process::Stdio::null())
            .output()
            .expect("git config starts");
        assert!(
            output.status.success(),
            "git {} failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&output.stderr)
        );
    }
}

fn isolated_env(root: &Path, extra: &[(&str, String)]) -> BTreeMap<String, String> {
    let mut env = BTreeMap::new();
    env.insert("PATH".to_owned(), std::env::var("PATH").expect("PATH"));
    env.insert(
        "HOME".to_owned(),
        root.join("home").to_string_lossy().into_owned(),
    );
    env.insert(
        "GIT_CONFIG_GLOBAL".to_owned(),
        root.join("global-gitconfig").to_string_lossy().into_owned(),
    );
    env.insert(
        "GIT_CONFIG_SYSTEM".to_owned(),
        root.join("system-gitconfig").to_string_lossy().into_owned(),
    );
    env.insert("GIT_CONFIG_NOSYSTEM".to_owned(), "1".to_owned());
    env.insert("GIT_TERMINAL_PROMPT".to_owned(), "0".to_owned());
    for (key, value) in extra {
        env.insert((*key).to_owned(), value.clone());
    }
    env
}

fn logging_git(dir: &Path) -> (PathBuf, PathBuf) {
    write_logging_git(dir, false)
}

/// Like [`logging_git`], and records `GIT_DIR` for the ssh config probe.
fn logging_git_probe(dir: &Path) -> (PathBuf, PathBuf) {
    write_logging_git(dir, true)
}

fn write_logging_git(dir: &Path, record_probe_git_dir: bool) -> (PathBuf, PathBuf) {
    let log = dir.join("git-args");
    std::fs::create_dir(&log).expect("argument log");
    let record = if record_probe_git_dir {
        "probe=0\nfor arg in \"$@\"; do\n  if [ \"$arg\" = --show-scope ]; then\n    probe=1\n  fi\ndone\nif [ \"$probe\" = 1 ]; then\n  if [ -n \"${GIT_DIR+set}\" ]; then\n    printf '%s' \"$GIT_DIR\" > \"$log_dir/probe-git-dir\"\n  else\n    printf '%s' UNSET > \"$log_dir/probe-git-dir\"\n  fi\nfi\n"
    } else {
        ""
    };
    let body = format!(
        "#!/bin/sh\nlog_dir='{}'\n{record}log=$(mktemp \"$log_dir/call.XXXXXXXX\") || exit 1\nfirst=1\nfor arg in \"$@\"; do\n  if [ \"$first\" = 1 ]; then\n    first=0\n  else\n    printf '\\037' >> \"$log\"\n  fi\n  printf '%s' \"$arg\" >> \"$log\"\ndone\nprintf '\\n' >> \"$log\"\nexec git \"$@\"\n",
        log.to_string_lossy().replace('\'', "'\\''")
    );
    let path = dir.join("logging-git");
    std::fs::write(&path, body).expect("wrapper");
    let mut permissions = std::fs::metadata(&path)
        .expect("wrapper metadata")
        .permissions();
    permissions.set_mode(0o755);
    std::fs::set_permissions(&path, permissions).expect("wrapper mode");
    (path, log)
}

fn command_argv(log: &Path, command: &str) -> Vec<String> {
    read_arg_log(log)
        .into_iter()
        .find(|argv| argv.iter().any(|arg| arg == command))
        .unwrap_or_else(|| panic!("one {command}"))
}

fn network_argv(log: &Path) -> Vec<String> {
    command_argv(log, "ls-remote")
}

fn read_arg_log(log: &Path) -> Vec<Vec<String>> {
    std::fs::read_dir(log)
        .expect("read argument log")
        .filter_map(|entry| {
            let path = entry.expect("log entry").path();
            let name = path.file_name()?.to_string_lossy();
            if !name.starts_with("call.") {
                return None;
            }
            let text = std::fs::read_to_string(&path).expect("log text");
            Some(
                text.strip_suffix('\n')
                    .unwrap_or(&text)
                    .split('\x1f')
                    .map(str::to_owned)
                    .collect(),
            )
        })
        .collect()
}

fn has_pair(argv: &[String], left: &str, right: &str) -> bool {
    argv.windows(2)
        .any(|pair| pair[0] == left && pair[1] == right)
}
