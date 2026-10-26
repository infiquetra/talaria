//! Git worktrees. Each test uses its own temporary repository and expects the real outcome.
//! The stub runner reports git missing, so the outcome assertions fail.

#[path = "support/git_fixture.rs"]
mod git_fixture;

use std::collections::BTreeMap;
use std::path::Path;
use std::process::Output;

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
    let extra = block(apply_include(&git, &record.main_root, &record.path));
    assert!(extra.is_empty());
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
