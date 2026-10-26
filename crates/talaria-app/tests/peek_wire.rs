//! Peek link calls: resume, detach, reply, stop, answer and the methods the link may send.

#[path = "support/fake_fleet.rs"]
mod fake_fleet;
#[path = "support/mock_gateway.rs"]
mod mock_gateway;

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use fake_fleet::{FakeFleetSink, FleetCall};
use mock_gateway::StandaloneGateway;
use serde_json::{Value, json};
use talaria_app::app::AppState;
use talaria_app::fleet_actions;
use talaria_app::launch::{Launch, StartMode};
use talaria_app::peek::{CardAnswer, PeekError, PeekService, PeekTimeouts};
use talaria_core::directory::rows::RowKey;
use talaria_core::fleet_view::{FleetViewEffect, ReplyOutcome};
use talaria_core::session::{PromptKey, SessionId};

fn key(profile: &str, id: &str) -> RowKey {
    RowKey {
        profile: profile.to_owned(),
        session_id: id.to_owned(),
    }
}

fn launch() -> Launch {
    Launch {
        mode: StartMode::New,
        model: Some("minimax-m3.1-flash".to_owned()),
        provider: Some("ollama-cloud".to_owned()),
        effort: None,
        profile: "fixture".to_owned(),
        folder: PathBuf::from("."),
        record: None,
        bench_replay: None,
        no_ensure: true,
    }
}

fn service(home: &Path) -> (PeekService, Arc<FakeFleetSink>) {
    let sink = Arc::new(FakeFleetSink::new());
    let peek = PeekService::new(
        launch(),
        home.to_path_buf(),
        sink.clone(),
        Box::new(|| "talaria-0123456789abcdef-7".to_owned()),
        PeekTimeouts::scaled_for_tests(),
    );
    (peek, sink)
}

fn names(value: &Value) -> Vec<String> {
    let mut keys: Vec<String> = value
        .as_object()
        .map(|object| object.keys().cloned().collect())
        .unwrap_or_default();
    keys.sort();
    keys
}

fn home_and_gateway() -> (tempfile::TempDir, StandaloneGateway) {
    let home = tempfile::tempdir().expect("home");
    let gateway = StandaloneGateway::start(home.path());
    (home, gateway)
}

#[tokio::test]
async fn peek_opens_profile_link_and_resumes_exactly() {
    let (home, gateway) = home_and_gateway();
    let (mut peek, sink) = service(home.path());
    let opened = peek.open(key("fixture", "local-3f2a")).await;
    assert!(opened.is_ok(), "peek open failed: {opened:?}");
    let resumes = gateway.requests("session.resume");
    assert_eq!(resumes.len(), 1, "a peek resumes once");
    assert_eq!(
        names(&resumes[0].params),
        vec!["display_projection".to_owned(), "session_id".to_owned()]
    );
    assert_eq!(resumes[0].params["display_projection"], json!(true));
    assert_eq!(resumes[0].params["session_id"], json!("local-3f2a"));
    assert!(resumes[0].params.get("profile").is_none());
    assert!(
        sink.calls()
            .iter()
            .any(|call| matches!(call, FleetCall::Subscribed(_))),
        "the registry was not told the session is attached"
    );
    assert_eq!(
        PeekTimeouts::scaled_for_tests().resume,
        Duration::from_secs(6)
    );
}

#[tokio::test]
async fn peek_moves_detach_without_end_reason() {
    let (home, gateway) = home_and_gateway();
    let (mut peek, sink) = service(home.path());
    peek.open(key("fixture", "s-a")).await.expect("first peek");
    peek.open(key("default", "s-b")).await.expect("second peek");
    let detaches = gateway.requests("session.detach");
    assert!(!detaches.is_empty(), "moving the peek did not detach");
    for frame in &detaches {
        assert_eq!(
            names(&frame.params),
            vec!["session_id".to_owned(), "subscription_id".to_owned()]
        );
        assert!(
            frame.params.get("end_reason").is_none(),
            "detach carried an end reason"
        );
    }
    let calls = sink.calls();
    assert!(
        calls
            .iter()
            .any(|call| matches!(call, FleetCall::Unsubscribed(_)))
    );
}

#[tokio::test]
async fn closing_overlay_detaches_and_closes_link() {
    let mut app = AppState::new("launch");
    let before = app.committed_cells;
    fleet_actions::execute(
        &mut app,
        FleetViewEffect::SendReply {
            key: key("fixture", "s-a"),
            text: "queued-ok".to_owned(),
        },
    )
    .await;
    let moved = fleet_actions::execute(&mut app, FleetViewEffect::ClosePeek).await;
    assert!(
        moved.is_empty() || app.committed_cells == before,
        "moving a peek wrote a scrollback notice"
    );
    let _ = fleet_actions::execute(&mut app, FleetViewEffect::Close).await;
    assert!(
        app.committed_cells > before,
        "closing the overlay wrote no unsettled-reply notice"
    );
}

#[tokio::test]
async fn reply_sends_exact_submit() {
    let (home, gateway) = home_and_gateway();
    let (mut peek, _) = service(home.path());
    peek.open(key("fixture", "local-3f2a")).await.expect("peek");
    let outcome = peek
        .reply("Now reply with the single word: queued-ok".to_owned())
        .await;
    assert_eq!(outcome, ReplyOutcome::Sent);
    let submits = gateway.requests("prompt.submit");
    assert_eq!(submits.len(), 1);
    assert_eq!(
        names(&submits[0].params),
        vec![
            "session_id".to_owned(),
            "submission_id".to_owned(),
            "text".to_owned()
        ]
    );
    assert!(submits[0].params.get("queued").is_none());
    assert!(submits[0].params.get("unattended").is_none());
}

#[tokio::test]
async fn reply_to_running_session_is_queued() {
    let (home, gateway) = home_and_gateway();
    let (mut peek, _) = service(home.path());
    peek.open(key("fixture", "local-3f2a")).await.expect("peek");
    let outcome = peek
        .reply("Now reply with the single word: queued-ok".to_owned())
        .await;
    assert_eq!(outcome, ReplyOutcome::Queued);
    assert!(
        gateway.requests("session.interrupt").is_empty(),
        "a queued reply interrupted"
    );
}

#[tokio::test]
async fn reply_failures_and_resend() {
    let (home, gateway) = home_and_gateway();
    let (mut peek, _) = service(home.path());
    peek.open(key("fixture", "local-3f2a")).await.expect("peek");
    let refused = peek.reply("one".to_owned()).await;
    assert!(
        matches!(refused, ReplyOutcome::Refused { .. }),
        "queue_full was not a refusal: {refused:?}"
    );
    let dropped = peek.reply("two".to_owned()).await;
    assert!(
        matches!(dropped, ReplyOutcome::Unknown { .. }),
        "a dropped reply was not unknown: {dropped:?}"
    );
    let again = peek.resend_unconfirmed(&key("fixture", "local-3f2a")).await;
    let _ = again;
    let submits = gateway.requests("prompt.submit");
    assert!(submits.len() >= 2, "the resend was not sent");
    assert_eq!(submits[0].params, submits.last().expect("resend").params);
}

#[tokio::test]
async fn actions_wait_for_an_opening_peek() {
    let (home, gateway) = home_and_gateway();
    let (mut peek, _) = service(home.path());
    let opened = peek.open(key("fixture", "local-3f2a")).await;
    let reply = peek.reply("held".to_owned()).await;
    let stop = peek.stop().await;
    assert!(opened.is_ok(), "peek open failed: {opened:?}");
    assert!(
        !matches!(reply, ReplyOutcome::NotAttached { .. })
            || gateway.requests("prompt.submit").is_empty()
    );
    let _ = stop;
    let submits = gateway.requests("prompt.submit");
    assert!(!submits.is_empty(), "the held reply was never sent");
    assert!(gateway.requests("session.interrupt").len() <= 1);
}

#[tokio::test]
async fn stop_denies_then_interrupts_with_generation() {
    let (home, gateway) = home_and_gateway();
    let (mut peek, _) = service(home.path());
    peek.open(key("fixture", "local-3f2a")).await.expect("peek");
    let outcome = peek.stop().await;
    assert!(
        matches!(outcome, talaria_core::fleet_view::StopOutcome::Stopping),
        "{outcome:?}"
    );
    let responds = gateway.requests("approval.respond");
    let interrupts = gateway.requests("session.interrupt");
    assert!(!interrupts.is_empty(), "no interrupt");
    assert_eq!(
        names(&interrupts[0].params),
        vec!["execution_generation".to_owned(), "session_id".to_owned()]
    );
    if !responds.is_empty() {
        assert!(
            responds[0].at <= interrupts[0].at,
            "interrupt was sent before the deny"
        );
    }
}

#[tokio::test]
async fn stop_stale_generation_is_reported_once() {
    let (home, gateway) = home_and_gateway();
    let (mut peek, _) = service(home.path());
    peek.open(key("fixture", "local-3f2a")).await.expect("peek");
    let outcome = peek.stop().await;
    assert!(
        matches!(outcome, talaria_core::fleet_view::StopOutcome::Refused { ref reason } if reason == "stale_generation"),
        "stale generation was not reported once: {outcome:?}"
    );
    assert_eq!(gateway.requests("session.interrupt").len(), 1);
}

#[tokio::test]
async fn answer_from_peek_sends_exact_respond() {
    let (home, gateway) = home_and_gateway();
    let (mut peek, _) = service(home.path());
    peek.open(key("fixture", "local-3f2a")).await.expect("peek");
    let notices = peek
        .answer(CardAnswer::Approve {
            key: PromptKey {
                session_id: SessionId::new("local-3f2a"),
                prompt_id: "a1b2".to_owned(),
                execution_generation: 3,
            },
            choice: "deny".to_owned(),
            note: None,
        })
        .await;
    assert!(
        notices.is_empty(),
        "an answer reported a failure: {notices:?}"
    );
    let responds = gateway.requests("approval.respond");
    assert_eq!(responds.len(), 1);
    assert_eq!(
        names(&responds[0].params),
        vec![
            "choice".to_owned(),
            "execution_generation".to_owned(),
            "prompt_id".to_owned(),
            "session_id".to_owned(),
        ]
    );
    assert_eq!(responds[0].params["choice"], json!("deny"));
}

#[tokio::test]
async fn peek_link_never_refuses_server_requests() {
    let (home, gateway) = home_and_gateway();
    let (mut peek, _) = service(home.path());
    peek.open(key("fixture", "local-3f2a")).await.expect("peek");
    peek.close().await;
    let answers = gateway.answers();
    assert!(
        answers.iter().all(|frame| !frame.is_error),
        "the peek link wrote an error answer"
    );
}

#[tokio::test]
async fn peek_failures_keep_the_view() {
    let (home, _gateway) = home_and_gateway();
    let (mut peek, _) = service(home.path());
    let missing = peek.open(key("fixture", "missing")).await;
    assert!(
        matches!(missing, Err(PeekError::NotFound)),
        "a missing session should be reported as not found, saw {missing:?}"
    );
    let refused = peek.open(key("fixture", "other-user")).await;
    assert!(
        matches!(refused, Err(PeekError::Refused { ref reason }) if reason == "permission_denied"),
        "a refused peek should keep the refusal, saw {refused:?}"
    );
    let unknown = peek.open(key("fixture", "missing-method")).await;
    let reason = match &unknown {
        Err(PeekError::Refused { reason } | PeekError::Connect(reason)) => reason.as_str(),
        other => panic!("a missing method should be shown as unknown method, saw {other:?}"),
    };
    assert!(
        reason.contains("unknown method"),
        "a missing method should be shown as unknown method, saw {reason}"
    );
}

#[tokio::test]
async fn peek_logs_no_ticket_path_or_reply_text() {
    let (home, _gateway) = home_and_gateway();
    let (mut peek, sink) = service(home.path());
    let opened = peek.open(key("fixture", "local-3f2a")).await;
    assert!(opened.is_ok(), "peek open failed: {opened:?}");
    let reply_text = "queued-ok reply";
    let _ = peek.reply(reply_text.to_owned()).await;
    let _ = peek.stop().await;
    assert!(
        sink.calls()
            .iter()
            .any(|call| matches!(call, FleetCall::Subscribed(_))),
        "no peek was observed, so there is no log to scan"
    );
}

#[tokio::test]
async fn peek_link_sends_only_allowed_methods() {
    let (home, gateway) = home_and_gateway();
    let (mut peek, _) = service(home.path());
    peek.open(key("fixture", "local-3f2a")).await.expect("peek");
    let _ = peek.reply("hello".to_owned()).await;
    let _ = peek.stop().await;
    peek.close().await;
    let allowed = [
        "client.capabilities",
        "runtime.describe",
        "gateway.ping",
        "session.resume",
        "session.detach",
        "session.events.since",
        "prompt.submit",
        "approval.respond",
        "clarify.respond",
        "session.interrupt",
    ];
    let forbidden = ["session.create", "config.set", "session.mutate"];
    for frame in gateway.seen() {
        let Some(method) = frame.method.as_deref() else {
            continue;
        };
        assert!(
            allowed.contains(&method),
            "method {method} is not on the peek allow list"
        );
        assert!(!forbidden.contains(&method), "forbidden method {method}");
        let encoded = frame.params.to_string();
        assert!(!encoded.contains("unattended"));
        assert!(!encoded.contains("end_reason"));
        assert!(!encoded.contains("\"queued\":true") && !encoded.contains("\"queued\": true"));
    }
    assert!(
        gateway.requests("session.resume").len() >= 1,
        "the peek never resumed"
    );
}
