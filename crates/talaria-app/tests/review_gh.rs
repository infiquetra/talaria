//! `gh`-runner red tests for the review. Issue #245.
//!
//! Replies come only from recordings in `fixtures/gh/`. A missing recording
//! fails `BLOCKED`, never silently.

#[path = "support/gh_stub.rs"]
mod gh_stub;

#[path = "support/git_fixture.rs"]
mod git_fixture;

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use gh_stub::{Reply, read_log, write_arg_stub, write_counting_stub, write_reply_stub};
use git_fixture::GitRepo;
use talaria_app::review::gh::GhRunner;
use talaria_app::review::pr_status::{PrDemand, PrStatusNotice, PrStatusService};
use talaria_app::review::service::ReviewService;
use talaria_app::worktree::{GitError, GitRunner};
use talaria_core::review::{GhReady, GitAction, ReviewEffect};

fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/gh")
}

fn frames_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/frames")
}

fn require(name: &str) -> Reply {
    let path = fixtures_dir().join(name);
    if !path.exists() {
        panic!("BLOCKED: fixtures/gh/{name} not recorded");
    }
    Reply::read(&path)
}

/// Three recorded readiness states, plus missing binary, closed stdin and a
/// killed sleep.
#[tokio::test]
async fn readiness_states() {
    let signed_in = require("auth-status-signed-in.json");
    let signed_out = require("auth-status-signed-out.json");
    let github = require("repo-view-github.json");
    let not_github = require("repo-view-not-github.json");

    let dir = tempfile::tempdir().expect("temp folder");

    // Signed in and on GitHub.
    let stub = write_reply_stub(dir.path(), &[signed_in, github]);
    let gh = GhRunner::with_binary(stub.program.clone());
    assert_eq!(
        gh.readiness(dir.path()).await,
        GhReady::Ready {
            repo: "fixture-owner/fixture-repo".to_owned(),
        }
    );

    // Signed out.
    let stub = write_reply_stub(dir.path(), &[signed_out]);
    let gh = GhRunner::with_binary(stub.program.clone());
    let ready = gh.readiness(dir.path()).await;
    assert!(
        matches!(ready, GhReady::NotSignedIn(_)),
        "signed out gives NotSignedIn: {ready:?}"
    );

    // A non-GitHub origin.
    let signed_in = require("auth-status-signed-in.json");
    let stub = write_reply_stub(dir.path(), &[signed_in, not_github]);
    let gh = GhRunner::with_binary(stub.program.clone());
    let ready = gh.readiness(dir.path()).await;
    assert!(
        matches!(ready, GhReady::NotGitHub(_)),
        "a non-GitHub origin gives NotGitHub: {ready:?}"
    );

    // Standard input is closed: a stub that reads it sees end of file at once.
    let probe = dir.path().join("stdin-probe");
    std::fs::write(&probe, "#!/bin/sh\nwc -c\n").expect("probe script");
    {
        use std::os::unix::fs::PermissionsExt;
        let mut permissions = std::fs::metadata(&probe)
            .expect("probe metadata")
            .permissions();
        permissions.set_mode(0o755);
        std::fs::set_permissions(&probe, permissions).expect("probe mode");
    }
    let gh = GhRunner::with_binary(probe);
    let started = Instant::now();
    let output = gh
        .run(dir.path(), &["auth", "status"], Duration::from_secs(5))
        .await
        .expect("the probe runs");
    assert_eq!(output.stdout.trim(), "0", "stdin reads end of file at once");
    assert!(
        started.elapsed() < Duration::from_secs(2),
        "no waiting on stdin"
    );

    // A sleeping stub is killed when the timeout passes.
    let stub = write_arg_stub(dir.path(), 30);
    let gh = GhRunner::with_binary(stub.program.clone());
    let started = Instant::now();
    let failure = gh
        .run(dir.path(), &["auth", "status"], Duration::from_millis(500))
        .await;
    assert!(
        matches!(failure, Err(GitError::TimedOut)),
        "the sleep is killed: {failure:?}"
    );
    assert!(
        started.elapsed() < Duration::from_secs(5),
        "the timeout kills promptly"
    );

    // A missing binary. Vacuous while the runner is a stub; real once it runs.
    let gh = GhRunner::with_binary(dir.path().join("missing-gh"));
    assert_eq!(gh.readiness(dir.path()).await, GhReady::NotFound);
}

/// Draft creation sends the exact arguments, reads the URL, surfaces failures,
/// and never runs when the push fails.
#[tokio::test]
async fn create_draft_argv_and_url() {
    let created = require("pr-create.json");
    let url = created
        .stdout
        .lines()
        .rev()
        .find(|line| line.starts_with("https://"))
        .expect("the recording prints a URL")
        .to_owned();

    let dir = tempfile::tempdir().expect("temp folder");
    let stub = write_reply_stub(dir.path(), &[created]);
    let gh = GhRunner::with_binary(stub.program.clone());
    assert_eq!(
        gh.create_draft(dir.path(), "main", "talaria/work").await,
        Ok(url)
    );
    let calls = read_log(&stub.log);
    assert_eq!(calls.len(), 1, "one gh call: {calls:?}");
    assert_eq!(
        calls[0],
        vec![
            "pr".to_owned(),
            "create".to_owned(),
            "--draft".to_owned(),
            "--base".to_owned(),
            "main".to_owned(),
            "--head".to_owned(),
            "talaria/work".to_owned(),
            "--fill".to_owned(),
        ]
    );

    // A failure surfaces with the last error line.
    let stub = write_reply_stub(dir.path(), &[]);
    let gh = GhRunner::with_binary(stub.program.clone());
    let failure = gh.create_draft(dir.path(), "main", "talaria/work").await;
    assert!(
        failure
            .expect_err("a missing reply fails")
            .starts_with("gh pr create failed: "),
        "the failure carries gh's line"
    );

    // A refused push stops the action before any `gh pr create` runs.
    let git = GitRepo::new();
    std::fs::write(git.repo.join(".git").join("index.lock"), "").expect("lock file");
    let stub = write_reply_stub(dir.path(), &[require("pr-create.json")]);
    let gh = GhRunner::with_binary(stub.program.clone());
    let pr = Arc::new(PrStatusService::new(
        gh,
        GitRunner::from_env(),
        Arc::new(Instant::now),
        |_: PrStatusNotice| {},
    ));
    let service = ReviewService::new(
        GitRunner::from_env(),
        GhRunner::with_binary(stub.program.clone()),
        pr,
    );
    let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();
    service.execute(
        ReviewEffect::Run(GitAction::CreatePr {
            base: "main".to_owned(),
            branch: "talaria/work".to_owned(),
        }),
        tx,
    );
    let calls = read_log(&stub.log);
    assert!(
        calls.is_empty(),
        "no gh runs when the push is refused: {calls:?}"
    );
}

/// One process at a time, one query per 60 seconds, at once after a push, and
/// none while idle.
#[tokio::test]
async fn pr_status_polling_is_bounded() {
    let status = require("api-graphql-status.json");
    let now = Arc::new(Mutex::new(Instant::now()));
    let tick = now.clone();
    let clock: Arc<dyn Fn() -> Instant + Send + Sync> =
        Arc::new(move || *tick.lock().expect("clock"));

    let dir = tempfile::tempdir().expect("temp folder");
    let stub = write_counting_stub(dir.path(), &[status], 1);
    let gh = GhRunner::with_binary(stub.program.clone());
    let seen = Arc::new(Mutex::new(0_u32));
    let count = seen.clone();
    let pr = PrStatusService::new(gh, GitRunner::from_env(), clock, move |_| {
        *count.lock().expect("count") += 1;
    });

    let git = GitRepo::new();
    git.ok(&git.repo, &["checkout", "-q", "-b", "talaria/work"]);
    pr.want(PrDemand::Review(git.repo.clone(), true));
    pr.tick();
    pr.tick();
    let lines = std::fs::read_to_string(&stub.log).unwrap_or_default();
    let starts = lines
        .lines()
        .filter(|line| line.starts_with("start "))
        .count();
    assert_eq!(starts, 1, "a refresh due while one runs is skipped");
    let mut depth = 0;
    for line in lines.lines() {
        if line.starts_with("start ") {
            depth += 1;
        } else if line.starts_with("end ") {
            depth -= 1;
        }
        assert!(depth <= 1, "never two processes at once: {lines}");
    }

    *now.lock().expect("clock") += Duration::from_secs(30);
    pr.tick();
    let lines = std::fs::read_to_string(&stub.log).unwrap_or_default();
    assert_eq!(
        lines
            .lines()
            .filter(|line| line.starts_with("start "))
            .count(),
        1,
        "no second query within 60 seconds"
    );
    *now.lock().expect("clock") += Duration::from_secs(31);
    pr.tick();
    let lines = std::fs::read_to_string(&stub.log).unwrap_or_default();
    assert_eq!(
        lines
            .lines()
            .filter(|line| line.starts_with("start "))
            .count(),
        2,
        "one query per 60 seconds"
    );

    pr.refresh_now(&git.repo);
    let lines = std::fs::read_to_string(&stub.log).unwrap_or_default();
    assert_eq!(
        lines
            .lines()
            .filter(|line| line.starts_with("start "))
            .count(),
        3,
        "a push refreshes at once"
    );

    pr.want(PrDemand::Review(git.repo.clone(), false));
    *now.lock().expect("clock") += Duration::from_secs(600);
    pr.tick();
    let lines = std::fs::read_to_string(&stub.log).unwrap_or_default();
    assert_eq!(
        lines
            .lines()
            .filter(|line| line.starts_with("start "))
            .count(),
        3,
        "nothing refreshes while idle"
    );
}

/// Every recording is public-safe by allowlist.
#[test]
fn recordings_are_public_safe() {
    let gh_files = [
        "auth-status-signed-in.json",
        "auth-status-signed-out.json",
        "repo-view-github.json",
        "repo-view-not-github.json",
        "pr-create.json",
        "api-graphql-status.json",
    ];
    for name in gh_files {
        let path = fixtures_dir().join(name);
        if !path.exists() {
            panic!("BLOCKED: fixtures/gh/{name} not recorded");
        }
        let text = std::fs::read_to_string(&path).expect("reply file");
        let reply: serde_json::Value = serde_json::from_str(&text).expect("reply json");
        for key in ["args_prefix", "exit", "stdout", "stderr"] {
            assert!(reply.get(key).is_some(), "{name} holds {key}");
        }
        check_public_safe(&path, &text);
    }
    for name in ["review-oneshot.jsonl", "review-session-info.jsonl"] {
        let path = frames_dir().join(name);
        if !path.exists() {
            panic!("BLOCKED: fixtures/frames/{name} not recorded");
        }
        let text = std::fs::read_to_string(&path).expect("frame file");
        for (number, line) in text.lines().enumerate() {
            let frame: serde_json::Value =
                serde_json::from_str(line).expect("one JSON frame per line");
            let _ = (number, frame);
            check_public_safe(&path, line);
        }
    }
}

/// The allowlist: fixture names only, no paths, no outside profiles.
fn check_public_safe(path: &Path, text: &str) {
    for url in find_urls(text) {
        assert!(
            url.starts_with("https://github.com/fixture-owner/fixture-repo"),
            "{}: {url}",
            path.display()
        );
    }
    for owner in find_name_with_owner(text) {
        assert_eq!(owner, "fixture-owner/fixture-repo", "{}", path.display());
    }
    for login in find_logins(text) {
        assert_eq!(login, "fixture-user", "{}", path.display());
    }
    for profile in find_profiles(text) {
        assert!(
            profile == "default" || profile == "fixture",
            "{}: {profile}",
            path.display()
        );
    }
    for cwd in find_cwds(text) {
        assert_eq!(cwd, "<tmp>", "{}", path.display());
    }
    for token in find_path_tokens(text) {
        assert_eq!(token, "/dev/null", "{}: {token}", path.display());
    }
}

fn find_urls(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    for part in text.split("https://").skip(1) {
        let end = part
            .find(|ch: char| ch.is_whitespace() || matches!(ch, '"' | '\'' | ')' | ']'))
            .unwrap_or(part.len());
        out.push(format!("https://{}", &part[..end]));
    }
    out
}

fn find_name_with_owner(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    for part in text.split("nameWithOwner").skip(1) {
        let value = part.split('"').nth(2).unwrap_or_default().trim().to_owned();
        if !value.is_empty() {
            out.push(value);
        }
    }
    for part in text.split("repository(owner:").skip(1) {
        let owner = part.split('"').nth(1).unwrap_or_default();
        let name = part.split('"').nth(3).unwrap_or_default();
        if !owner.is_empty() {
            out.push(format!("{owner}/{name}"));
        }
    }
    out
}

fn find_logins(text: &str) -> Vec<String> {
    text.split("account ")
        .skip(1)
        .map(|part| {
            part.split(|ch: char| ch.is_whitespace() || ch == ')')
                .next()
                .unwrap_or_default()
                .trim_matches(|ch| ch == '"' || ch == '\'')
                .to_owned()
        })
        .filter(|login| !login.is_empty())
        .collect()
}

fn find_profiles(text: &str) -> Vec<String> {
    text.split("\"profile\"")
        .skip(1)
        .filter_map(|part| part.split('"').nth(2).map(str::to_owned))
        .collect()
}

fn find_cwds(text: &str) -> Vec<String> {
    text.split("\"cwd\"")
        .skip(1)
        .filter_map(|part| {
            let value = part.split('"').nth(1)?;
            if value.trim_start().starts_with(':') {
                part.split('"').nth(2).map(str::to_owned)
            } else {
                None
            }
        })
        .collect()
}

fn find_path_tokens(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let bytes = text.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'/'
            && index + 1 < bytes.len()
            && bytes[index + 1].is_ascii_alphabetic()
            && (index == 0
                || matches!(
                    bytes[index - 1],
                    b' ' | b'\t' | b'\n' | b'"' | b'\'' | b'=' | b'(' | b':'
                ))
        {
            let start = index;
            while index < bytes.len()
                && !matches!(
                    bytes[index],
                    b' ' | b'\t' | b'\n' | b'"' | b'\'' | b')' | b']' | b','
                )
            {
                index += 1;
            }
            out.push(text[start..index].to_owned());
        } else {
            index += 1;
        }
    }
    out
}
