//! Background calls on the mock gateway, and an idle app clock.

#[path = "support/mock_gateway.rs"]
mod mock_gateway;

use std::collections::BTreeSet;
use std::time::Duration;

use hermes_client::rpc::{CallError, RpcError};
use hermes_contract::schemas::{ProcessKillResult, ProcessKillStatus, ProcessStopResult};
use mock_gateway::MockGateway;
use serde_json::{Value, json};
use talaria_app::app::{AppEffect, AppMessage, AppState, update as app_update};
use talaria_app::background::{BackgroundService, call_failure, kill_outcome, stop_outcome};
use talaria_app::sessions::{OpenedSession, SessionInfoSummary};
use talaria_core::background::{
    BackgroundEffect, BackgroundEvent, BackgroundWork, CallFailure, KillOutcome, ListedProcess,
    ProcessId, StopOutcome, next_deadline, update,
};
use talaria_core::session::{SessionId, SessionState};
use talaria_core::side::RequestId;
use talaria_tui::live::FrameStats;

fn keys_of(value: &Value) -> BTreeSet<&str> {
    value
        .as_object()
        .map(|object| object.keys().map(String::as_str).collect())
        .unwrap_or_default()
}

fn rpc(code: i64, reason: Option<&str>, detail: Option<&str>) -> CallError {
    let data = match (reason, detail) {
        (None, None) => None,
        _ => Some(json!({
            "reason": reason,
            "detail": detail,
        })),
    };
    CallError::Rpc(RpcError {
        code,
        message: reason.unwrap_or("error").to_string(),
        reason: reason.map(str::to_owned),
        data,
    })
}

fn kill_result(status: ProcessKillStatus, error: Option<&str>) -> ProcessKillResult {
    ProcessKillResult {
        command: Some("sh tick.sh".to_string()),
        completion_reason: Some(status.to_string()),
        error: error.map(str::to_owned),
        exit_code: Some(-15),
        output: None,
        session_id: Some("proc_1".to_string()),
        status,
        termination_source: Some("process.kill".to_string()),
    }
}

fn opened(id: &str) -> OpenedSession {
    OpenedSession {
        state: SessionState::empty(SessionId::new(id)),
        session_id: id.to_string(),
        stored_session_id: id.to_string(),
        subscription_id: format!("sub-{id}"),
        info: SessionInfoSummary {
            model: None,
            provider: None,
            cwd: None,
            branch: None,
        },
        title: None,
        cursor: hermes_client::replay::Cursor {
            replay_epoch: "epoch".to_string(),
            last_sequence: 0,
        },
        revision: 0,
        execution_generation: 0,
        launch_notices: Vec::new(),
        effort: None,
    }
}

fn lists_for(gateway: &MockGateway, session: &str) -> usize {
    gateway
        .requests("process.list")
        .iter()
        .filter(|seen| seen.params.get("session_id").and_then(Value::as_str) == Some(session))
        .count()
}

#[tokio::test]
async fn calls_send_only_the_method_keys() {
    let (gateway, client, _inbound) = MockGateway::start().await;
    gateway.enable_background();
    let session_id = gateway.session_id();
    gateway.set_background_processes(
        &session_id,
        vec![json!({
            "session_id": "proc_1",
            "command": "sh tick.sh",
            "status": "running",
        })],
    );
    let service = BackgroundService::new(client);
    let session = SessionId::new(session_id);
    let now = || 0_u64;
    let _ = service
        .execute(
            &session,
            BackgroundEffect::ListProcesses {
                request: RequestId(1),
            },
            now,
        )
        .await;
    let _ = service
        .execute(
            &session,
            BackgroundEffect::KillProcess {
                request: RequestId(2),
                id: ProcessId("proc_1".to_string()),
            },
            now,
        )
        .await;
    let _ = service
        .execute(
            &session,
            BackgroundEffect::StopProcesses {
                request: RequestId(3),
            },
            now,
        )
        .await;
    let _ = service
        .execute(
            &session,
            BackgroundEffect::StartBackground {
                request: RequestId(4),
                text: "Reply with exactly: bg-ok".to_string(),
            },
            now,
        )
        .await;

    let list = gateway.requests("process.list");
    assert_eq!(list.len(), 1);
    assert_eq!(keys_of(&list[0].params), BTreeSet::from(["session_id"]));
    let kill = gateway.requests("process.kill");
    assert_eq!(kill.len(), 1);
    assert_eq!(
        keys_of(&kill[0].params),
        BTreeSet::from(["session_id", "process_id"])
    );
    let stop = gateway.requests("process.stop");
    assert_eq!(stop.len(), 1);
    assert_eq!(keys_of(&stop[0].params), BTreeSet::from(["session_id"]));
    let background = gateway.requests("prompt.background");
    assert_eq!(background.len(), 1);
    assert_eq!(
        keys_of(&background[0].params),
        BTreeSet::from(["session_id", "text"])
    );
    let methods: BTreeSet<String> = gateway
        .seen()
        .into_iter()
        .filter_map(|seen| seen.method)
        .collect();
    assert!(!methods.contains("slash.exec"));
    assert!(!methods.contains("command.dispatch"));
}

#[test]
fn call_failures_map_onto_the_notes() {
    assert_eq!(call_failure(&CallError::NotConnected), CallFailure::NotSent);
    assert_eq!(call_failure(&CallError::Encode), CallFailure::NotSent);
    assert_eq!(
        call_failure(&CallError::InvalidTimeout),
        CallFailure::NotSent
    );
    assert_eq!(
        call_failure(&rpc(4001, Some("not_found"), Some("gone"))),
        CallFailure::Refused {
            reason: "not_found".to_string(),
            detail: Some("gone".to_string()),
        }
    );
    assert_eq!(
        call_failure(&rpc(4001, None, None)),
        CallFailure::Refused {
            reason: String::new(),
            detail: None,
        }
    );
    assert_eq!(
        call_failure(&rpc(5001, Some("storage_unavailable"), None)),
        CallFailure::Storage
    );
    assert_eq!(
        call_failure(&rpc(-32601, Some("unknown_method"), None)),
        CallFailure::MethodMissing
    );
    assert_eq!(
        call_failure(&rpc(404, Some("nope"), None)),
        CallFailure::Refused {
            reason: "nope".to_string(),
            detail: None,
        }
    );
    assert_eq!(
        call_failure(&rpc(409, None, None)),
        CallFailure::Refused {
            reason: "error 409".to_string(),
            detail: None,
        }
    );
    assert_eq!(
        call_failure(&CallError::BadResult {
            method: "process.list".to_string(),
            reason: "result did not decode",
        }),
        CallFailure::Unreadable
    );
    assert_eq!(
        call_failure(&CallError::TimedOut {
            method: "process.list".to_string(),
            after: Duration::from_secs(10),
        }),
        CallFailure::Unknown
    );
    assert_eq!(
        call_failure(&CallError::Unknown {
            method: "process.kill".to_string(),
        }),
        CallFailure::Unknown
    );

    assert_eq!(
        kill_outcome(Ok(kill_result(ProcessKillStatus::Killed, None))),
        KillOutcome::Killed
    );
    assert_eq!(
        kill_outcome(Ok(kill_result(ProcessKillStatus::AlreadyExited, None))),
        KillOutcome::AlreadyExited {
            scope_stop_failed: false,
        }
    );
    assert_eq!(
        kill_outcome(Ok(kill_result(ProcessKillStatus::NotFound, None))),
        KillOutcome::NotFound
    );
    assert_eq!(
        kill_outcome(Ok(kill_result(
            ProcessKillStatus::Error,
            Some("still running")
        ))),
        KillOutcome::Error {
            text: "still running".to_string(),
        }
    );
    assert_eq!(
        kill_outcome(Err(CallError::NotConnected)),
        KillOutcome::Failed(CallFailure::NotSent)
    );
    assert_eq!(
        stop_outcome(Ok(ProcessStopResult {
            killed: 2,
            delegations: 1,
        })),
        StopOutcome::Stopped {
            killed: 2,
            delegations: 1,
        }
    );
    assert_eq!(
        stop_outcome(Err(rpc(-32601, Some("unknown_method"), None))),
        StopOutcome::Failed(CallFailure::MethodMissing)
    );
    let (level, text) = talaria_core::background::stop_notice_text(&StopOutcome::Failed(
        CallFailure::MethodMissing,
    ));
    assert_eq!(level, talaria_core::background::NoticeLevel::Error);
    assert_eq!(text, "This Hermes does not offer process.stop.");
}

#[tokio::test]
async fn only_the_foreground_session_is_listed() {
    let mut primary = BackgroundWork::default();
    let _ = update(&mut primary, BackgroundEvent::Attached { now_ms: 0 });
    let first = update(
        &mut primary,
        BackgroundEvent::ForegroundChanged {
            foreground: true,
            now_ms: 0,
        },
    );
    let primary_request = first
        .iter()
        .find_map(|effect| match effect {
            BackgroundEffect::ListProcesses { request } => Some(*request),
            _ => None,
        })
        .expect("launch lists the foreground");
    let _ = update(
        &mut primary,
        BackgroundEvent::Listed {
            request: primary_request,
            processes: vec![ListedProcess {
                id: ProcessId("a-row".to_string()),
                command: "sleep 1".to_string(),
                status_raw: "running".to_string(),
                exit_code: None,
                started_epoch_s: Some(1.0),
                exited_at_s: None,
                completion_reason: None,
                output_tail: None,
            }],
            now_ms: 1,
        },
    );
    let again = update(
        &mut primary,
        BackgroundEvent::RefreshRequested { now_ms: 2 },
    );
    let in_flight = again
        .iter()
        .find_map(|effect| match effect {
            BackgroundEffect::ListProcesses { request } => Some(*request),
            _ => None,
        })
        .expect("a refresh lists");
    let _ = update(
        &mut primary,
        BackgroundEvent::ForegroundChanged {
            foreground: false,
            now_ms: 3,
        },
    );
    let mut other = BackgroundWork::default();
    let _ = update(&mut other, BackgroundEvent::Attached { now_ms: 3 });
    let _ = update(
        &mut other,
        BackgroundEvent::ForegroundChanged {
            foreground: true,
            now_ms: 3,
        },
    );
    let _ = update(
        &mut primary,
        BackgroundEvent::Listed {
            request: in_flight,
            processes: vec![ListedProcess {
                id: ProcessId("late".to_string()),
                command: "sleep 2".to_string(),
                status_raw: "running".to_string(),
                exit_code: None,
                started_epoch_s: Some(2.0),
                exited_at_s: None,
                completion_reason: None,
                output_tail: None,
            }],
            now_ms: 4,
        },
    );
    assert!(primary.rows().any(|row| row.id.0 == "late"));
    assert!(other.rows().all(|row| row.id.0 != "late"));

    let _ = update(&mut primary, BackgroundEvent::OverlayOpened { now_ms: 5 });
    assert!(primary.overlay_open());
    let _ = update(&mut primary, BackgroundEvent::OverlayClosed);
    assert!(!primary.overlay_open());

    let _ = update(
        &mut primary,
        BackgroundEvent::ForegroundChanged {
            foreground: true,
            now_ms: 6,
        },
    );
    let reconnect = update(&mut primary, BackgroundEvent::Attached { now_ms: 7 });
    assert_eq!(
        reconnect
            .iter()
            .filter(|effect| matches!(effect, BackgroundEffect::ListProcesses { .. }))
            .count(),
        1
    );
    let version = primary.version();
    let _ = update(
        &mut primary,
        BackgroundEvent::Listed {
            request: in_flight,
            processes: vec![ListedProcess {
                id: ProcessId("stale-gen".to_string()),
                command: "sleep 3".to_string(),
                status_raw: "running".to_string(),
                exit_code: None,
                started_epoch_s: Some(3.0),
                exited_at_s: None,
                completion_reason: None,
                output_tail: None,
            }],
            now_ms: 8,
        },
    );
    assert_eq!(primary.version(), version);
    assert!(primary.rows().all(|row| row.id.0 != "stale-gen"));

    let (gateway, _client, _inbound) = MockGateway::start().await;
    gateway.enable_background();
    let mut app = AppState::new("launch-bg");
    app.profile = "fixture".to_string();
    let _ = app_update(
        &mut app,
        AppMessage::Opened {
            opened: opened("session-a"),
            notice: None,
        },
    );
    assert_eq!(lists_for(&gateway, "session-a"), 1, "launch lists once");
    let _ = app_update(
        &mut app,
        AppMessage::Opened {
            opened: opened("session-b"),
            notice: None,
        },
    );
    assert_eq!(
        lists_for(&gateway, "session-b"),
        1,
        "a switch lists the new session once"
    );
    assert_eq!(
        lists_for(&gateway, "session-a"),
        1,
        "a switch does not list the old session again"
    );
    let _ = app_update(
        &mut app,
        AppMessage::Opened {
            opened: opened("session-a"),
            notice: None,
        },
    );
    assert_eq!(
        lists_for(&gateway, "session-a"),
        2,
        "a reconnect lists once more"
    );
}

#[tokio::test]
async fn idle_work_schedules_nothing() {
    assert_eq!(next_deadline(&BackgroundWork::default(), 0), None);

    let mut work = BackgroundWork::default();
    let _ = update(&mut work, BackgroundEvent::Attached { now_ms: 0 });
    let started = update(
        &mut work,
        BackgroundEvent::ForegroundChanged {
            foreground: true,
            now_ms: 0,
        },
    );
    let request = started
        .iter()
        .find_map(|effect| match effect {
            BackgroundEffect::ListProcesses { request } => Some(*request),
            _ => None,
        })
        .expect("the foreground lists");
    let _ = update(
        &mut work,
        BackgroundEvent::Listed {
            request,
            processes: vec![ListedProcess {
                id: ProcessId("tick".to_string()),
                command: "sh tick.sh".to_string(),
                status_raw: "running".to_string(),
                exit_code: None,
                started_epoch_s: Some(0.0),
                exited_at_s: None,
                completion_reason: None,
                output_tail: None,
            }],
            now_ms: 1,
        },
    );
    let bg = update(
        &mut work,
        BackgroundEvent::BgCommand {
            text: "stay running".to_string(),
            now_ms: 1,
        },
    );
    let bg_request = bg.iter().find_map(|effect| match effect {
        BackgroundEffect::StartBackground { request, .. } => Some(*request),
        _ => None,
    });
    if let Some(bg_request) = bg_request {
        let _ = update(
            &mut work,
            BackgroundEvent::BgAccepted {
                request: bg_request,
                task_id: "bg_000001".to_string(),
                now_ms: 1,
            },
        );
    }
    let mut now = 0_u64;
    let mut listed = false;
    for _ in 0..6 {
        let due = next_deadline(&work, now).expect("a running process wakes within a second");
        assert!(
            due <= now + 1_000,
            "wake-up {due} is more than a second after {now}"
        );
        now = due.max(now + 1);
        let effects = update(
            &mut work,
            BackgroundEvent::Clock {
                now_ms: now,
                wall_s: now as f64 / 1_000.0,
            },
        );
        listed |= effects
            .iter()
            .any(|effect| matches!(effect, BackgroundEffect::ListProcesses { .. }));
    }
    assert!(
        listed,
        "six seconds with a running process list at least once"
    );

    let (gateway, _client, _inbound) = MockGateway::start().await;
    gateway.enable_background();
    let mut app = AppState::new("launch-bg");
    app.profile = "fixture".to_string();
    let _ = app_update(
        &mut app,
        AppMessage::Opened {
            opened: opened(&gateway.session_id()),
            notice: None,
        },
    );
    let mut drew = 0_u32;
    for step in 1..=6 {
        let effects = app_update(
            &mut app,
            AppMessage::Tick {
                now_ms: step * 1_000,
            },
        );
        if effects
            .iter()
            .any(|effect| matches!(effect, AppEffect::Redraw))
        {
            drew += 1;
        }
    }
    assert!(
        gateway.requests("process.list").len() >= 1,
        "the app listed the foreground session during the six seconds"
    );
    assert!(drew >= 1, "the running row wakes the screen");

    let request = update(&mut work, BackgroundEvent::RefreshRequested { now_ms: now })
        .iter()
        .find_map(|effect| match effect {
            BackgroundEffect::ListProcesses { request } => Some(*request),
            _ => None,
        })
        .unwrap_or(request);
    let _ = update(
        &mut work,
        BackgroundEvent::Listed {
            request,
            processes: vec![ListedProcess {
                id: ProcessId("tick".to_string()),
                command: "sh tick.sh".to_string(),
                status_raw: "exited".to_string(),
                exit_code: Some(0),
                started_epoch_s: Some(0.0),
                exited_at_s: Some(1.0),
                completion_reason: Some("exited".to_string()),
                output_tail: None,
            }],
            now_ms: now + 1,
        },
    );
    if let Some(task) = work.bg_tasks().first().map(|task| task.request) {
        let _ = update(
            &mut work,
            BackgroundEvent::BgCompleted {
                task_id: "bg_000001".to_string(),
                text: "done".to_string(),
                now_ms: now + 1,
            },
        );
        let _ = task;
    }
    let idle_at = now + 61_000;
    let _ = update(
        &mut work,
        BackgroundEvent::Clock {
            now_ms: idle_at,
            wall_s: idle_at as f64 / 1_000.0,
        },
    );
    assert_eq!(next_deadline(&work, idle_at), None);
    let seen_before = gateway.seen().len();
    let drew_before = drew;
    let mut stats = FrameStats {
        drew: false,
        bytes: 0,
    };
    for step in 1..=60 {
        let effects = app_update(
            &mut app,
            AppMessage::Tick {
                now_ms: idle_at + step * 1_000,
            },
        );
        if effects
            .iter()
            .any(|effect| matches!(effect, AppEffect::Redraw))
        {
            stats.drew = true;
            drew += 1;
        }
    }
    assert!(!stats.drew, "an idle minute draws no frame");
    assert_eq!(drew, drew_before);
    assert_eq!(
        gateway.seen().len(),
        seen_before,
        "an idle minute sends no request"
    );
    assert_eq!(next_deadline(&work, idle_at + 60_000), None);
}
