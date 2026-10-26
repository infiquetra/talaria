//! Detached prompt tasks: elapsed text, answers, and a reconnect.

#[path = "support/events.rs"]
mod events;

use serde_json::json;
use talaria_core::background::{
    BackgroundEffect, BackgroundEvent, BackgroundWork, BgOrigin, BgResult, BgState, CallFailure,
    KillState, ListState, ListedProcess, ProcessId, carry_over, format_elapsed, update,
};
use talaria_core::session::{Cell, SessionId, SessionState, TurnState};
use talaria_core::side::RequestId;

fn effects_of(work: &mut BackgroundWork, event: BackgroundEvent) -> Vec<BackgroundEffect> {
    update(work, event)
}

fn notices(effects: &[BackgroundEffect]) -> Vec<&str> {
    effects
        .iter()
        .filter_map(|effect| match effect {
            BackgroundEffect::InsertNotice { text, .. } => Some(text.as_str()),
            _ => None,
        })
        .collect()
}

fn results(effects: &[BackgroundEffect]) -> Vec<&talaria_core::background::BackgroundCell> {
    effects
        .iter()
        .filter_map(|effect| match effect {
            BackgroundEffect::InsertResult { cell } => Some(cell),
            _ => None,
        })
        .collect()
}

fn start(work: &mut BackgroundWork, text: &str, now_ms: u64) -> RequestId {
    let effects = effects_of(
        work,
        BackgroundEvent::BgCommand {
            text: text.to_string(),
            now_ms,
        },
    );
    effects
        .iter()
        .find_map(|effect| match effect {
            BackgroundEffect::StartBackground { request, .. } => Some(*request),
            _ => None,
        })
        .expect("a task was started")
}

#[test]
fn elapsed_text_uses_whole_seconds() {
    assert_eq!(format_elapsed(0), "0s");
    assert_eq!(format_elapsed(59), "59s");
    assert_eq!(format_elapsed(65), "1m05s");
    assert_eq!(format_elapsed(185), "3m05s");
    assert_eq!(format_elapsed(3_599), "59m59s");
    assert_eq!(format_elapsed(3_600), "1h00m");
    assert_eq!(format_elapsed(3_720), "1h02m");
    assert_eq!(format_elapsed(27 * 3_600), "27h00m");
    let skewed = (10_i64 - 25).max(0) as u64;
    assert_eq!(format_elapsed(skewed), "0s");
}

#[test]
fn a_task_without_an_answer_stays() {
    let mut work = BackgroundWork::default();
    let request = start(&mut work, "count slowly", 1_000);
    let _ = effects_of(
        &mut work,
        BackgroundEvent::BgAccepted {
            request,
            task_id: "bg_000001".to_string(),
            now_ms: 1_100,
        },
    );
    let _ = effects_of(
        &mut work,
        BackgroundEvent::Clock {
            now_ms: 86_400_000,
            wall_s: 90_000.0,
        },
    );
    assert!(matches!(work.bg_tasks()[0].state, BgState::Running { .. }));
    let request = start(&mut work, "second", 86_400_001);
    let _ = effects_of(&mut work, BackgroundEvent::Attached { now_ms: 86_400_002 });
    assert!(matches!(
        work.bg_tasks()
            .iter()
            .find(|task| task.request == request)
            .unwrap()
            .state,
        BgState::ReplyLost
    ));
    let _ = effects_of(
        &mut work,
        BackgroundEvent::Clock {
            now_ms: 200_000_000,
            wall_s: 200_000.0,
        },
    );
    assert!(
        work.bg_tasks()
            .iter()
            .any(|task| matches!(task.state, BgState::ReplyLost))
    );
}

#[test]
fn an_answer_before_the_reply_is_held() {
    let mut work = BackgroundWork::default();
    let request = start(&mut work, "summarise", 1);
    let early = effects_of(
        &mut work,
        BackgroundEvent::BgCompleted {
            task_id: "bg_held".to_string(),
            text: "ready".to_string(),
            now_ms: 2,
        },
    );
    assert!(results(&early).is_empty());
    let accepted = effects_of(
        &mut work,
        BackgroundEvent::BgAccepted {
            request,
            task_id: "bg_held".to_string(),
            now_ms: 3,
        },
    );
    let cell = results(&accepted)[0];
    assert!(matches!(cell.origin, BgOrigin::Here));
    assert_eq!(cell.prompt.as_deref(), Some("summarise"));
    assert!(matches!(cell.result, BgResult::Answer(ref text) if text == "ready"));
}

#[test]
fn another_windows_answer_waits_for_the_reply() {
    let mut work = BackgroundWork::default();
    let request_a = start(&mut work, "task a", 1);
    let unknown = effects_of(
        &mut work,
        BackgroundEvent::BgCompleted {
            task_id: "bg_other".to_string(),
            text: "from elsewhere".to_string(),
            now_ms: 2,
        },
    );
    assert!(results(&unknown).is_empty());
    let accepted = effects_of(
        &mut work,
        BackgroundEvent::BgAccepted {
            request: request_a,
            task_id: "bg_aaaaaa".to_string(),
            now_ms: 3,
        },
    );
    assert_eq!(results(&accepted).len(), 1);
    assert!(matches!(results(&accepted)[0].origin, BgOrigin::Elsewhere));
    let _request_c = start(&mut work, "task c", 4);
    let mut seen = 0;
    for index in 0..17 {
        let effects = effects_of(
            &mut work,
            BackgroundEvent::BgCompleted {
                task_id: format!("bg_extra{index:02}"),
                text: format!("answer {index}"),
                now_ms: 5 + index,
            },
        );
        seen += results(&effects).len();
    }
    assert_eq!(seen, 1);
    let replay = effects_of(
        &mut work,
        BackgroundEvent::BgCompleted {
            task_id: "bg_other".to_string(),
            text: "from elsewhere".to_string(),
            now_ms: 40,
        },
    );
    assert!(results(&replay).is_empty());
}

#[test]
fn starting_and_failing_a_task() {
    let mut work = BackgroundWork::default();
    let blank = effects_of(
        &mut work,
        BackgroundEvent::BgCommand {
            text: "   ".to_string(),
            now_ms: 1,
        },
    );
    assert_eq!(
        notices(&blank),
        [
            "Usage: /bg <prompt> — runs the prompt on a fresh agent in the background; the answer appears here when it finishes."
        ]
    );
    assert!(
        blank
            .iter()
            .all(|effect| !matches!(effect, BackgroundEffect::StartBackground { .. }))
    );
    let started = effects_of(
        &mut work,
        BackgroundEvent::BgCommand {
            text: "  hello world  ".to_string(),
            now_ms: 2,
        },
    );
    let (request, text) = started
        .iter()
        .find_map(|effect| match effect {
            BackgroundEffect::StartBackground { request, text } => Some((*request, text.clone())),
            _ => None,
        })
        .unwrap();
    assert_eq!(text, "hello world");
    assert!(matches!(work.bg_tasks()[0].state, BgState::Starting));
    let accepted = effects_of(
        &mut work,
        BackgroundEvent::BgAccepted {
            request,
            task_id: "bg_0000aa".to_string(),
            now_ms: 3,
        },
    );
    assert_eq!(
        notices(&accepted),
        [
            "Background task bg_0000aa started. Its answer appears here when it finishes; it is not part of the conversation."
        ]
    );
    let early = effects_of(
        &mut work,
        BackgroundEvent::BgCommand {
            text: "other".to_string(),
            now_ms: 4,
        },
    );
    let other = early
        .iter()
        .find_map(|effect| match effect {
            BackgroundEffect::StartBackground { request, .. } => Some(*request),
            _ => None,
        })
        .unwrap();
    let _ = effects_of(
        &mut work,
        BackgroundEvent::BgCompleted {
            task_id: "bg_soon".to_string(),
            text: "soon".to_string(),
            now_ms: 5,
        },
    );
    let applied = effects_of(
        &mut work,
        BackgroundEvent::BgAccepted {
            request: other,
            task_id: "bg_soon".to_string(),
            now_ms: 6,
        },
    );
    assert!(
        results(&applied)
            .iter()
            .any(|cell| cell.task_id == "bg_soon")
    );
    let cases = [
        (
            CallFailure::Refused {
                reason: "not_found".into(),
                detail: None,
            },
            true,
            "Hermes did not start the background task: this session is no longer live. Resume it and try again.",
        ),
        (
            CallFailure::Refused {
                reason: "permission_denied".into(),
                detail: None,
            },
            true,
            "Hermes did not start the background task: this window is not subscribed to the session (permission_denied). Re-open the session and try again.",
        ),
        (
            CallFailure::Refused {
                reason: "runtime_draining".into(),
                detail: None,
            },
            true,
            "Hermes is restarting, so it did not start the background task. Try again when it is back.",
        ),
        (
            CallFailure::Refused {
                reason: "invalid_params".into(),
                detail: None,
            },
            true,
            "Hermes rejected the background task request as malformed (invalid_params). This is a Talaria bug; please report it.",
        ),
        (
            CallFailure::Refused {
                reason: String::new(),
                detail: None,
            },
            true,
            "Hermes did not start the background task (no reason given).",
        ),
        (
            CallFailure::Refused {
                reason: "busy".into(),
                detail: None,
            },
            true,
            "Hermes did not start the background task (busy).",
        ),
        (
            CallFailure::MethodMissing,
            true,
            "This Hermes does not offer background tasks (prompt.background is missing).",
        ),
        (
            CallFailure::Storage,
            true,
            "Hermes could not reach its session store (storage_unavailable). Try again.",
        ),
        (
            CallFailure::NotSent,
            true,
            "Not connected to Hermes, so the background task was not started.",
        ),
        (
            CallFailure::Unknown,
            false,
            "Hermes did not confirm the background task. It may still run; its answer will appear here if it does.",
        ),
        (
            CallFailure::Unreadable,
            false,
            "Hermes did not confirm the background task. It may still run; its answer will appear here if it does.",
        ),
    ];
    for (failure, gone, text) in cases {
        let request = start(&mut work, "again", 10);
        let before = work.bg_tasks().len();
        let failed = effects_of(
            &mut work,
            BackgroundEvent::BgFailed {
                request,
                failure,
                now_ms: 11,
            },
        );
        assert_eq!(notices(&failed), [text]);
        if gone {
            assert!(work.bg_tasks().len() < before);
        } else {
            assert!(
                work.bg_tasks()
                    .iter()
                    .any(|task| matches!(task.state, BgState::ReplyLost))
            );
        }
    }
    let ignored = effects_of(
        &mut work,
        BackgroundEvent::BgFailed {
            request: RequestId(u64::MAX),
            failure: CallFailure::NotSent,
            now_ms: 12,
        },
    );
    assert!(ignored.is_empty());
}

#[test]
fn an_answer_and_a_failure_become_cells() {
    let mut work = BackgroundWork::default();
    let request = start(&mut work, "prompt", 1);
    let _ = effects_of(
        &mut work,
        BackgroundEvent::BgAccepted {
            request,
            task_id: "bg_ok".to_string(),
            now_ms: 2,
        },
    );
    for (text, failed) in [
        ("plain", false),
        ("error: boom", true),
        ("", false),
        ("  ", false),
    ] {
        let effects = effects_of(
            &mut work,
            BackgroundEvent::BgCompleted {
                task_id: format!("bg-{text}"),
                text: text.to_string(),
                now_ms: 3,
            },
        );
        if text == "plain" {
            let cell = &results(&effects)[0];
            assert!(matches!(cell.result, BgResult::Answer(ref body) if body == "plain"));
        }
        if text == "error: boom" {
            let cell = &results(&effects)[0];
            assert!(matches!(cell.result, BgResult::Failed(ref body) if body == "boom"));
        }
        assert!(effects.iter().any(|effect| matches!(
            effect,
            BackgroundEffect::Signal(talaria_core::background::BackgroundSignal::BgFinished { failed: flag, .. }) if *flag == failed
        )));
    }
    let replay = effects_of(
        &mut work,
        BackgroundEvent::BgCompleted {
            task_id: "bg-plain".to_string(),
            text: "plain".to_string(),
            now_ms: 4,
        },
    );
    assert!(results(&replay).is_empty());
}

#[test]
fn a_lost_reply_takes_the_oldest_task() {
    let mut work = BackgroundWork::default();
    let first = start(&mut work, "oldest prompt", 1);
    let _second = start(&mut work, "newer prompt", 2);
    let _ = effects_of(&mut work, BackgroundEvent::Attached { now_ms: 3 });
    assert!(
        work.bg_tasks()
            .iter()
            .all(|task| matches!(task.state, BgState::ReplyLost))
    );
    let effects = effects_of(
        &mut work,
        BackgroundEvent::BgCompleted {
            task_id: "bg_taken".to_string(),
            text: "landed".to_string(),
            now_ms: 4,
        },
    );
    let cell = results(&effects)[0];
    assert!(matches!(cell.origin, BgOrigin::Here));
    assert_eq!(cell.prompt.as_deref(), Some("oldest prompt"));
    assert!(work.bg_tasks().iter().all(|task| task.request != first));
    let again = effects_of(
        &mut work,
        BackgroundEvent::BgCompleted {
            task_id: "bg_taken".to_string(),
            text: "landed".to_string(),
            now_ms: 5,
        },
    );
    assert!(results(&again).is_empty());
}

#[test]
fn a_shown_answer_is_ignored_and_the_turn_stays() {
    let mut state = SessionState::empty(SessionId::new(events::SESSION));
    let started = talaria_core::session::update(
        &mut state,
        events::session_event(
            events::SESSION,
            json!({
                "type": "message.start",
                "session_id": events::SESSION,
                "replay_epoch": events::EPOCH,
                "seq": 1,
                "execution_generation": 7,
                "authority_epoch": 1,
                "payload": {},
            }),
        ),
    );
    assert!(
        started.is_empty()
            || started
                .iter()
                .all(|effect| !matches!(effect, talaria_core::session::Effect::Interrupt { .. }))
    );
    assert!(matches!(state.turn(), TurnState::Running));
    let generation = state.execution_generation();
    let queue = state.queue.clone();
    let _ = talaria_core::session::update(
        &mut state,
        talaria_core::session::Event::Background(BackgroundEvent::BgCompleted {
            task_id: "bg_side".to_string(),
            text: "answer".to_string(),
            now_ms: 10,
        }),
    );
    assert!(matches!(state.turn(), TurnState::Running));
    assert_eq!(state.execution_generation(), generation);
    assert_eq!(state.queue, queue);
    assert!(
        state
            .cells()
            .iter()
            .any(|cell| matches!(cell, Cell::Background(_)))
    );
}

#[test]
fn carry_over_keeps_work_and_attach_resets_inflight() {
    let mut work = BackgroundWork::default();
    let _ = effects_of(&mut work, BackgroundEvent::Attached { now_ms: 1 });
    let attached = effects_of(
        &mut work,
        BackgroundEvent::ForegroundChanged {
            foreground: true,
            now_ms: 1,
        },
    );
    let request = attached
        .iter()
        .find_map(|effect| match effect {
            BackgroundEffect::ListProcesses { request } => Some(*request),
            _ => None,
        })
        .expect("the foreground lists");
    let _ = effects_of(
        &mut work,
        BackgroundEvent::Listed {
            request,
            processes: vec![ListedProcess {
                id: ProcessId("tick".to_string()),
                command: "sh tick.sh".to_string(),
                status_raw: "running".to_string(),
                exit_code: None,
                started_epoch_s: Some(1.0),
                exited_at_s: None,
                completion_reason: None,
                output_tail: None,
            }],
            now_ms: 2,
        },
    );
    let _ = start(&mut work, "kept prompt", 3);
    let kill = effects_of(
        &mut work,
        BackgroundEvent::KillRequested {
            id: ProcessId("tick".to_string()),
            now_ms: 4,
        },
    );
    assert!(
        kill.iter()
            .any(|effect| matches!(effect, BackgroundEffect::KillProcess { .. }))
    );
    let _ = effects_of(
        &mut work,
        BackgroundEvent::TerminalOutput {
            process_id: "tick".to_string(),
            chunk: "tick 1\n".to_string(),
            now_ms: 5,
        },
    );
    let mut rebuilt = BackgroundWork::default();
    carry_over(&work, &mut rebuilt);
    assert_eq!(rebuilt.bg_tasks().len(), work.bg_tasks().len());
    assert_eq!(rebuilt.rows().count(), work.rows().count());
    assert!(rebuilt.output(&ProcessId("tick".to_string())).is_some());
    let reset = effects_of(&mut work, BackgroundEvent::Attached { now_ms: 9 });
    assert!(
        work.bg_tasks()
            .iter()
            .any(|task| matches!(task.state, BgState::ReplyLost))
    );
    assert!(!matches!(work.list_state(), ListState::Stopped { .. }));
    let note = work
        .rows()
        .find(|row| row.id.0 == "tick")
        .map(|row| row.kill.clone());
    assert_eq!(
        note,
        Some(KillState::Note {
            text: "Hermes did not confirm. Refreshing the list.".to_string(),
        })
    );
    assert!(
        reset
            .iter()
            .any(|effect| matches!(effect, BackgroundEffect::ListProcesses { .. }))
    );
    let again = effects_of(&mut work, BackgroundEvent::RefreshRequested { now_ms: 10 });
    assert!(
        again
            .iter()
            .any(|effect| matches!(effect, BackgroundEffect::ListProcesses { .. }))
    );
}
