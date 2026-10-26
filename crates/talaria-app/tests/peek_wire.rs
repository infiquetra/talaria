//! Peek link calls: resume, detach, reply, stop, answer and the methods the link may send.

#[path = "support/fake_fleet.rs"]
mod fake_fleet;
#[path = "support/mock_gateway.rs"]
mod mock_gateway;

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use fake_fleet::{FakeFleetSink, FleetCall};
use mock_gateway::{Behavior, MockGateway, Reply, StandaloneGateway, data_snapshot};
use serde_json::{Value, json};
use talaria_app::app::{self, AppEffect, AppMessage, AppState, UserAction};
use talaria_app::fleet_actions::{self, PeekSetup, PeekWorker};
use talaria_app::launch::{Launch, StartMode};
use talaria_app::peek::{CardAnswer, PeekError, PeekService, PeekTimeouts};
use talaria_core::directory::rows::RowKey;
use talaria_core::fleet::{FleetRow, ProfileStatus, RowState};
use talaria_core::fleet_view::{
    FleetAction, FleetViewEffect, FleetViewInput, ReplyOutcome, StopOutcome,
};
use talaria_core::session::{PromptKey, SessionId};

const NOW: f64 = 1_792_700_000.0;

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

/// A contract-checked snapshot for session `id`: idle, running, or running with an open
/// approval `a1b2` at generation 3.
fn snapshot(id: &str, running: bool, approval: bool) -> Value {
    let mut snapshot = if approval {
        data_snapshot("open_approval.json")
    } else if running {
        data_snapshot("running_turn.json")
    } else {
        let mut idle = data_snapshot("running_turn.json");
        idle["running"] = json!(false);
        idle
    };
    snapshot["session_id"] = json!(id);
    snapshot["stored_session_id"] = json!(id);
    if approval {
        snapshot["running"] = json!(running);
        snapshot["execution_generation"] = json!(3);
        snapshot["prompts"][0]["prompt_id"] = json!("a1b2");
        snapshot["prompts"][0]["execution_generation"] = json!(3);
        snapshot["prompts"][0]["session_id"] = json!(id);
    }
    snapshot
}

fn seed(gateway: &StandaloneGateway, id: &str, running: bool, approval: bool) {
    gateway.add_session_snapshot(snapshot(id, running, approval));
}

#[tokio::test]
async fn peek_opens_profile_link_and_resumes_exactly() {
    let (home, gateway) = home_and_gateway();
    seed(&gateway, "local-3f2a", false, false);
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
        gateway
            .ticket_records()
            .iter()
            .all(|ticket| ticket.scope == "profile"),
        "the peek link is profile scoped"
    );
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
    seed(&gateway, "s-a", false, false);
    seed(&gateway, "s-c", false, false);
    seed(&gateway, "s-b", false, false);
    let (mut peek, sink) = service(home.path());
    peek.open(key("fixture", "s-a")).await.expect("first peek");
    peek.open(key("fixture", "s-c"))
        .await
        .expect("same profile");
    assert_eq!(
        gateway.websocket_connections(),
        1,
        "a peek of the same profile reuses the link"
    );
    peek.open(key("default", "s-b")).await.expect("second peek");
    assert_eq!(
        gateway.websocket_connections(),
        2,
        "a peek of another profile replaces the link"
    );
    let detaches = gateway.requests("session.detach");
    assert_eq!(detaches.len(), 2, "each move detached the last peek");
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
            .any(|call| matches!(call, FleetCall::Unsubscribed(key) if key.session_id == "s-a"))
    );
}

fn fleet_row(id: &str, state: RowState) -> FleetRow {
    FleetRow {
        key: key("fixture", id),
        title: format!("title {id}"),
        model: Some("minimax-m3.1-flash".to_owned()),
        source: Some("tui".to_owned()),
        started_at: Some(NOW - 60.0),
        message_count: 2,
        state,
        state_since: NOW - 20.0,
        outcome: None,
        foreground: false,
        subscribed: false,
        stale: false,
        gone: false,
        unlisted: false,
        activity: None,
        cost_usd: None,
    }
}

/// Feed one input to the app and run every peek effect it produces, as the loop would.
async fn drive(app: &mut AppState, input: FleetViewInput) -> Vec<AppEffect> {
    let mut pending = app::update(app, AppMessage::User(UserAction::Fleet(input)));
    let mut seen = Vec::new();
    while let Some(effect) = pending.pop() {
        match effect {
            AppEffect::FleetPeek(effect) => {
                for result in fleet_actions::execute(app, effect).await {
                    pending.extend(app::update(app, AppMessage::FleetView(result)));
                }
            }
            other => seen.push(other),
        }
    }
    seen
}

fn committed(effects: &[AppEffect]) -> String {
    effects
        .iter()
        .filter_map(|effect| match effect {
            AppEffect::CommitLines(lines) => Some(lines.join("\n")),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[tokio::test]
async fn closing_overlay_detaches_and_closes_link() {
    let (home, gateway) = home_and_gateway();
    seed(&gateway, "s-a", true, false);
    seed(&gateway, "s-b", true, false);
    // The reply's call never answers, so its outcome stays unknown.
    gateway.queue_behavior("prompt.submit", Behavior::Silent);
    let mut app = AppState::new("launch");
    app.hermes_home = home.path().to_path_buf();
    app.profile = "fixture".to_owned();
    app.peek_timeouts = PeekTimeouts {
        call: Duration::from_millis(300),
        ..PeekTimeouts::scaled_for_tests()
    };
    let _ = app::update(
        &mut app,
        AppMessage::User(UserAction::Submit("/fleet".to_owned())),
    );
    let _ = app::update(
        &mut app,
        AppMessage::FleetView(FleetViewInput::Snapshot {
            version: 1,
            rows: vec![
                fleet_row("s-a", RowState::Working),
                fleet_row("s-b", RowState::Working),
            ],
            queue: Vec::new(),
            profiles: vec![(
                "fixture".to_owned(),
                ProfileStatus::Listed { truncated: false },
            )],
            scope_note: None,
            link_note: None,
        }),
    );
    drive(&mut app, FleetViewInput::Action(FleetAction::Reply)).await;
    for c in "queued-ok".chars() {
        drive(&mut app, FleetViewInput::Text(c)).await;
    }
    let sent = drive(&mut app, FleetViewInput::Enter).await;
    assert!(
        committed(&sent).is_empty(),
        "a reply wrote a scrollback notice"
    );
    let flags = app
        .fleet
        .as_ref()
        .expect("overlay")
        .row_flags(&key("fixture", "s-a"));
    assert!(flags.unconfirmed, "an unanswered reply is unconfirmed");
    drive(&mut app, FleetViewInput::Action(FleetAction::Down)).await;
    let moved = drive(&mut app, FleetViewInput::Action(FleetAction::Peek)).await;
    assert!(
        committed(&moved).is_empty(),
        "moving a peek wrote a scrollback notice"
    );
    let started = Instant::now();
    drive(&mut app, FleetViewInput::Escape).await;
    let closed = drive(&mut app, FleetViewInput::Escape).await;
    assert!(
        started.elapsed() < Duration::from_secs(2),
        "closing took {:?}",
        started.elapsed()
    );
    assert!(app.fleet.is_none(), "the overlay closed");
    let text = committed(&closed);
    assert!(
        text.contains("The reply to \"title s-a\" may not have been sent (no reply). Open the session to check before sending it again."),
        "closing the overlay wrote no unsettled-reply notice: {text}"
    );
    assert_eq!(text.matches("may not have been sent").count(), 1, "{text}");
    for frame in gateway.requests("session.detach") {
        assert!(frame.params.get("end_reason").is_none());
    }
    assert!(
        !gateway.requests("session.detach").is_empty(),
        "closing the overlay detached the peek"
    );
}

#[tokio::test]
async fn reply_sends_exact_submit() {
    let (home, gateway) = home_and_gateway();
    seed(&gateway, "local-3f2a", false, false);
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
    assert_eq!(
        submits[0].params["submission_id"],
        json!("talaria-0123456789abcdef-7"),
        "the reply uses the shared submission id generator"
    );
    assert!(submits[0].params.get("queued").is_none());
    assert!(submits[0].params.get("unattended").is_none());
}

#[tokio::test]
async fn reply_to_running_session_is_queued() {
    let (home, gateway) = home_and_gateway();
    seed(&gateway, "local-3f2a", true, false);
    let (mut peek, _) = service(home.path());
    peek.open(key("fixture", "local-3f2a")).await.expect("peek");
    gateway.queue_submit_status("queued");
    let outcome = peek
        .reply("Now reply with the single word: queued-ok".to_owned())
        .await;
    assert_eq!(outcome, ReplyOutcome::Queued);
    for (status, want) in [
        ("started", ReplyOutcome::Sent),
        ("terminal", ReplyOutcome::AlreadyRan),
        ("unknown", ReplyOutcome::StateUnknown),
    ] {
        gateway.queue_submit_status(status);
        assert_eq!(peek.reply(format!("reply {status}")).await, want);
    }
    gateway.queue_behavior("prompt.submit", Behavior::VoiceStopped);
    assert_eq!(
        peek.reply("stop".to_owned()).await,
        ReplyOutcome::VoiceStopped
    );
    assert!(
        gateway.requests("session.interrupt").is_empty(),
        "a queued reply interrupted"
    );
}

#[tokio::test]
async fn reply_failures_and_resend() {
    let (home, gateway) = home_and_gateway();
    seed(&gateway, "local-3f2a", true, false);
    let (mut peek, _) = service(home.path());
    peek.open(key("fixture", "local-3f2a")).await.expect("peek");
    gateway.queue_behavior("prompt.submit", Behavior::Refuse("queue_full".to_owned()));
    let refused = peek.reply("one".to_owned()).await;
    assert!(
        matches!(refused, ReplyOutcome::Refused { ref reason } if reason == "queue_full"),
        "queue_full was not a refusal: {refused:?}"
    );
    gateway.queue_behavior(
        "prompt.submit",
        Behavior::Refuse("permission_denied".to_owned()),
    );
    let other = peek.reply("other".to_owned()).await;
    assert!(
        matches!(other, ReplyOutcome::Refused { ref reason } if reason == "permission_denied"),
        "{other:?}"
    );
    gateway.queue_behavior("prompt.submit", Behavior::DropBeforeReply);
    let dropped = peek.reply("two".to_owned()).await;
    assert!(
        matches!(dropped, ReplyOutcome::Unknown { .. }),
        "a dropped reply was not unknown: {dropped:?}"
    );
    // The resend goes out once the link is back.
    let started = Instant::now();
    while gateway.websocket_connections() < 2 && started.elapsed() < Duration::from_secs(5) {
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    let mut again = ReplyOutcome::Unknown {
        why: "not tried".to_owned(),
    };
    while started.elapsed() < Duration::from_secs(5) {
        again = peek.resend_unconfirmed(&key("fixture", "local-3f2a")).await;
        if !matches!(again, ReplyOutcome::Refused { ref reason } if reason == "not connected") {
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    assert_eq!(again, ReplyOutcome::Sent, "the resend was answered");
    let submits = gateway.requests("prompt.submit");
    assert!(submits.len() >= 4, "the resend was not sent");
    assert_eq!(
        submits[2].params,
        submits.last().expect("resend").params,
        "the resend repeats the dropped reply's params byte for byte"
    );
}

fn worker(home: &Path) -> PeekWorker {
    PeekWorker::start(PeekSetup {
        launch: launch(),
        home: home.to_path_buf(),
        fleet: Arc::new(FakeFleetSink::new()),
        next_submission_id: Box::new(|| "talaria-0123456789abcdef-7".to_owned()),
        timeouts: PeekTimeouts::scaled_for_tests(),
        reports: None,
    })
}

#[tokio::test]
async fn actions_wait_for_an_opening_peek() {
    let (home, gateway) = home_and_gateway();
    seed(&gateway, "local-3f2a", true, false);
    gateway.queue_behavior(
        "session.resume",
        Behavior::Delay(Duration::from_millis(300)),
    );
    let worker = worker(home.path());
    let target = key("fixture", "local-3f2a");
    let opened = worker.run(FleetViewEffect::OpenPeek(target.clone()));
    let reply = worker.run(FleetViewEffect::SendReply {
        key: target.clone(),
        text: "held".to_owned(),
    });
    let stop = worker.run(FleetViewEffect::Stop(target.clone()));
    let opened = opened.await.expect("open result");
    assert!(
        matches!(opened.as_slice(), [FleetViewInput::Peek { .. }]),
        "{opened:?}"
    );
    let reply = reply.await.expect("reply result");
    let stop = stop.await.expect("stop result");
    assert!(
        matches!(
            reply.as_slice(),
            [FleetViewInput::ReplyDone {
                outcome: ReplyOutcome::Sent,
                ..
            }]
        ),
        "{reply:?}"
    );
    assert!(
        matches!(
            stop.as_slice(),
            [FleetViewInput::StopDone {
                outcome: StopOutcome::Stopping,
                ..
            }]
        ),
        "{stop:?}"
    );
    let resumed_at = gateway.requests("session.resume")[0].at;
    let submits = gateway.requests("prompt.submit");
    assert_eq!(submits.len(), 1, "the held reply was sent once");
    assert!(
        submits[0].at >= resumed_at + Duration::from_millis(250),
        "the reply went out before the resume answered"
    );
    let interrupts = gateway.requests("session.interrupt");
    assert_eq!(interrupts.len(), 1);
    assert_eq!(interrupts[0].params["execution_generation"], json!(2));

    let missing = key("fixture", "missing");
    let failed = worker
        .run(FleetViewEffect::SendReply {
            key: missing.clone(),
            text: "lost".to_owned(),
        })
        .await
        .expect("reply result");
    assert!(
        matches!(failed.as_slice(), [FleetViewInput::ReplyDone { outcome: ReplyOutcome::NotAttached { reason }, .. }] if reason == "not_found"),
        "{failed:?}"
    );
    let stopped = worker
        .run(FleetViewEffect::Stop(missing))
        .await
        .expect("stop result");
    assert!(
        matches!(
            stopped.as_slice(),
            [FleetViewInput::StopDone {
                outcome: StopOutcome::NotAttached { .. },
                ..
            }]
        ),
        "{stopped:?}"
    );
    assert_eq!(
        gateway.requests("prompt.submit").len(),
        1,
        "a failed attach sent nothing"
    );
}

/// Run the peek link's events into the service until `check` holds or the budget ends.
async fn pump(
    peek: &mut PeekService,
    events: &mut tokio::sync::mpsc::Receiver<hermes_client::liveness::LinkEvent>,
    budget: Duration,
    check: impl Fn(&PeekService) -> bool,
) -> bool {
    let deadline = tokio::time::Instant::now() + budget;
    while !check(peek) {
        match tokio::time::timeout_at(deadline, events.recv()).await {
            Ok(Some(event)) => {
                let _ = peek.on_link_event(event);
            }
            _ => return false,
        }
    }
    true
}

#[tokio::test]
async fn stop_denies_then_interrupts_with_generation() {
    let (home, gateway) = home_and_gateway();
    seed(&gateway, "local-3f2a", true, true);
    let (mut peek, _) = service(home.path());
    peek.open(key("fixture", "local-3f2a")).await.expect("peek");
    let outcome = peek.stop().await;
    assert!(matches!(outcome, StopOutcome::Stopping), "{outcome:?}");
    let responds = gateway.requests("approval.respond");
    let interrupts = gateway.requests("session.interrupt");
    assert_eq!(responds.len(), 1, "the open approval was denied");
    assert_eq!(responds[0].params["choice"], json!("deny"));
    assert!(!interrupts.is_empty(), "no interrupt");
    assert_eq!(
        names(&interrupts[0].params),
        vec!["execution_generation".to_owned(), "session_id".to_owned()]
    );
    assert_eq!(interrupts[0].params["execution_generation"], json!(3));
    assert!(
        responds[0].at <= interrupts[0].at,
        "interrupt was sent before the deny"
    );

    // A deny that never answers: after the deny timeout the stop goes on with a notice.
    let (home, gateway) = home_and_gateway();
    seed(&gateway, "local-3f2a", true, true);
    gateway.set_respond_reply(Reply::Silent);
    let (mut peek, _) = service(home.path());
    peek.open(key("fixture", "local-3f2a")).await.expect("peek");
    let outcome = peek.stop().await;
    assert!(matches!(outcome, StopOutcome::Stopping), "{outcome:?}");
    assert_eq!(gateway.requests("session.interrupt").len(), 1);
    let notices = peek.take_notices();
    assert!(
        notices
            .iter()
            .any(|notice| notice.text == "deny not confirmed; interrupting anyway"),
        "{notices:?}"
    );

    // An open sudo request: the stop answers nothing for it; Hermes's cancel closes its card.
    let (home, gateway) = home_and_gateway();
    seed(&gateway, "local-3f2a", true, false);
    let (mut peek, _) = service(home.path());
    peek.open(key("fixture", "local-3f2a")).await.expect("peek");
    let mut events = peek.take_events().expect("link events");
    let mock = MockGateway::clone(&gateway);
    let sudo = mock
        .server_request_with("sudo", json!({"session_id": "local-3f2a"}))
        .await;
    assert!(
        pump(&mut peek, &mut events, Duration::from_secs(2), |peek| {
            !peek.masked_requests().is_empty()
        })
        .await,
        "the sudo card did not open"
    );
    let _ = peek.stop().await;
    assert!(
        gateway.answers_for(&sudo).is_empty(),
        "the stop answered the sudo request"
    );
    mock.request_cancel(&sudo, "sudo", "interrupted").await;
    assert!(
        pump(&mut peek, &mut events, Duration::from_secs(2), |peek| {
            peek.masked_requests().is_empty()
        })
        .await,
        "request.cancel did not close the sudo card"
    );
}

#[tokio::test]
async fn stop_stale_generation_is_reported_once() {
    let (home, gateway) = home_and_gateway();
    seed(&gateway, "local-3f2a", true, false);
    gateway.queue_behavior(
        "session.interrupt",
        Behavior::Refuse("stale_generation".to_owned()),
    );
    let (mut peek, _) = service(home.path());
    peek.open(key("fixture", "local-3f2a")).await.expect("peek");
    let outcome = peek.stop().await;
    assert!(
        matches!(outcome, StopOutcome::Refused { ref reason } if reason == "stale_generation"),
        "stale generation was not reported once: {outcome:?}"
    );
    assert_eq!(gateway.requests("session.interrupt").len(), 1);
    // The latch is cleared: a second stop sends a second interrupt.
    let again = peek.stop().await;
    assert!(matches!(again, StopOutcome::Stopping), "{again:?}");
    assert_eq!(gateway.requests("session.interrupt").len(), 2);
}

#[tokio::test]
async fn answer_from_peek_sends_exact_respond() {
    let (home, gateway) = home_and_gateway();
    seed(&gateway, "local-3f2a", true, true);
    let (mut peek, _) = service(home.path());
    peek.open(key("fixture", "local-3f2a")).await.expect("peek");
    let prompt = PromptKey {
        session_id: SessionId::new("local-3f2a"),
        prompt_id: "a1b2".to_owned(),
        execution_generation: 3,
    };
    let notices = peek
        .answer(CardAnswer::Approve {
            key: prompt.clone(),
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

    let (home, gateway) = home_and_gateway();
    seed(&gateway, "local-3f2a", true, true);
    gateway.set_respond_reply(Reply::AlreadyResolved);
    let (mut peek, _) = service(home.path());
    peek.open(key("fixture", "local-3f2a")).await.expect("peek");
    let late = peek
        .answer(CardAnswer::Approve {
            key: prompt,
            choice: "deny".to_owned(),
            note: None,
        })
        .await;
    assert!(
        late.iter()
            .any(|notice| notice.text == "answered elsewhere or timed out"),
        "already_resolved was not reported: {late:?}"
    );
}

#[tokio::test]
async fn peek_link_never_refuses_server_requests() {
    let (home, gateway) = home_and_gateway();
    seed(&gateway, "local-3f2a", true, false);
    let (mut peek, _) = service(home.path());
    peek.open(key("fixture", "local-3f2a")).await.expect("peek");
    let mut events = peek.take_events().expect("link events");
    let mock = MockGateway::clone(&gateway);
    let session = json!({"session_id": "local-3f2a"});
    let mut ignored = Vec::new();
    for kind in ["theme.pick", "clarify", "approval"] {
        ignored.push(mock.server_request_with(kind, session.clone()).await);
    }
    let sudo = mock.server_request_with("sudo", session).await;
    assert!(
        pump(&mut peek, &mut events, Duration::from_secs(2), |peek| {
            !peek.masked_requests().is_empty()
        })
        .await,
        "the sudo request did not become a masked card"
    );
    tokio::time::sleep(Duration::from_millis(100)).await;
    for id in &ignored {
        assert!(
            gateway.answers_for(id).is_empty(),
            "the peek link answered a request it cannot show"
        );
    }
    peek.close().await;
    tokio::time::sleep(Duration::from_millis(100)).await;
    assert!(
        gateway.answers_for(&sudo).is_empty(),
        "closing the peek answered the open sudo request"
    );
    let answers = gateway.answers();
    assert!(
        answers.iter().all(|frame| !frame.is_error),
        "the peek link wrote an error answer"
    );
}

#[tokio::test]
async fn peek_failures_keep_the_view() {
    let (home, gateway) = home_and_gateway();
    let (mut peek, _) = service(home.path());
    let missing = peek.open(key("fixture", "missing")).await;
    assert!(
        matches!(missing, Err(PeekError::NotFound)),
        "a missing session should be reported as not found, saw {missing:?}"
    );
    seed(&gateway, "other-user", false, false);
    gateway.queue_behavior(
        "session.resume",
        Behavior::Refuse("permission_denied".to_owned()),
    );
    let refused = peek.open(key("fixture", "other-user")).await;
    assert!(
        matches!(refused, Err(PeekError::Refused { ref reason }) if reason == "permission_denied"),
        "a refused peek should keep the refusal, saw {refused:?}"
    );
    seed(&gateway, "missing-method", false, false);
    gateway.queue_behavior(
        "session.resume",
        Behavior::FailCode {
            code: -32601,
            message: "unknown method".to_owned(),
            reason: None,
        },
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
    seed(&gateway, "slow", false, false);
    gateway.queue_behavior("session.resume", Behavior::Silent);
    let slow = peek.open(key("fixture", "slow")).await;
    assert!(
        matches!(slow, Err(PeekError::Timeout)),
        "a resume with no reply times out: {slow:?}"
    );
    assert_eq!(
        PeekError::TooLarge.text(),
        "This session's history is larger than Talaria accepts in one message (8 MiB)."
    );
    let empty = tempfile::tempdir().expect("empty home");
    let (mut lonely, _) = service(empty.path());
    let none = lonely.open(key("fixture", "local-3f2a")).await;
    assert!(
        matches!(none, Err(PeekError::Connect(_))),
        "no gateway is a connect failure: {none:?}"
    );
}

#[tokio::test]
async fn peek_logs_no_ticket_path_or_reply_text() {
    let (home, gateway) = home_and_gateway();
    seed(&gateway, "local-3f2a", true, false);
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
    // The errors a peek reports are the only text it hands out; none names a ticket, a path
    // under the home or the reply.
    let home_text = home.path().display().to_string();
    for error in [
        PeekError::Connect("could not reach the gateway".to_owned()),
        PeekError::NotFound,
        PeekError::Timeout,
        PeekError::Dropped,
    ] {
        let text = format!("{} {:?}", error.text(), error);
        assert!(!text.contains(&home_text));
        assert!(!text.contains(reply_text));
    }
}

#[tokio::test]
async fn peek_link_sends_only_allowed_methods() {
    let (home, gateway) = home_and_gateway();
    seed(&gateway, "local-3f2a", true, true);
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
    for method in [
        "session.resume",
        "prompt.submit",
        "approval.respond",
        "session.interrupt",
        "session.detach",
    ] {
        assert!(
            !gateway.requests(method).is_empty(),
            "the run never sent {method}"
        );
    }
}
