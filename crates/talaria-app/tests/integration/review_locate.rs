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
use talaria_app::review::locate::{
    KnownRepos, LocateContext, Located, SessionInfoError, SessionInfoReply, SessionInfoSource,
    locate, locate_folder,
};
use talaria_app::worktree::GitRunner;
use talaria_core::directory::rows::RowKey;

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
    let mut repos = KnownRepos::new();
    repos.insert(std::fs::canonicalize(&git.repo).expect("canonical repo"));
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

/// A recorded tree answers before any call; a tree gone from disk says so.
#[tokio::test]
async fn recorded_tree_answers_first_and_a_missing_one_says_so() {
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

    // The tree is gone from disk.
    git.ok(
        &repo,
        &["worktree", "remove", "--force", ".worktrees/talaria-rec"],
    );
    let gone = locate(&row("fixture", "local-rec"), &ctx).await;
    assert_eq!(
        gone,
        Located::NotFound(format!(
            "The worktree {} is gone. Hermes may have archived it: run hermes worktree restore talaria-rec in {}, then press r.",
            repo.join(".worktrees/talaria-rec").to_string_lossy(),
            repo.to_string_lossy()
        ))
    );
}

/// A `session.info` answer whose `cwd` is fixed, with no gateway.
struct FixedInfo {
    cwd: String,
}

impl SessionInfoSource for FixedInfo {
    fn info<'a>(
        &'a self,
        _row: &'a RowKey,
    ) -> Pin<Box<dyn Future<Output = Result<SessionInfoReply, SessionInfoError>> + Send + 'a>> {
        let cwd = self.cwd.clone();
        Box::pin(async move {
            Ok(SessionInfoReply {
                cwd: Some(cwd),
                model: None,
                lazy: true,
                branch: None,
            })
        })
    }
}

fn trusted(root: &std::path::Path) -> KnownRepos {
    let mut repos = KnownRepos::new();
    repos.insert(root.to_path_buf());
    repos
}

fn refused(found: &Located) -> bool {
    matches!(
        found,
        Located::NotFound(text) if text == "Talaria cannot find this session's folder."
    )
}

/// The foreground row's gateway `cwd` is not a review target when it sits outside
/// every locally trusted root.
#[tokio::test]
async fn review_foreground_cwd_outside_trusted_roots_is_refused() {
    if skip_unless_git() {
        return;
    }
    let git = GitRepo::new();
    let launch = std::fs::canonicalize(&git.repo).expect("canonical launch folder");
    let hostile = git.path().join("other");
    let repos = trusted(&launch);
    let runner = GitRunner::from_env();
    let key = row("fixture", "local-foreground");
    let cwd = hostile.to_string_lossy().into_owned();
    let info = FixedInfo { cwd: cwd.clone() };
    let ctx = LocateContext {
        git: &runner,
        known_roots: &repos,
        foreground: Some((&key, Some(cwd.as_str()))),
        info: &info,
    };
    let found = locate(&key, &ctx).await;
    assert!(
        refused(&found),
        "a foreground cwd outside trusted roots must not build a ReviewTarget: {found:?}"
    );
}

/// `session.info.cwd` outside every locally trusted root is not a review target.
#[tokio::test]
async fn review_session_info_cwd_outside_trusted_roots_is_refused() {
    if skip_unless_git() {
        return;
    }
    let git = GitRepo::new();
    let launch = std::fs::canonicalize(&git.repo).expect("canonical launch folder");
    let hostile = git.path().join("other");
    let repos = trusted(&launch);
    let runner = GitRunner::from_env();
    let info = FixedInfo {
        cwd: hostile.to_string_lossy().into_owned(),
    };
    let ctx = LocateContext {
        git: &runner,
        known_roots: &repos,
        foreground: None,
        info: &info,
    };
    let found = locate(&row("fixture", "local-moved"), &ctx).await;
    assert!(
        refused(&found),
        "session.info cwd outside trusted roots must not build a ReviewTarget: {found:?}"
    );
}

/// A symlink whose target leaves the trusted root is refused, even when the link
/// itself sits inside that root.
#[tokio::test]
async fn review_symlink_cwd_outside_trusted_roots_is_refused() {
    if skip_unless_git() {
        return;
    }
    let git = GitRepo::new();
    let launch = std::fs::canonicalize(&git.repo).expect("canonical launch folder");
    let outside = git.path().join("other");
    let link = launch.join("escape");
    std::os::unix::fs::symlink(&outside, &link).expect("symlink");
    let repos = trusted(&launch);
    let runner = GitRunner::from_env();
    let info = FixedInfo {
        cwd: link.to_string_lossy().into_owned(),
    };
    let ctx = LocateContext {
        git: &runner,
        known_roots: &repos,
        foreground: None,
        info: &info,
    };
    let found = locate(&row("fixture", "local-link"), &ctx).await;
    assert!(
        refused(&found),
        "a symlink that resolves outside trusted roots must not build a ReviewTarget: {found:?}"
    );
}

/// `/trusted-evil` is not inside `/trusted`: containment is by path component.
#[tokio::test]
async fn review_sibling_prefix_cwd_is_refused() {
    if skip_unless_git() {
        return;
    }
    let git = GitRepo::new();
    let trusted_root = git.path().join("trusted");
    let sibling = git.path().join("trusted-evil");
    git.ok(git.path(), &["init", "-b", "main", "trusted"]);
    git.ok(git.path(), &["init", "-b", "main", "trusted-evil"]);
    let trusted_root = std::fs::canonicalize(&trusted_root).expect("canonical trusted");
    let sibling = std::fs::canonicalize(&sibling).expect("canonical sibling");
    let sibling_text = sibling.to_string_lossy();
    let trusted_text = trusted_root.to_string_lossy();
    assert!(
        sibling_text.starts_with(trusted_text.as_ref()),
        "the fixture must be a string prefix so a naive check would accept it"
    );
    let repos = trusted(&trusted_root);
    let runner = GitRunner::from_env();
    let info = FixedInfo {
        cwd: sibling.to_string_lossy().into_owned(),
    };
    let ctx = LocateContext {
        git: &runner,
        known_roots: &repos,
        foreground: None,
        info: &info,
    };
    let found = locate(&row("fixture", "local-sibling"), &ctx).await;
    assert!(
        refused(&found),
        "a sibling whose name shares a prefix must not build a ReviewTarget: {found:?}"
    );

    let key = row("fixture", "local-sibling-fg");
    let cwd = sibling.to_string_lossy().into_owned();
    let ctx = LocateContext {
        git: &runner,
        known_roots: &repos,
        foreground: Some((&key, Some(cwd.as_str()))),
        info: &info,
    };
    let found = locate(&key, &ctx).await;
    assert!(
        refused(&found),
        "foreground sibling prefix must not build a ReviewTarget: {found:?}"
    );
}

/// The launch folder opens, and the review target keeps the canonical path that
/// passed the trust check rather than the gateway's original spelling.
#[tokio::test]
async fn review_launch_folder_opens_on_its_canonical_path() {
    if skip_unless_git() {
        return;
    }
    let git = GitRepo::new();
    let launch = std::fs::canonicalize(&git.repo).expect("canonical launch folder");
    let indirect = launch
        .join("..")
        .join(launch.file_name().expect("launch name"));
    assert_ne!(
        indirect, launch,
        "the gateway spelling differs from the canonical path"
    );
    assert_eq!(
        std::fs::canonicalize(&indirect).expect("indirect canonical"),
        launch
    );
    let repos = trusted(&launch);
    let runner = GitRunner::from_env();
    let key = row("fixture", "local-launch");
    let cwd = indirect.to_string_lossy().into_owned();
    let info = FixedInfo { cwd: cwd.clone() };

    let ctx = LocateContext {
        git: &runner,
        known_roots: &repos,
        foreground: Some((&key, Some(cwd.as_str()))),
        info: &info,
    };
    let found = locate(&key, &ctx).await;
    let Located::Found(target) = found else {
        panic!("the launch folder opens from the foreground row: {found:?}");
    };
    assert_eq!(
        target.folder, launch,
        "the review keeps the canonical path that passed containment"
    );

    let ctx = LocateContext {
        git: &runner,
        known_roots: &repos,
        foreground: None,
        info: &info,
    };
    let found = locate(&row("fixture", "local-launch-info"), &ctx).await;
    let Located::Found(target) = found else {
        panic!("the launch folder opens from session.info: {found:?}");
    };
    assert_eq!(target.folder, launch);

    let alias = git.path().join("launch-alias");
    std::os::unix::fs::symlink(&launch, &alias).expect("alias");
    let info = FixedInfo {
        cwd: alias.to_string_lossy().into_owned(),
    };
    let ctx = LocateContext {
        git: &runner,
        known_roots: &repos,
        foreground: None,
        info: &info,
    };
    let found = locate(&row("fixture", "local-alias"), &ctx).await;
    let Located::Found(target) = found else {
        panic!("a symlink that resolves to the launch folder opens: {found:?}");
    };
    assert_eq!(target.folder, launch);
    assert_ne!(target.folder, alias);
}

fn write_record(git: &GitRepo, repo: &std::path::Path, branch: &str, path: &str, session: &str) {
    for (key, value) in [("path", path), ("profile", "fixture"), ("session", session)] {
        let name = format!("branch.{branch}.talaria-{key}");
        git.ok(repo, &["config", "--local", &name, value]);
    }
}

/// A linked checkout inside the trusted folder whose main checkout is outside
/// is not a review target, from either gateway cwd.
#[tokio::test]
async fn review_linked_worktree_outside_main_is_refused() {
    if skip_unless_git() {
        return;
    }
    let git = GitRepo::new();
    let trusted_root = std::fs::canonicalize(&git.repo).expect("trusted");
    let outside = git.path().join("other");
    let linked = trusted_root.join("linked");
    let linked_arg = linked.to_string_lossy().into_owned();
    git.ok(
        &outside,
        &[
            "worktree",
            "add",
            "--no-track",
            "-b",
            "talaria/sneak",
            &linked_arg,
            "main",
        ],
    );
    let repos = trusted(&trusted_root);
    let runner = GitRunner::from_env();
    let cwd = linked.to_string_lossy().into_owned();
    let info = FixedInfo { cwd: cwd.clone() };
    let key = row("fixture", "local-linked");
    let ctx = LocateContext {
        git: &runner,
        known_roots: &repos,
        foreground: Some((&key, Some(cwd.as_str()))),
        info: &info,
    };
    let found = locate(&key, &ctx).await;
    assert!(
        refused(&found),
        "foreground linked checkout with an outside main must not open: {found:?}"
    );
    let ctx = LocateContext {
        git: &runner,
        known_roots: &repos,
        foreground: None,
        info: &info,
    };
    let found = locate(&row("fixture", "local-linked-info"), &ctx).await;
    assert!(
        refused(&found),
        "session.info linked checkout with an outside main must not open: {found:?}"
    );
}

/// A record whose path is outside the trusted root does not open that checkout.
#[tokio::test]
async fn review_record_outside_trusted_root_is_refused() {
    if skip_unless_git() {
        return;
    }
    let git = GitRepo::new();
    let outside = git.path().join("other");
    write_record(
        &git,
        &git.repo,
        "talaria/evil",
        &outside.to_string_lossy(),
        "local-evil",
    );
    let repos = trusted(&git.repo);
    let runner = GitRunner::from_env();
    let ctx = LocateContext {
        git: &runner,
        known_roots: &repos,
        foreground: None,
        info: &NeverAsked,
    };
    let found = locate(&row("fixture", "local-evil"), &ctx).await;
    assert!(
        refused(&found),
        "an outside record path must not open a review: {found:?}"
    );
}

/// A record under a symlinked `.worktrees` does not open the link target.
#[tokio::test]
async fn review_record_symlinked_worktrees_is_refused() {
    if skip_unless_git() {
        return;
    }
    let git = GitRepo::new();
    git.ok(git.path(), &["init", "-b", "main", "elsewhere/evil"]);
    std::os::unix::fs::symlink(git.path().join("elsewhere"), git.repo.join(".worktrees"))
        .expect("symlink");
    write_record(
        &git,
        &git.repo,
        "talaria/evil",
        ".worktrees/evil",
        "local-evil",
    );
    let repos = trusted(&git.repo);
    let runner = GitRunner::from_env();
    let ctx = LocateContext {
        git: &runner,
        known_roots: &repos,
        foreground: None,
        info: &NeverAsked,
    };
    let found = locate(&row("fixture", "local-evil"), &ctx).await;
    assert!(
        refused(&found),
        "a symlinked .worktrees record must not open a review: {found:?}"
    );
}

/// Fallback record attachment ignores an outside path and a symlink that
/// resolves to the folder being opened.
#[tokio::test]
async fn review_record_at_ignores_outside_and_symlinked_records() {
    if skip_unless_git() {
        return;
    }
    let git = GitRepo::new();
    let launch = std::fs::canonicalize(&git.repo).expect("launch");
    let outside = git.path().join("other");
    write_record(
        &git,
        &launch,
        "talaria/outside",
        &outside.to_string_lossy(),
        "other-session",
    );
    std::fs::create_dir(launch.join(".worktrees")).expect("worktrees");
    std::os::unix::fs::symlink(&launch, launch.join(".worktrees/alias")).expect("alias");
    write_record(
        &git,
        &launch,
        "talaria/pwned",
        ".worktrees/alias",
        "another-session",
    );
    let repos = trusted(&launch);
    let runner = GitRunner::from_env();
    let info = FixedInfo {
        cwd: launch.to_string_lossy().into_owned(),
    };
    let ctx = LocateContext {
        git: &runner,
        known_roots: &repos,
        foreground: None,
        info: &info,
    };
    let found = locate(&row("fixture", "local-open"), &ctx).await;
    let Located::Found(target) = found else {
        panic!("the trusted folder still opens: {found:?}");
    };
    assert_eq!(target.folder, launch);
    assert_ne!(target.folder, outside);
    assert!(
        target.record.is_none(),
        "neither the outside record nor the symlink record attaches: {:?}",
        target.record
    );
}
