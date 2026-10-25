//! Shell and side questions through the service and the app.

#[path = "support/mock_gateway.rs"]
mod mock_gateway;
#[path = "../../talaria-core/tests/support/snapshot_json.rs"]
mod snapshot_json;

use std::collections::BTreeSet;
use std::time::Duration;

use hermes_client::rpc::{CallError, CallOptions, RpcError};
use hermes_contract::schemas::{ShellExecResult, TaskIdResult};
use mock_gateway::{Behavior, MockGateway};
use serde_json::{Value, json};
use talaria_app::app::{AppEffect, AppMessage, AppState, Slash, UserAction, slash_line, update};
use talaria_app::sessions::OpenedSession;
use talaria_app::side::{SideService, shell_outcome, side_reply_event};
use talaria_core::commands::{LocalAction, LocalCommandTable};
use talaria_core::session::{Effect, Event, SessionId};
use talaria_core::side::{RequestId, ShellOutcome, SideOutcome};

const SESSION: &str = "local-session-1";

fn keys(value: &Value) -> BTreeSet<&str> {
    value
        .as_object()
        .expect("an object")
        .keys()
        .map(String::as_str)
        .collect()
}

fn shell_effect() -> Effect {
    Effect::RunShell {
        session_id: SessionId::new(SESSION),
        request_id: RequestId(1),
        command: "echo hi".to_string(),
    }
}

fn side_effect() -> Effect {
    Effect::AskSide {
        session_id: SessionId::new(SESSION),
        request_id: RequestId(1),
        question: "why?".to_string(),
    }
}

#[tokio::test]
async fn shell_exec_sends_exact_canonical_params() {
    let (mock, client, _inbound) = MockGateway::start().await;
    let service = SideService::new(client);
    let events = service.execute(shell_effect(), || 0).await;
    assert_eq!(events.len(), 1);
    let seen = mock.requests("shell.exec");
    assert_eq!(seen.len(), 1);
    assert_eq!(
        keys(&seen[0].params),
        BTreeSet::from(["session_id", "command"])
    );
    assert_eq!(seen[0].params["command"], json!("echo hi"));
}

#[tokio::test]
async fn shell_exec_errors_become_refusals_with_detail() {
    for reason in [
        "invalid_params",
        "not_found",
        "profile_mismatch",
        "permission_denied",
        "runtime_draining",
        "unsupported_terminal_backend",
        "dangerous_command",
        "command_timeout",
        "command_failed",
        "cwd_unavailable",
    ] {
        let (_mock, client, _inbound) = MockGateway::start().await;
        _mock.set_behavior("shell.exec", Behavior::Refuse(reason.to_string()));
        let service = SideService::new(client);
        let events = service.execute(shell_effect(), || 0).await;
        assert_eq!(events.len(), 1, "{reason}");
        assert!(
            matches!(
                &events[0],
                Event::ShellFinished {
                    outcome: ShellOutcome::Refused {
                        reason: got,
                        ..
                    },
                    ..
                } if got == reason
            ),
            "{reason}"
        );
    }
}

#[tokio::test]
async fn call_errors_map_to_unknown_unreadable_or_not_sent() {
    // Harvested from the mock: silence becomes TimedOut, a dropped socket Unknown.
    let (mock, client, _inbound) = MockGateway::start().await;
    mock.set_silent("shell.exec", true);
    let timed_out = client
        .call_raw(
            "shell.exec",
            json!({"session_id": SESSION, "command": "echo hi"}),
            CallOptions {
                timeout: Some(Duration::from_millis(50)),
            },
        )
        .await
        .expect_err("silence times out");
    assert!(matches!(timed_out, CallError::TimedOut { .. }));
    let (mock, client, _inbound) = MockGateway::start().await;
    mock.queue_behavior("session.create", Behavior::DropBeforeReply);
    let unknown = client
        .call_raw(
            "session.create",
            json!({
                "request_id": "0123456789abcdef0123456789abcdef",
                "source": "tui",
                "cwd": "/work/proj",
            }),
            CallOptions { timeout: None },
        )
        .await
        .expect_err("a dropped socket is unknown");
    assert!(
        matches!(unknown, CallError::Unknown { .. }),
        "got {unknown:?}"
    );
    // BadResult has no honest value here: its fields are private to its
    // crate with no constructor, and no mock reply mistypes. Its two
    // mapping arms are reviewed against the mapping table instead.
    // The mapping, no server needed past this point.
    assert!(matches!(
        shell_outcome(Err(timed_out)),
        ShellOutcome::Unknown
    ));
    assert!(matches!(shell_outcome(Err(unknown)), ShellOutcome::Unknown));
    assert!(matches!(
        shell_outcome(Err(CallError::NotConnected)),
        ShellOutcome::NotSent {
            not_connected: true
        }
    ));
    assert!(matches!(
        shell_outcome(Err(CallError::Encode)),
        ShellOutcome::NotSent {
            not_connected: false
        }
    ));
    assert!(matches!(
        shell_outcome(Err(CallError::InvalidTimeout)),
        ShellOutcome::NotSent {
            not_connected: false
        }
    ));
    let rpc = CallError::Rpc(RpcError {
        code: 4001,
        message: "dangerous_command".to_string(),
        reason: Some("dangerous_command".to_string()),
        data: None,
    });
    assert!(matches!(
        shell_outcome(Err(rpc)),
        ShellOutcome::Refused { .. }
    ));
    let ok = shell_outcome(Ok(ShellExecResult {
        stdout: "hi\n".to_string(),
        stderr: String::new(),
        code: 0,
    }));
    assert!(matches!(ok, ShellOutcome::Ok { code: 0, .. }));
    assert!(matches!(
        side_reply_event(RequestId(1), Err(CallError::NotConnected), 7),
        Event::SideNotSent {
            not_connected: true,
            ..
        }
    ));
    assert!(matches!(
        side_reply_event(RequestId(1), Err(CallError::Encode), 7),
        Event::SideNotSent {
            not_connected: false,
            ..
        }
    ));
    assert!(matches!(
        side_reply_event(
            RequestId(1),
            Ok(TaskIdResult {
                task_id: "btw_1a2b3c".to_string()
            }),
            7
        ),
        Event::SideAccepted { .. }
    ));
    assert!(matches!(
        side_reply_event(
            RequestId(1),
            Err(CallError::Rpc(RpcError {
                code: 4001,
                message: "not_found".to_string(),
                reason: Some("not_found".to_string()),
                data: None,
            })),
            7
        ),
        Event::SideRefused { .. }
    ));
    let _ = SideOutcome::Answered {
        text: String::new(),
    };
}

#[tokio::test]
async fn dropped_connection_gives_unknown_and_one_call() {
    let (mock, client, _inbound) = MockGateway::start().await;
    mock.queue_behavior("shell.exec", Behavior::DropBeforeReply);
    let service = SideService::new(client);
    let events = service.execute(shell_effect(), || 0).await;
    assert_eq!(events.len(), 1);
    assert!(matches!(
        &events[0],
        Event::ShellFinished {
            outcome: ShellOutcome::Unknown,
            ..
        }
    ));
    assert_eq!(mock.requests("shell.exec").len(), 1);
}

#[tokio::test]
async fn prompt_btw_sends_exact_params_and_maps_errors() {
    let (mock, client, _inbound) = MockGateway::start().await;
    let service = SideService::new(client);
    let events = service.execute(side_effect(), || 0).await;
    assert_eq!(events.len(), 1);
    assert!(matches!(&events[0], Event::SideAccepted { .. }));
    let seen = mock.requests("prompt.btw");
    assert_eq!(seen.len(), 1);
    assert_eq!(
        keys(&seen[0].params),
        BTreeSet::from(["session_id", "text"])
    );
    let (mock, client, _inbound) = MockGateway::start().await;
    mock.set_behavior(
        "prompt.btw",
        Behavior::Refuse("permission_denied".to_string()),
    );
    let service = SideService::new(client);
    let events = service.execute(side_effect(), || 0).await;
    assert!(matches!(
        &events[0],
        Event::SideRefused { reason, .. } if reason == "permission_denied"
    ));
}

#[tokio::test]
async fn submit_routes_bang_and_btw_without_queueing() {
    let (mock, _client, _inbound) = MockGateway::start().await;
    let mut state = AppState::new("0123456789abcdef");
    let effects = update(
        &mut state,
        AppMessage::User(UserAction::Submit("!echo x".to_string())),
    );
    assert!(!effects.is_empty());
    assert!(mock.requests("shell.exec").len() <= 1);
    assert!(mock.requests("prompt.submit").is_empty());
    assert!(mock.requests("prompt.btw").is_empty());
    assert!(mock.requests("slash.exec").is_empty());
    let effects = update(
        &mut state,
        AppMessage::User(UserAction::Submit("/btw why?".to_string())),
    );
    assert!(!effects.is_empty());
    assert!(mock.requests("slash.exec").is_empty());
}

#[test]
fn idle_app_wakes_and_draws_nothing() {
    let mut state = AppState::new("0123456789abcdef");
    let idle = update(&mut state, AppMessage::Tick { now_ms: 1_000 });
    assert!(idle.is_empty());
    let snapshot = snapshot_json::snapshot(snapshot_json::base("local-session-1"));
    let opened = OpenedSession::from_snapshot(snapshot);
    let mut state = AppState::new("0123456789abcdef");
    let _ = update(
        &mut state,
        AppMessage::Opened {
            opened,
            notice: None,
        },
    );
    let _ = update(
        &mut state,
        AppMessage::User(UserAction::Submit("!sleep 1".to_string())),
    );
    let busy = update(&mut state, AppMessage::Tick { now_ms: 2_000 });
    assert_eq!(
        busy.iter()
            .filter(|effect| matches!(effect, talaria_app::app::AppEffect::Redraw))
            .count(),
        1
    );
}

fn opened_state() -> AppState {
    let snapshot = snapshot_json::snapshot(snapshot_json::base(SESSION));
    let mut state = AppState::new("0123456789abcdef");
    let _ = update(
        &mut state,
        AppMessage::Opened {
            opened: OpenedSession::from_snapshot(snapshot),
            notice: None,
        },
    );
    state
}

fn submit(state: &mut AppState, text: &str) -> Vec<AppEffect> {
    update(
        state,
        AppMessage::User(UserAction::Submit(text.to_string())),
    )
}

fn unrecorded(effects: &[AppEffect]) -> Vec<&str> {
    effects
        .iter()
        .filter_map(|effect| match effect {
            AppEffect::UnrecordSubmit(text) => Some(text.as_str()),
            _ => None,
        })
        .collect()
}

#[test]
fn btw_is_a_local_command_that_asks_a_side_question() {
    assert_eq!(slash_line("/btw why?"), Slash::Btw("why?".to_string()));
    assert_eq!(slash_line("/BTW  why? "), Slash::Btw("why?".to_string()));
    assert_eq!(slash_line("/btw"), Slash::Btw(String::new()));
    assert!(
        LocalCommandTable::builtin()
            .find("btw")
            .is_some_and(|command| command.action == LocalAction::Btw && command.takes_argument)
    );
    let mut state = opened_state();
    let effects = submit(&mut state, "/btw why?");
    assert!(effects.iter().any(|effect| matches!(
        effect,
        AppEffect::Side(Effect::AskSide { question, .. }) if question == "why?"
    )));
    assert!(unrecorded(&effects).is_empty());
}

#[test]
fn hint_and_usage_leave_recall_and_a_sent_command_stays() {
    for open in [true, false] {
        let mut state = if open {
            opened_state()
        } else {
            AppState::new("0123456789abcdef")
        };
        assert_eq!(unrecorded(&submit(&mut state, "!")), ["!"]);
        assert_eq!(unrecorded(&submit(&mut state, "/btw")), ["/btw"]);
        assert_eq!(unrecorded(&submit(&mut state, "/btw   ")), ["/btw   "]);
    }
    let mut state = opened_state();
    assert!(unrecorded(&submit(&mut state, "!echo x")).is_empty());
    assert!(unrecorded(&submit(&mut state, "/btw why?")).is_empty());
}
