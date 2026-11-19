//! Folder-lookup red tests for the review. Issue #245.
//!
//! The `session.info` answers come from a frame recording, served by the mock
//! gateway. A missing recording fails `BLOCKED`, never silently.

use crate::git_fixture;

use crate::mock_gateway;

use std::collections::BTreeSet;
use std::future::Future;
use std::path::PathBuf;
use std::pin::Pin;
use std::time::Duration;

use git_fixture::{GitRepo, git_available};
use mock_gateway::{Behavior, StandaloneGateway};
use serde_json::{Value, json};
use talaria_app::bg::{CallFailure, LaunchConn, LaunchLink};
use talaria_app::review::git_ops::list;
use talaria_app::review::locate::{
    KnownRepos, LocateContext, Located, SessionInfoError, SessionInfoReply, SessionInfoSource,
    locate, locate_folder,
};
use talaria_app::worktree::GitRunner;
use talaria_core::directory::rows::RowKey;
use talaria_core::review::{RecordView, ReviewTarget, Scope};

fn frames_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/frames")
}

fn skip_unless_git() -> bool {
    if git_available() {
        return false;
    }
    eprintln!("git not found on PATH");
    true
}

fn row(profile: &str, session_id: &str) -> RowKey {
    RowKey {
        profile: profile.to_owned(),
        session_id: session_id.to_owned(),
    }
}

/// A source that reads `session.info` over a launch connection, adding
/// `profile` only when the row's profile differs from the link's.
struct GatewaySource<'a> {
    conn: &'a LaunchConn,
}

impl SessionInfoSource for GatewaySource<'_> {
    fn info<'a>(
        &'a self,
        row: &'a RowKey,
    ) -> Pin<Box<dyn Future<Output = Result<SessionInfoReply, SessionInfoError>> + Send + 'a>> {
        Box::pin(async move {
            let mut params = serde_json::Map::new();
            params.insert("session_id".to_owned(), json!(row.session_id));
            if let Some(profile) = self.conn.routed_profile(&row.profile) {
                params.insert("profile".to_owned(), json!(profile));
            }
            let reply = self
                .conn
                .call(
                    "session.info",
                    Value::Object(params),
                    Duration::from_secs(10),
                )
                .await
                .map_err(|failure| match failure {
                    CallFailure::Refused {
                        code: 4001,
                        reason: Some(reason),
                        ..
                    } if reason == "not_found" => SessionInfoError::NotFound,
                    CallFailure::Refused {
                        code: 4001, reason, ..
                    } => SessionInfoError::Refused(reason.unwrap_or_default()),
                    other => SessionInfoError::Other(format!("{other:?}")),
                })?;
            Ok(SessionInfoReply {
                cwd: reply["cwd"].as_str().map(str::to_owned),
                model: reply["model"].as_str().map(str::to_owned),
                lazy: reply["lazy"].as_bool().unwrap_or(true),
                branch: reply["branch"].as_str().map(str::to_owned),
            })
        })
    }
}

/// Read the recorded `session.info` frame, with `<tmp>` pointing at `dir`.
fn recorded_info(dir: &std::path::Path) -> Value {
    let path = frames_dir().join("review-session-info.jsonl");
    let Ok(text) = std::fs::read_to_string(&path) else {
        panic!("BLOCKED: fixtures/frames/review-session-info.jsonl not recorded");
    };
    // The recording is a whole session: a header, then one frame per line. The reply
    // is the received frame with the id of the sent `session.info` request.
    let frames: Vec<Value> = text
        .lines()
        .filter_map(|line| serde_json::from_str::<Value>(line).ok())
        .filter(|line| line["kind"] == "frame")
        .collect();
    let request = frames
        .iter()
        .find(|line| line["dir"] == "out" && line["frame"]["method"] == "session.info")
        .expect("a session.info request frame");
    let id = request["frame"]["id"].clone();
    let reply = frames
        .iter()
        .find(|line| line["dir"] == "in" && line["frame"]["id"] == id)
        .expect("a session.info reply frame");
    let mut info = reply["frame"]["result"].clone();
    if info["cwd"] == json!("<tmp>") {
        info["cwd"] = json!(dir.to_string_lossy());
    }
    info
}

/// Record, then the foreground row's own folder, then `session.info`.
#[tokio::test]
async fn record_then_foreground_then_session_info() {
    if skip_unless_git() {
        return;
    }
    let git = GitRepo::new();
    let home = tempfile::tempdir().expect("home");
    let gateway = StandaloneGateway::start(home.path());
    let link = LaunchLink::open(home.path(), "fixture")
        .await
        .expect("link");
    let conn = LaunchConn::Link(link);
    let source = GatewaySource { conn: &conn };
    let runner = GitRunner::from_env();
    let repos = KnownRepos::new();
    let foreground = row("fixture", "local-foreground");

    // The recorded folder answers through `session.info`.
    gateway.set_session_info(recorded_info(&git.repo));
    let ctx = LocateContext {
        git: &runner,
        known_roots: &repos,
        foreground: None,
        info: &source,
    };
    let found = locate(&row("fixture", "local-recorded"), &ctx).await;
    assert!(
        matches!(found, Located::Found(_)),
        "session.info reports the folder: {found:?}"
    );
    let seen = gateway.requests("session.info");
    assert_eq!(seen.len(), 1, "one lookup call");
    let keys: BTreeSet<String> = seen[0]
        .params
        .as_object()
        .expect("object params")
        .keys()
        .cloned()
        .collect();
    assert!(
        keys == BTreeSet::from(["session_id".to_owned()])
            || keys == BTreeSet::from(["session_id".to_owned(), "profile".to_owned()]),
        "exactly session_id, plus profile when routed: {keys:?}"
    );
    assert_eq!(
        seen[0].params["session_id"], "local-recorded",
        "the row's own session"
    );

    // The foreground row's own folder wins without a call.
    let launch_folder = git.repo.to_string_lossy().into_owned();
    let ctx = LocateContext {
        git: &runner,
        known_roots: &repos,
        foreground: Some((&foreground, Some(launch_folder.as_str()))),
        info: &source,
    };
    let found = locate(&foreground, &ctx).await;
    assert!(
        matches!(found, Located::Found(_)),
        "the foreground folder wins: {found:?}"
    );
    assert_eq!(
        gateway.requests("session.info").len(),
        1,
        "no second lookup call"
    );

    // Another row never borrows the foreground folder.
    let found = locate(&row("fixture", "local-other"), &ctx).await;
    assert!(
        matches!(found, Located::Found(_)),
        "other rows look up their own folder: {found:?}"
    );
    assert_eq!(
        gateway.requests("session.info").len(),
        2,
        "other rows call session.info"
    );

    // A null folder reports no folder.
    let mut info = recorded_info(&git.repo);
    info["cwd"] = Value::Null;
    gateway.set_session_info(info);
    let found = locate(&row("fixture", "local-null"), &ctx).await;
    assert_eq!(
        found,
        Located::NotFound("Talaria cannot find this session's folder.".to_owned())
    );

    // A refusal other than `not_found` also reports no folder.
    gateway.queue_behavior(
        "session.info",
        Behavior::RefuseCode {
            reason: "permission_denied".to_owned(),
            code: 4001,
        },
    );
    let found = locate(&row("fixture", "local-refused"), &ctx).await;
    assert_eq!(
        found,
        Located::NotFound("Talaria cannot find this session's folder.".to_owned())
    );

    // Never Talaria's own folder.
    let own = std::env::current_dir().expect("own folder");
    let mut info = recorded_info(&git.repo);
    info["cwd"] = json!(own.to_string_lossy());
    gateway.set_session_info(info);
    let found = locate(&row("fixture", "local-own"), &ctx).await;
    assert_eq!(
        found,
        Located::NotFound("Talaria cannot find this session's folder.".to_owned()),
        "Talaria's own folder is refused"
    );
}

/// Each entry point reports a missing folder in its own words.
#[tokio::test]
async fn missing_folder_texts() {
    if skip_unless_git() {
        return;
    }
    let git = GitRepo::new();
    let home = tempfile::tempdir().expect("home");
    let gateway = StandaloneGateway::start(home.path());
    let link = LaunchLink::open(home.path(), "fixture")
        .await
        .expect("link");
    let conn = LaunchConn::Link(link);
    let source = GatewaySource { conn: &conn };
    let runner = GitRunner::from_env();
    let repos = KnownRepos::new();
    let ctx = LocateContext {
        git: &runner,
        known_roots: &repos,
        foreground: None,
        info: &source,
    };

    // A row with no folder anywhere.
    gateway.set_session_info(
        json!({"cwd": Value::Null, "model": Value::Null, "lazy": true, "branch": Value::Null}),
    );
    let found = locate(&row("fixture", "local-lost"), &ctx).await;
    assert_eq!(
        found,
        Located::NotFound("Talaria cannot find this session's folder.".to_owned())
    );

    // A session the gateway no longer knows.
    gateway.queue_behavior(
        "session.info",
        Behavior::RefuseCode {
            reason: "not_found".to_owned(),
            code: 4001,
        },
    );
    let found = locate(&row("fixture", "local-gone"), &ctx).await;
    assert_eq!(
        found,
        Located::NotFound("This session no longer exists on the gateway.".to_owned())
    );

    // A folder missing on this machine.
    let missing = git.path().join("missing");
    let found = locate_folder(&runner, &missing, None, "missing".to_owned()).await;
    assert_eq!(
        found,
        Located::NotFound(format!(
            "The folder {} does not exist on this machine.",
            missing.to_string_lossy()
        ))
    );

    // A folder outside any repository.
    let plain = tempfile::tempdir().expect("plain folder");
    let found = locate_folder(&runner, plain.path(), None, "plain".to_owned()).await;
    assert_eq!(
        found,
        Located::NotFound(format!(
            "{} is not inside a git repository.",
            plain.path().to_string_lossy()
        ))
    );
}

/// A source that must not be asked: the record answers first.
struct NeverAsked;

impl SessionInfoSource for NeverAsked {
    fn info<'a>(
        &'a self,
        _row: &'a RowKey,
    ) -> Pin<Box<dyn Future<Output = Result<SessionInfoReply, SessionInfoError>> + Send + 'a>> {
        Box::pin(async { Err(SessionInfoError::Other("asked".to_owned())) })
    }
}

/// A recorded tree answers before any call. A tree gone from disk cannot be
/// resolved, so the record is skipped and the lookup falls through.
#[tokio::test]
async fn recorded_tree_answers_first_and_a_missing_one_falls_through() {
    if skip_unless_git() {
        return;
    }
    let git = GitRepo::new();
    let repo = git.repo.clone();
    git.ok(
        &repo,
        &[
            "worktree",
            "add",
            "--no-track",
            "-b",
            "talaria/rec",
            ".worktrees/talaria-rec",
            "main",
        ],
    );
    let base = git.ok(&repo, &["rev-parse", "main"]);
    for (key, value) in [
        ("path", ".worktrees/talaria-rec"),
        ("base", "origin/main"),
        ("base-commit", base.as_str()),
        ("profile", "fixture"),
        ("session", "local-rec"),
    ] {
        git.ok(
            &repo,
            &[
                "config",
                "--local",
                &format!("branch.talaria/rec.talaria-{key}"),
                value,
            ],
        );
    }

    let runner = GitRunner::from_env();
    let mut repos = KnownRepos::new();
    repos.insert(repo.clone());
    let ctx = LocateContext {
        git: &runner,
        known_roots: &repos,
        foreground: None,
        info: &NeverAsked,
    };
    let found = locate(&row("fixture", "local-rec"), &ctx).await;
    let Located::Found(target) = found else {
        panic!("the record answers: {found:?}");
    };
    assert_eq!(target.folder, repo.join(".worktrees/talaria-rec"));
    assert_eq!(target.main_root, repo);
    let record = target.record.expect("the record rides along");
    assert_eq!(record.branch, "talaria/rec");
    assert_eq!(record.base_commit, base);

    // The same session id in another profile is another session.
    let other = locate(&row("default", "local-rec"), &ctx).await;
    assert_eq!(
        other,
        Located::NotFound("Talaria cannot find this session's folder.".to_owned())
    );

    // The tree is gone from disk: resolution fails closed and the lookup
    // falls through instead of opening the recorded path.
    git.ok(
        &repo,
        &["worktree", "remove", "--force", ".worktrees/talaria-rec"],
    );
    let gone = locate(&row("fixture", "local-rec"), &ctx).await;
    assert_eq!(
        gone,
        Located::NotFound("Talaria cannot find this session's folder.".to_owned())
    );
}

const NO_FOLDER: &str = "Talaria cannot find this session's folder.";

/// Write the keys `records_in` needs. `path` is stored as git config text.
fn write_record(git: &GitRepo, branch: &str, path: &str, session: &str) {
    for (key, value) in [
        ("path", path),
        ("base", "origin/main"),
        ("base-commit", "0123456789abcdef0123456789abcdef01234567"),
        ("profile", "fixture"),
        ("session", session),
    ] {
        git.ok(
            &git.repo,
            &[
                "config",
                "--local",
                &format!("branch.{branch}.talaria-{key}"),
                value,
            ],
        );
    }
}

async fn locate_recorded(git: &GitRepo, session: &str) -> Located {
    let runner = GitRunner::from_env();
    let mut repos = KnownRepos::new();
    repos.insert(git.repo.clone());
    let ctx = LocateContext {
        git: &runner,
        known_roots: &repos,
        foreground: None,
        info: &NeverAsked,
    };
    locate(&row("fixture", session), &ctx).await
}

/// An absolute `talaria-path` outside `<main>/.worktrees` is not opened.
#[tokio::test]
async fn absolute_record_path_outside_worktrees_is_refused() {
    if skip_unless_git() {
        return;
    }
    let git = GitRepo::new();
    let outside = git.path().join("outside-review");
    std::fs::create_dir(&outside).expect("outside folder");
    write_record(&git, "talaria/abs", &outside.to_string_lossy(), "local-abs");
    let found = locate_recorded(&git, "local-abs").await;
    assert_eq!(
        found,
        Located::NotFound(NO_FOLDER.to_owned()),
        "an absolute path outside .worktrees must not open"
    );
}

/// `<main>/.worktrees/evil` pointing outside the repository is not opened.
#[tokio::test]
async fn symlinked_worktree_path_outside_worktrees_is_refused() {
    if skip_unless_git() {
        return;
    }
    let git = GitRepo::new();
    let outside = git.path().join("elsewhere");
    std::fs::create_dir(&outside).expect("outside folder");
    std::fs::create_dir(git.repo.join(".worktrees")).expect("worktrees");
    std::os::unix::fs::symlink(&outside, git.repo.join(".worktrees/evil")).expect("symlink");
    write_record(&git, "talaria/evil", ".worktrees/evil", "local-evil");
    let found = locate_recorded(&git, "local-evil").await;
    assert_eq!(
        found,
        Located::NotFound(NO_FOLDER.to_owned()),
        "a symlink out of .worktrees must not open"
    );
}

/// A `.worktrees` directory that is itself a symlink is not opened.
#[tokio::test]
async fn symlinked_worktrees_directory_is_refused() {
    if skip_unless_git() {
        return;
    }
    let git = GitRepo::new();
    let outside = git.path().join("linked-trees");
    std::fs::create_dir_all(outside.join("talaria-linked")).expect("outside tree");
    std::os::unix::fs::symlink(&outside, git.repo.join(".worktrees")).expect("symlink");
    write_record(
        &git,
        "talaria/linked",
        ".worktrees/talaria-linked",
        "local-linked",
    );
    let found = locate_recorded(&git, "local-linked").await;
    assert_eq!(
        found,
        Located::NotFound(NO_FOLDER.to_owned()),
        "a symlinked .worktrees directory must not open"
    );
}

/// A git linked worktree directly under `<main>/.worktrees` still opens.
#[tokio::test]
async fn legitimate_worktree_record_opens_review() {
    if skip_unless_git() {
        return;
    }
    let git = GitRepo::new();
    let repo = git.repo.clone();
    git.ok(
        &repo,
        &[
            "worktree",
            "add",
            "--no-track",
            "-b",
            "talaria/ok",
            ".worktrees/talaria-ok",
            "main",
        ],
    );
    let tree = repo.join(".worktrees/talaria-ok");
    write_record(&git, "talaria/ok", ".worktrees/talaria-ok", "local-ok");
    let found = locate_recorded(&git, "local-ok").await;
    let Located::Found(target) = found else {
        panic!("a linked worktree record opens: {found:?}");
    };
    assert_eq!(target.folder, std::fs::canonicalize(&tree).expect("tree"));
    assert_eq!(target.main_root, repo);
    let record = target.record.as_ref().expect("the linked worktree record");
    assert_eq!(record.branch, "talaria/ok");
    list(&GitRunner::from_env(), &target, Scope::Uncommitted)
        .await
        .expect("the linked worktree lists");

    let via_folder = locate_folder(&GitRunner::from_env(), &tree, None, String::new()).await;
    let Located::Found(via_folder) = via_folder else {
        panic!("the same linked worktree opened by folder keeps its record: {via_folder:?}");
    };
    assert_eq!(
        via_folder
            .record
            .as_ref()
            .map(|record| record.branch.as_str()),
        Some("talaria/ok")
    );
}

/// A plain directory under `.worktrees` is not a linked worktree.
#[tokio::test]
async fn plain_directory_under_worktrees_is_refused() {
    if skip_unless_git() {
        return;
    }
    let git = GitRepo::new();
    let plain = git.repo.join(".worktrees/plain");
    std::fs::create_dir_all(&plain).expect("plain directory");
    write_record(&git, "talaria/plain", ".worktrees/plain", "local-plain");
    let found = locate_recorded(&git, "local-plain").await;
    assert_eq!(
        found,
        Located::NotFound(format!(
            "The worktree {} is not a linked worktree of {}.",
            git.repo.join(".worktrees/plain").to_string_lossy(),
            git.repo.to_string_lossy(),
        )),
        "a plain directory must not open as the worktree record"
    );

    let via_folder = locate_folder(&GitRunner::from_env(), &plain, None, String::new()).await;
    let Located::Found(via_folder) = via_folder else {
        panic!("the caller can still open the folder: {via_folder:?}");
    };
    assert!(
        via_folder.record.is_none(),
        "a plain directory must not keep the record's base commit: {via_folder:?}"
    );
}

/// An out-of-scope record is not attached when the folder is chosen another way.
#[tokio::test]
async fn fallback_lookup_ignores_an_uncontained_record() {
    if skip_unless_git() {
        return;
    }
    let git = GitRepo::new();
    let repo = git.repo.clone();
    let attacker = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    let outside = repo.to_string_lossy().into_owned();
    for (key, value) in [
        ("path", outside.as_str()),
        ("base", "origin/main"),
        ("base-commit", attacker),
        ("profile", "fixture"),
        ("session", "local-fallback"),
    ] {
        git.ok(
            &repo,
            &[
                "config",
                "--local",
                &format!("branch.talaria/fallback.talaria-{key}"),
                value,
            ],
        );
    }
    let runner = GitRunner::from_env();
    let mut repos = KnownRepos::new();
    repos.insert(repo.clone());
    let asked = row("fixture", "local-fallback");
    let launch = repo.to_string_lossy();
    let ctx = LocateContext {
        git: &runner,
        known_roots: &repos,
        foreground: Some((&asked, Some(launch.as_ref()))),
        info: &NeverAsked,
    };
    let found = locate(&asked, &ctx).await;
    let Located::Found(target) = found else {
        panic!("the foreground folder still opens: {found:?}");
    };
    assert_eq!(target.folder, repo);
    assert!(
        target.record.is_none(),
        "an absolute record must not supply the branch base: {target:?}"
    );
    let listing = list(&runner, &target, Scope::Branch)
        .await
        .expect("the foreground folder lists");
    assert_ne!(listing.base.as_deref(), Some(attacker));

    let via_folder = locate_folder(&runner, &repo, None, String::new()).await;
    let Located::Found(via_folder) = via_folder else {
        panic!("an explicit folder still opens: {via_folder:?}");
    };
    assert!(via_folder.record.is_none());
}

#[tokio::test]
async fn record_backed_target_outside_worktrees_is_refused() {
    if skip_unless_git() {
        return;
    }
    let git = GitRepo::new();
    let outside = git.path().join("outside");
    git.ok(
        &git.repo,
        &[
            "worktree",
            "add",
            outside.to_str().expect("path"),
            "-b",
            "outside",
        ],
    );
    let runner = GitRunner::from_env();
    let target = ReviewTarget {
        folder: outside.canonicalize().expect("outside worktree"),
        main_root: git.repo.canonicalize().expect("main worktree"),
        session: None,
        record: Some(RecordView {
            branch: "outside".into(),
            base_ref: "origin/main".into(),
            base_commit: "a".repeat(40),
        }),
        title: "outside".into(),
    };
    assert!(list(&runner, &target, Scope::Uncommitted).await.is_err());
}

/// Replacing the linked worktree after it opens stops later git operations.
#[tokio::test]
async fn record_backed_review_stops_when_the_worktree_changes() {
    if skip_unless_git() {
        return;
    }
    let git = GitRepo::new();
    let repo = git.repo.clone();
    git.ok(
        &repo,
        &[
            "worktree",
            "add",
            "--no-track",
            "-b",
            "talaria/bound",
            ".worktrees/talaria-bound",
            "main",
        ],
    );
    write_record(
        &git,
        "talaria/bound",
        ".worktrees/talaria-bound",
        "local-bound",
    );
    let found = locate_recorded(&git, "local-bound").await;
    let Located::Found(target) = found else {
        panic!("the linked worktree opens: {found:?}");
    };
    git.ok(
        &repo,
        &["worktree", "remove", "--force", ".worktrees/talaria-bound"],
    );
    std::os::unix::fs::symlink(&repo, repo.join(".worktrees/talaria-bound")).expect("symlink");
    let listed = list(&GitRunner::from_env(), &target, Scope::Uncommitted).await;
    assert!(
        listed.is_err(),
        "a replaced worktree must not be read: {listed:?}"
    );
}
