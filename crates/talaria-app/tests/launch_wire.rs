//! Frames a launch sends. The stub sends none, so each test fails on an empty recording.

#[path = "support/mock_gateway.rs"]
mod mock_gateway;

use std::collections::BTreeSet;
use std::path::PathBuf;

use mock_gateway::StandaloneGateway;
use serde_json::Value;
use talaria_app::bg::{
    CancelToken, LaunchConn, LaunchIds, LaunchLink, LaunchOrigin, LaunchRequest, LinkScope,
    WorktreeWanted, run_launch,
};
use talaria_app::worktree::GitRunner;

fn request(profile: &str, worktree: bool) -> LaunchRequest {
    LaunchRequest {
        prompt: "Reply with the single word: ready".to_owned(),
        profile: profile.to_owned(),
        folder: PathBuf::from("repo"),
        model: Some("fixture-model".to_owned()),
        provider: Some("fixture-provider".to_owned()),
        effort: Some("low".to_owned()),
        worktree: worktree.then_some(WorktreeWanted { name: None }),
        origin: LaunchOrigin::CommandLine,
    }
}

fn ids() -> LaunchIds {
    LaunchIds {
        request_id: "req-1".to_owned(),
        submission_id: "sub-1".to_owned(),
    }
}

fn keys_of(params: &Value) -> BTreeSet<String> {
    params
        .as_object()
        .map(|object| object.keys().cloned().collect())
        .unwrap_or_default()
}

fn has_key(value: &Value, name: &str) -> bool {
    match value {
        Value::Object(map) => {
            map.contains_key(name) || map.values().any(|child| has_key(child, name))
        }
        Value::Array(items) => items.iter().any(|child| has_key(child, name)),
        _ => false,
    }
}

async fn launch_in(home: &std::path::Path, profile: &str, foreground: &str) {
    let link = LaunchLink::open(home, foreground).await.expect("link");
    let conn = LaunchConn::Link(link);
    let _ = run_launch(
        &conn,
        &GitRunner::from_env(),
        request(profile, false),
        ids(),
        None,
        CancelToken::new(),
    )
    .await;
}

#[tokio::test]
async fn command_line_launch_sends_exact_frames() {
    let home = tempfile::tempdir().expect("home");
    let gateway = StandaloneGateway::start(home.path());
    launch_in(home.path(), "fixture", "fixture").await;
    let creates = gateway.requests("session.create");
    assert_eq!(creates.len(), 1, "no session.create frame");
    let keys = keys_of(&creates[0].params);
    let expected = BTreeSet::from([
        "request_id".to_owned(),
        "source".to_owned(),
        "cwd".to_owned(),
        "model".to_owned(),
        "provider".to_owned(),
        "reasoning".to_owned(),
    ]);
    assert_eq!(keys, expected);
    assert_eq!(creates[0].params["source"], "tui");
    let submits = gateway.requests("prompt.submit");
    assert_eq!(submits.len(), 1, "no prompt.submit frame");
    assert_eq!(
        keys_of(&submits[0].params),
        BTreeSet::from([
            "session_id".to_owned(),
            "text".to_owned(),
            "submission_id".to_owned(),
        ])
    );
    assert_eq!(
        submits[0].params["text"],
        "Reply with the single word: ready"
    );
    let detaches = gateway.requests("session.detach");
    assert_eq!(detaches.len(), 1, "no session.detach frame");
    assert!(detaches[0].params.get("end_reason").is_none());
}

#[tokio::test]
async fn host_link_routes_profile() {
    let home = tempfile::tempdir().expect("home");
    let gateway = StandaloneGateway::start(home.path());
    launch_in(home.path(), "fixture", "default").await;
    let creates = gateway.requests("session.create");
    assert_eq!(creates.len(), 1, "no session.create frame");
    assert_eq!(creates[0].params["profile"], "fixture");
    let submits = gateway.requests("prompt.submit");
    assert_eq!(submits.len(), 1, "no prompt.submit frame");
    assert_eq!(submits[0].params["profile"], "fixture");
}

#[tokio::test]
async fn profile_only_link_cannot_launch_elsewhere() {
    let home = tempfile::tempdir().expect("home");
    let gateway = StandaloneGateway::start(home.path());
    gateway.set_refuse_host_scope(true);
    let _binding = hermes_client::ticket::TicketError::BindingRejected;
    let _refused = hermes_client::ticket::TicketError::Refused(String::new());
    launch_in(home.path(), "fixture", "fixture").await;
    assert_eq!(
        gateway.requests("session.create").len(),
        1,
        "no session.create frame"
    );
    let link = LaunchLink::open(home.path(), "fixture")
        .await
        .expect("link");
    assert!(matches!(
        link.scope(),
        LinkScope::ProfileOnly { profile } if profile == "fixture"
    ));
    let before = gateway.requests("session.create").len();
    launch_in(home.path(), "default", "fixture").await;
    assert_eq!(
        gateway.requests("session.create").len(),
        before,
        "profile-only link sent a frame"
    );
}

#[tokio::test]
async fn create_retry_reuses_request_id() {
    let home = tempfile::tempdir().expect("home");
    let gateway = StandaloneGateway::start(home.path());
    gateway.set_behavior("session.create", mock_gateway::Behavior::DropBeforeReply);
    launch_in(home.path(), "fixture", "fixture").await;
    let creates = gateway.requests("session.create");
    assert_eq!(creates.len(), 2, "no session.create frame");
    assert_eq!(
        creates[0].params["request_id"],
        creates[1].params["request_id"]
    );
}

#[tokio::test]
async fn server_requests_are_left_alone() {
    let home = tempfile::tempdir().expect("home");
    let gateway = StandaloneGateway::start(home.path());
    launch_in(home.path(), "fixture", "fixture").await;
    assert_eq!(
        gateway.requests("session.create").len(),
        1,
        "no session.create frame"
    );
    assert!(!gateway.server_request_kinds().is_empty());
    assert!(
        gateway.answers().is_empty(),
        "a server request was answered"
    );
}

#[tokio::test]
async fn end_reason_only_for_abandoned_sessions() {
    let home = tempfile::tempdir().expect("home");
    let gateway = StandaloneGateway::start(home.path());
    launch_in(home.path(), "fixture", "fixture").await;
    let detaches = gateway.requests("session.detach");
    assert_eq!(detaches.len(), 1, "no session.detach frame");
    assert!(detaches[0].params.get("end_reason").is_none());
    gateway.set_behavior("prompt.submit", mock_gateway::Behavior::VoiceStopped);
    launch_in(home.path(), "fixture", "fixture").await;
    let detaches = gateway.requests("session.detach");
    let abandoned = detaches
        .iter()
        .any(|frame| frame.params["end_reason"] == "launch_abandoned");
    assert!(abandoned, "abandoned detach has no end reason");
}

#[tokio::test]
async fn forbidden_keys_never_sent() {
    let home = tempfile::tempdir().expect("home");
    let gateway = StandaloneGateway::start(home.path());
    launch_in(home.path(), "fixture", "fixture").await;
    assert_eq!(
        gateway.requests("session.create").len(),
        1,
        "no session.create frame"
    );
    let banned = ["title", "hidden", "cwd_explicit", "finite", "unattended"];
    for frame in gateway.seen() {
        for name in banned {
            assert!(!has_key(&frame.params, name), "forbidden key {name}");
        }
    }
    assert!(gateway.forbidden_keys().is_empty());
}
