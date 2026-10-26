//! Live drafting and folder tests for the review. Issue #245.
//!
//! Ignored unless someone asks for it. A skip prints
//! `LIVE_REVIEW_RESULT: SKIPPED (<reason>)`; with `TALARIA_LIVE_REQUIRE=1` a
//! skip fails.

#[path = "support/git_fixture.rs"]
mod git_fixture;

use std::path::PathBuf;
use std::time::Duration;

use git_fixture::{GitRepo, git_available};
use hermes_client::{DiscoverOptions, EnsurePolicy, default_home, discover_with};
use serde_json::json;
use talaria_app::bg::wire::{CREATE_TIMEOUT, create_params, read_created};
use talaria_app::bg::{LaunchConn, LaunchLink, NewSessionSpec};
use talaria_app::review::draft::draft_message;
use talaria_core::directory::rows::RowKey;

fn home() -> PathBuf {
    std::env::var_os("HERMES_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(default_home)
}

fn skip(reason: &str) {
    println!("LIVE_REVIEW_RESULT: SKIPPED ({reason})");
    if std::env::var("TALARIA_LIVE_REQUIRE").ok().as_deref() == Some("1") {
        panic!("live test skipped: {reason}");
    }
}

async fn reachable() -> bool {
    let options = DiscoverOptions {
        control_timeout: Duration::from_secs(5),
        ensure: EnsurePolicy::Never,
    };
    let endpoint = match discover_with(&home(), &options).await {
        Ok(endpoint) => endpoint,
        Err(_) => {
            skip("no Hermes gateway reachable");
            return false;
        }
    };
    if let Some(reason) = hermes_contract::pinned::live_skip_reason(endpoint.code_sha()) {
        skip(&reason);
        return false;
    }
    if endpoint.resolve_profile("fixture").is_err() {
        skip("the fixture profile is not served");
        return false;
    }
    true
}

async fn create_in(conn: &LaunchConn, cwd: &std::path::Path, request_id: &str) -> String {
    let spec = NewSessionSpec {
        request_id: request_id.to_owned(),
        cwd: cwd.to_owned(),
        model: None,
        provider: None,
        reasoning: None,
    };
    let profile = conn.routed_profile("fixture");
    let reply = conn
        .call(
            "session.create",
            create_params(&spec, profile.as_deref()),
            CREATE_TIMEOUT,
        )
        .await
        .expect("session.create answers");
    read_created(&reply).expect("a created session").session_id
}

/// A live `llm.oneshot` on a second link drafts a message; no prompt is ever
/// submitted and the session is never detached.
#[tokio::test]
#[ignore = "needs a Hermes gateway on the fixture profile"]
async fn draft_message_from_session_model() {
    if !git_available() {
        skip("git not found on PATH");
        return;
    }
    if !reachable().await {
        return;
    }
    let git = GitRepo::new();
    let link_a = LaunchLink::open(&home(), "fixture").await.expect("link A");
    let conn_a = LaunchConn::Link(link_a);
    let session_id = create_in(&conn_a, &git.repo, &talaria_app::launch::new_request_id()).await;

    std::fs::write(git.repo.join("draft.txt"), "draft me\n").expect("change file");
    let diff = git.ok(&git.repo, &["diff", "HEAD", "--", "draft.txt"]);
    let recent = git.ok(&git.repo, &["log", "-n", "10", "--pretty=format:%s"]);

    // Link B has never attached the session.
    let link_b = LaunchLink::open(&home(), "fixture").await.expect("link B");
    let conn_b = LaunchConn::Link(link_b);
    let info = conn_b
        .call(
            "session.info",
            json!({"session_id": session_id}),
            Duration::from_secs(10),
        )
        .await
        .expect("session.info answers");
    let lazy = info["lazy"].as_bool().unwrap_or(true);
    let cwd = info["cwd"].as_str();
    let model = info["model"].as_str().unwrap_or_default();
    assert!(
        (!lazy || cwd.is_some()) && !model.is_empty(),
        "the session can draft: {info}"
    );

    let row = RowKey {
        profile: "fixture".to_owned(),
        session_id: session_id.clone(),
    };
    let text = draft_message(&conn_b, &row, &diff, &recent, "")
        .await
        .expect("the draft arrives");
    assert!(!text.trim().is_empty(), "the draft is non-empty");
    let first = text.lines().next().unwrap_or_default();
    assert!(
        first.chars().count() <= 100,
        "the first line is short: {first}"
    );
    println!("draft: {text}");

    conn_b.close().await;
    conn_a.close().await;
}

/// A routed `session.info` reports the session's folder.
#[tokio::test]
#[ignore = "needs a Hermes gateway on the fixture profile"]
async fn session_info_reports_folder() {
    if !git_available() {
        skip("git not found on PATH");
        return;
    }
    if !reachable().await {
        return;
    }
    let git = GitRepo::new();
    let link = LaunchLink::open(&home(), "fixture").await.expect("link");
    let conn = LaunchConn::Link(link);
    let session_id = create_in(&conn, &git.repo, &talaria_app::launch::new_request_id()).await;
    conn.close().await;

    // A host-scoped link opened for `default`, asking for a `fixture` session.
    let host = LaunchLink::open(&home(), "default")
        .await
        .expect("host link");
    let conn = LaunchConn::Link(host);
    let reply = conn
        .call(
            "session.info",
            json!({"session_id": session_id, "profile": "fixture"}),
            Duration::from_secs(10),
        )
        .await;
    let info = match reply {
        Ok(info) => info,
        Err(failure) => panic!("a routed session.info is refused: {failure:?}"),
    };
    let cwd = info["cwd"].as_str().expect("a folder is reported");
    let want = std::fs::canonicalize(&git.repo).expect("canonical repo");
    let got = std::fs::canonicalize(cwd).expect("canonical cwd");
    println!("want: {}", want.display());
    println!("got: {}", got.display());
    assert_eq!(got, want, "the folder round-trips");
    conn.close().await;
}
