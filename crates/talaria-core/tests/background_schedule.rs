//! Stop, kill, list failures, and the three local commands.

use talaria_core::background::{
    BackgroundEffect, BackgroundEvent, BackgroundWork, CallFailure, ListState, ListStop,
    ListedProcess, MAX_POLL_BACKOFF_MS, NoticeLevel, OVERLAY_POLL_MS, PROCESS_POLL_MS, ProcessId,
    StopOrigin, StopOutcome, is_process_tool, update,
};
use talaria_core::commands::{LocalAction, LocalCommandTable};
use talaria_core::side::RequestId;

fn pid(id: &str) -> ProcessId {
    ProcessId(id.to_string())
}

fn running(id: &str) -> ListedProcess {
    ListedProcess {
        id: pid(id),
        command: format!("sh {id}.sh"),
        status_raw: "running".to_string(),
        exit_code: None,
        started_epoch_s: Some(10.0),
        exited_at_s: None,
        completion_reason: None,
        output_tail: None,
    }
}

fn foreground(work: &mut BackgroundWork) -> RequestId {
    let _ = update(work, BackgroundEvent::Attached { now_ms: 0 });
    let effects = update(
        work,
        BackgroundEvent::ForegroundChanged {
            foreground: true,
            now_ms: 0,
        },
    );
    list_id(&effects)
}

fn list_id(effects: &[BackgroundEffect]) -> RequestId {
    effects
        .iter()
        .find_map(|effect| match effect {
            BackgroundEffect::ListProcesses { request } => Some(*request),
            _ => None,
        })
        .expect("a list was requested")
}

fn stop_request(effects: &[BackgroundEffect]) -> RequestId {
    effects
        .iter()
        .find_map(|effect| match effect {
            BackgroundEffect::StopProcesses { request } => Some(*request),
            _ => None,
        })
        .expect("a stop was requested")
}

fn lists(effects: &[BackgroundEffect]) -> usize {
    effects
        .iter()
        .filter(|effect| matches!(effect, BackgroundEffect::ListProcesses { .. }))
        .count()
}

#[test]
fn stop_while_idle_does_not_interrupt() {
    let mut work = BackgroundWork::default();
    let _ = foreground(&mut work);
    let idle = update(
        &mut work,
        BackgroundEvent::StopCommand {
            turn_running: false,
            now_ms: 1,
        },
    );
    assert!(
        idle.iter()
            .all(|effect| !matches!(effect, BackgroundEffect::Interrupt))
    );
    assert!(lists(&idle) == 0);
    assert!(
        idle.iter()
            .any(|effect| matches!(effect, BackgroundEffect::StopProcesses { .. }))
    );
    let second = update(
        &mut work,
        BackgroundEvent::StopCommand {
            turn_running: true,
            now_ms: 2,
        },
    );
    assert!(second.is_empty());
    assert_eq!(
        work.overlay_note(),
        Some("Already stopping; waiting for Hermes.")
    );
    let mut fresh = BackgroundWork::default();
    let _ = foreground(&mut fresh);
    let busy = update(
        &mut fresh,
        BackgroundEvent::StopCommand {
            turn_running: true,
            now_ms: 2,
        },
    );
    assert!(matches!(busy.first(), Some(BackgroundEffect::Interrupt)));
    assert!(
        busy.iter()
            .any(|effect| matches!(effect, BackgroundEffect::StopProcesses { .. }))
    );
    let overlay = update(&mut fresh, BackgroundEvent::StopAllRequested { now_ms: 3 });
    assert!(overlay.is_empty());
    let _ = StopOrigin::Overlay;
}

#[test]
fn stop_results_use_the_plural_rule() {
    let pairs = [
        (0, 0, "No background processes were running."),
        (1, 0, "Stopped 1 background process."),
        (2, 0, "Stopped 2 background processes."),
        (
            2,
            1,
            "Stopped 2 background processes. Also interrupted 1 background delegation.",
        ),
        (
            0,
            3,
            "No background processes were running. Also interrupted 3 background delegations.",
        ),
    ];
    for (killed, delegations, text) in pairs {
        let mut work = BackgroundWork::default();
        let _ = foreground(&mut work);
        let started = update(
            &mut work,
            BackgroundEvent::StopCommand {
                turn_running: false,
                now_ms: 1,
            },
        );
        let request = stop_request(&started);
        let effects = update(
            &mut work,
            BackgroundEvent::StopReplied {
                request,
                outcome: StopOutcome::Stopped {
                    killed,
                    delegations,
                },
                now_ms: 2,
            },
        );
        let notices: Vec<_> = effects
            .iter()
            .filter_map(|effect| match effect {
                BackgroundEffect::InsertNotice { level, text } => Some((*level, text.as_str())),
                _ => None,
            })
            .collect();
        assert_eq!(notices, [(NoticeLevel::Info, text)]);
        let mut overlay = BackgroundWork::default();
        let _ = foreground(&mut overlay);
        let started = update(
            &mut overlay,
            BackgroundEvent::StopAllRequested { now_ms: 1 },
        );
        let request = stop_request(&started);
        let effects = update(
            &mut overlay,
            BackgroundEvent::StopReplied {
                request,
                outcome: StopOutcome::Stopped {
                    killed,
                    delegations,
                },
                now_ms: 2,
            },
        );
        assert!(
            effects
                .iter()
                .all(|effect| !matches!(effect, BackgroundEffect::InsertNotice { .. }))
        );
        assert_eq!(overlay.overlay_note(), Some(text));
    }
}

#[test]
fn kill_outcomes_refresh_the_list() {
    let mut work = BackgroundWork::default();
    let request = foreground(&mut work);
    let _ = update(
        &mut work,
        BackgroundEvent::Listed {
            request,
            processes: vec![running("tick")],
            now_ms: 1,
        },
    );
    let effects = update(
        &mut work,
        BackgroundEvent::KillRequested {
            id: pid("tick"),
            now_ms: 2,
        },
    );
    let kill_request = effects
        .iter()
        .find_map(|effect| match effect {
            BackgroundEffect::KillProcess { request, id } if id.0 == "tick" => Some(*request),
            _ => None,
        })
        .expect("a kill was requested");
    let again = update(
        &mut work,
        BackgroundEvent::KillRequested {
            id: pid("tick"),
            now_ms: 3,
        },
    );
    assert!(again.is_empty());
    assert_eq!(
        work.overlay_note()
            .or_else(|| work.rows().next().and_then(|row| match &row.kill {
                talaria_core::background::KillState::Note { text } => Some(text.as_str()),
                _ => None,
            })),
        Some("Already killing this process; waiting for Hermes.")
    );
    let finished = ListedProcess {
        status_raw: "exited".to_string(),
        exit_code: Some(0),
        ..running("done")
    };
    let request = foreground_refresh(&mut work, 4);
    let _ = update(
        &mut work,
        BackgroundEvent::Listed {
            request,
            processes: vec![running("tick"), finished],
            now_ms: 5,
        },
    );
    let refused = update(
        &mut work,
        BackgroundEvent::KillRequested {
            id: pid("done"),
            now_ms: 6,
        },
    );
    assert!(refused.is_empty());
    let request = {
        let killed = update(
            &mut work,
            BackgroundEvent::KillReplied {
                request: kill_request,
                process_id: pid("tick"),
                outcome: talaria_core::background::KillOutcome::Killed,
                now_ms: 7,
            },
        );
        assert!(lists(&killed) == 1);
        let row = work.rows().find(|row| row.id.0 == "tick").unwrap();
        assert!(matches!(
            row.status,
            talaria_core::background::ProcessStatus::Killed
        ));
        assert_eq!(row.exit_code, Some(-15));
        list_id(&killed)
    };
    let _ = request;
}

fn foreground_refresh(work: &mut BackgroundWork, now_ms: u64) -> RequestId {
    list_id(&update(work, BackgroundEvent::RefreshRequested { now_ms }))
}

#[test]
fn list_failures_stop_or_go_stale() {
    assert!(is_process_tool("terminal"));
    assert!(is_process_tool("process_manage"));
    assert!(is_process_tool("process"));
    assert!(!is_process_tool("shell"));
    let stopping = [
        (
            CallFailure::Refused {
                reason: "not_found".into(),
                detail: None,
            },
            ListStop::SessionNotLive,
        ),
        (
            CallFailure::Refused {
                reason: "unsupported_projection".into(),
                detail: None,
            },
            ListStop::Unsupported,
        ),
        (CallFailure::MethodMissing, ListStop::MethodMissing),
        (
            CallFailure::Refused {
                reason: "permission_denied".into(),
                detail: None,
            },
            ListStop::PermissionDenied,
        ),
    ];
    for (failure, why) in stopping {
        let mut work = BackgroundWork::default();
        let request = foreground(&mut work);
        let _ = update(
            &mut work,
            BackgroundEvent::ListFailed {
                request,
                failure,
                now_ms: 1,
            },
        );
        assert!(matches!(work.list_state(), ListState::Stopped { why: got } if *got == why));
        for event in [
            BackgroundEvent::OverlayOpened { now_ms: 2 },
            BackgroundEvent::RefreshRequested {
                now_ms: PROCESS_POLL_MS + 5,
            },
            BackgroundEvent::Clock {
                now_ms: PROCESS_POLL_MS + 5,
                wall_s: 20.0,
            },
            BackgroundEvent::ToolCompleted {
                name: "terminal".into(),
                now_ms: 6,
            },
        ] {
            assert_eq!(lists(&update(&mut work, event)), 0);
        }
        let _ = update(
            &mut work,
            BackgroundEvent::ForegroundChanged {
                foreground: true,
                now_ms: 7,
            },
        );
        let attached = update(&mut work, BackgroundEvent::Attached { now_ms: 8 });
        assert_eq!(lists(&attached), 1);
    }
    let stale = [
        CallFailure::Refused {
            reason: "stale_generation".into(),
            detail: None,
        },
        CallFailure::Refused {
            reason: "runtime_draining".into(),
            detail: None,
        },
        CallFailure::Storage,
        CallFailure::Unknown,
        CallFailure::NotSent,
        CallFailure::Refused {
            reason: "invalid_params".into(),
            detail: None,
        },
        CallFailure::Refused {
            reason: "other_reason".into(),
            detail: None,
        },
    ];
    for failure in stale {
        let mut work = BackgroundWork::default();
        let request = foreground(&mut work);
        let effects = update(
            &mut work,
            BackgroundEvent::ListFailed {
                request,
                failure: failure.clone(),
                now_ms: 1_000,
            },
        );
        assert!(matches!(work.list_state(), ListState::Stale { .. }));
        let logged = effects
            .iter()
            .any(|effect| matches!(effect, BackgroundEffect::Log { .. }));
        let reason = match &failure {
            CallFailure::Refused { reason, .. } => reason.as_str(),
            _ => "",
        };
        if reason == "invalid_params" || reason == "other_reason" {
            assert!(logged);
        }
        let quiet = update(
            &mut work,
            BackgroundEvent::Clock {
                now_ms: 1_000 + PROCESS_POLL_MS - 1,
                wall_s: 2.0,
            },
        );
        assert_eq!(lists(&quiet), 0);
        let due = 1_000 + (PROCESS_POLL_MS * 2).min(MAX_POLL_BACKOFF_MS);
        let retry = update(
            &mut work,
            BackgroundEvent::Clock {
                now_ms: due,
                wall_s: 3.0,
            },
        );
        assert_eq!(lists(&retry), 1);
        let _ = OVERLAY_POLL_MS;
    }
}

#[test]
fn local_commands_include_the_background_alias() {
    let table = LocalCommandTable::builtin();
    let names: Vec<_> = table.iter().map(|command| command.name).collect();
    let processes_at = names.iter().position(|name| *name == "processes").unwrap();
    let bg_at = names.iter().position(|name| *name == "bg").unwrap();
    let stop_at = names.iter().position(|name| *name == "stop").unwrap();
    assert!(processes_at < bg_at && bg_at < stop_at);
    assert!(processes_at > names.iter().position(|name| *name == "diff").unwrap_or(0));
    let processes = table.find("processes").unwrap();
    assert!(processes.aliases.is_empty());
    assert_eq!(processes.args_hint, "");
    assert!(!processes.takes_argument);
    assert_eq!(
        processes.summary,
        "Show this session's background processes, with live output and kill"
    );
    assert_eq!(processes.action, LocalAction::Processes);
    let bg = table.find("bg").unwrap();
    assert_eq!(bg.aliases, &["background"]);
    assert_eq!(bg.args_hint, "<prompt>");
    assert!(bg.takes_argument);
    assert_eq!(
        bg.summary,
        "Run a prompt on a fresh agent in the background; the answer appears here"
    );
    assert_eq!(bg.action, LocalAction::Bg);
    assert_eq!(table.find("background").unwrap().action, LocalAction::Bg);
    let stop = table.find("stop").unwrap();
    assert!(stop.aliases.is_empty());
    assert!(!stop.takes_argument);
    assert_eq!(
        stop.summary,
        "Stop the running turn and this session's background processes"
    );
    assert_eq!(stop.action, LocalAction::Stop);
    assert!(table.find("tasks").unwrap().action == LocalAction::Tasks);
    let mut work = BackgroundWork::default();
    let effects = update(
        &mut work,
        BackgroundEvent::BgCommand {
            text: "hello".to_string(),
            now_ms: 1,
        },
    );
    assert!(
        effects
            .iter()
            .any(|effect| matches!(effect, BackgroundEffect::StartBackground { .. }))
    );
}
