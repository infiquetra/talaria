//! Shell and side questions through the pure reducer.

#[path = "support/events.rs"]
mod events;
#[path = "support/snapshot_json.rs"]
mod snapshot_json;

use events::{SESSION, ev, submit};
use serde_json::json;
use talaria_core::session::{
    Cell, Effect, Event, NoticeKind, SessionId, SessionState, ToolStatus, TurnState, update,
};
use talaria_core::side::{Origin, PendingKind, RequestId, ShellOutcome, SideOutcome, carry_over};

fn fresh() -> SessionState {
    SessionState::empty(SessionId::new(SESSION))
}

fn btw(seq: u64, task: &str, question: Option<&str>, text: &str) -> Event {
    ev(
        seq,
        "btw.complete",
        json!({"task_id": task, "question": question, "text": text}),
    )
}

fn shell_cells(state: &SessionState) -> Vec<&talaria_core::side::ShellCell> {
    state
        .cells()
        .iter()
        .filter_map(|cell| match cell {
            Cell::Shell(cell) => Some(cell),
            _ => None,
        })
        .collect()
}

fn side_cells(state: &SessionState) -> Vec<&talaria_core::side::SideCell> {
    state
        .cells()
        .iter()
        .filter_map(|cell| match cell {
            Cell::Side(cell) => Some(cell),
            _ => None,
        })
        .collect()
}

fn notices(state: &SessionState) -> Vec<&talaria_core::session::NoticeCell> {
    state
        .cells()
        .iter()
        .filter_map(|cell| match cell {
            Cell::Notice(cell) => Some(cell),
            _ => None,
        })
        .collect()
}

#[test]
fn shell_request_allocates_id_effect_and_row_not_a_cell() {
    let mut state = fresh();
    let first = update(
        &mut state,
        Event::ShellRequested {
            command: "ls".to_string(),
            started_ms: 0,
        },
    );
    assert_eq!(
        first,
        vec![Effect::RunShell {
            session_id: SessionId::new(SESSION),
            request_id: RequestId(1),
            command: "ls".to_string(),
        }]
    );
    let second = update(
        &mut state,
        Event::ShellRequested {
            command: "pwd".to_string(),
            started_ms: 10,
        },
    );
    assert_eq!(
        second,
        vec![Effect::RunShell {
            session_id: SessionId::new(SESSION),
            request_id: RequestId(2),
            command: "pwd".to_string(),
        }]
    );
    assert_eq!(state.side().pending.len(), 2);
    assert!(state.cells().is_empty());
}

#[test]
fn shell_result_inserts_one_final_cell() {
    let mut state = fresh();
    let _ = update(
        &mut state,
        Event::ShellRequested {
            command: "echo hi".to_string(),
            started_ms: 0,
        },
    );
    let effects = update(
        &mut state,
        Event::ShellFinished {
            request_id: RequestId(1),
            outcome: ShellOutcome::Ok {
                stdout: "hi\n".to_string(),
                stderr: String::new(),
                code: 0,
            },
            duration_ms: 200,
        },
    );
    assert!(effects.is_empty());
    assert!(state.side().pending.is_empty());
    let cells = shell_cells(&state);
    assert_eq!(cells.len(), 1);
    assert_eq!(cells[0].command, "echo hi");
    assert!(state.cells()[0].is_final());
}

#[test]
fn shell_unknown_outcome_is_not_retried() {
    for outcome in [
        ShellOutcome::Unknown,
        ShellOutcome::Unreadable,
        ShellOutcome::NotSent {
            not_connected: true,
        },
    ] {
        let mut state = fresh();
        let _ = update(
            &mut state,
            Event::ShellRequested {
                command: "ls".to_string(),
                started_ms: 0,
            },
        );
        let effects = update(
            &mut state,
            Event::ShellFinished {
                request_id: RequestId(1),
                outcome,
                duration_ms: 5,
            },
        );
        assert!(effects.is_empty(), "no retry");
        assert_eq!(shell_cells(&state).len(), 1);
    }
}

#[test]
fn empty_shell_and_btw_usage_call_nothing() {
    let mut state = fresh();
    assert!(update(&mut state, Event::ShellEmpty).is_empty());
    assert!(update(&mut state, Event::SideUsage).is_empty());
    let notes = notices(&state);
    assert_eq!(notes.len(), 2);
    assert!(notes.iter().all(|note| note.kind == NoticeKind::System));
}

#[test]
fn results_for_ids_no_longer_pending_are_ignored() {
    let mut state = fresh();
    let _ = update(
        &mut state,
        Event::ShellRequested {
            command: "ls".to_string(),
            started_ms: 0,
        },
    );
    let _ = update(
        &mut state,
        Event::ShellFinished {
            request_id: RequestId(1),
            outcome: ShellOutcome::Ok {
                stdout: "x".to_string(),
                stderr: String::new(),
                code: 0,
            },
            duration_ms: 1,
        },
    );
    let before = state.cells().len();
    let effects = update(
        &mut state,
        Event::ShellFinished {
            request_id: RequestId(1),
            outcome: ShellOutcome::Ok {
                stdout: "again".to_string(),
                stderr: String::new(),
                code: 0,
            },
            duration_ms: 1,
        },
    );
    assert!(effects.is_empty());
    assert_eq!(state.cells().len(), before);
    let effects = update(
        &mut state,
        Event::SideAccepted {
            request_id: RequestId(99),
            task_id: "btw_999999".to_string(),
            accepted_ms: 1,
        },
    );
    assert!(effects.is_empty());
    assert_eq!(state.cells().len(), before);
}

#[test]
fn btw_round_trip_by_task_id() {
    let mut state = fresh();
    let asked = update(
        &mut state,
        Event::SideAsked {
            question: "why?".to_string(),
            started_ms: 0,
        },
    );
    assert_eq!(
        asked,
        vec![Effect::AskSide {
            session_id: SessionId::new(SESSION),
            request_id: RequestId(1),
            question: "why?".to_string(),
        }]
    );
    let accepted = update(
        &mut state,
        Event::SideAccepted {
            request_id: RequestId(1),
            task_id: "btw_1a2b3c".to_string(),
            accepted_ms: 0,
        },
    );
    assert!(accepted.is_empty());
    let answered = update(&mut state, btw(1, "btw_1a2b3c", Some("why?"), "because"));
    assert!(answered.is_empty());
    let cells = side_cells(&state);
    assert_eq!(cells.len(), 1);
    assert_eq!(cells[0].origin, Origin::Here);
    assert!(!cells[0].late);
    assert!(state.side().pending.is_empty());
    assert!(state.cells()[0].is_final());
}

#[test]
fn btw_answer_before_reply_matches_pending_question() {
    let mut state = fresh();
    let _ = update(
        &mut state,
        Event::SideAsked {
            question: "why?".to_string(),
            started_ms: 0,
        },
    );
    let _ = update(&mut state, btw(1, "btw_1a2b3c", Some("why?"), "because"));
    let cells = side_cells(&state);
    assert_eq!(cells.len(), 1);
    assert_eq!(cells[0].origin, Origin::Here);
    assert!(state.side().pending.is_empty());
    let before = state.cells().len();
    let effects = update(
        &mut state,
        Event::SideAccepted {
            request_id: RequestId(1),
            task_id: "btw_1a2b3c".to_string(),
            accepted_ms: 5,
        },
    );
    assert!(effects.is_empty());
    assert_eq!(state.cells().len(), before);
}

#[test]
fn btw_answer_from_another_window_and_duplicates() {
    let mut state = fresh();
    let _ = update(&mut state, btw(1, "btw_ffffff", Some("else?"), "an answer"));
    let cells = side_cells(&state);
    assert_eq!(cells.len(), 1);
    assert_eq!(cells[0].origin, Origin::Elsewhere);
    let _ = update(&mut state, btw(2, "btw_ffffff", Some("else?"), "an answer"));
    assert_eq!(side_cells(&state).len(), 1);
}

#[test]
fn btw_error_prefix_and_empty_text() {
    let mut state = fresh();
    let _ = update(&mut state, btw(1, "btw_000001", Some("q"), "error: boom"));
    assert_eq!(
        side_cells(&state)[0].outcome,
        SideOutcome::Failed {
            text: "boom".to_string()
        }
    );
    let mut state = fresh();
    let _ = update(&mut state, btw(1, "btw_000002", Some("q"), ""));
    assert_eq!(
        side_cells(&state)[0].outcome,
        SideOutcome::Answered {
            text: String::new()
        }
    );
}

#[test]
fn btw_lost_reply_matches_later_answer_by_question() {
    let mut state = fresh();
    for _ in 0..2 {
        let _ = update(
            &mut state,
            Event::SideAsked {
                question: "q".to_string(),
                started_ms: 0,
            },
        );
    }
    for id in [1, 2] {
        let _ = update(
            &mut state,
            Event::SideUnknown {
                request_id: RequestId(id),
            },
        );
    }
    assert_eq!(notices(&state).len(), 2);
    assert_eq!(state.side().unconfirmed.len(), 2);
    let _ = update(&mut state, btw(1, "btw_00aa11", Some("q"), "found"));
    assert_eq!(state.side().unconfirmed.len(), 1);
    assert_eq!(state.side().unconfirmed[0], (RequestId(2), "q".to_string()));
    assert_eq!(side_cells(&state).len(), 1);
}

#[test]
fn btw_clock_times_out_then_late_answer() {
    let mut state = fresh();
    let _ = update(
        &mut state,
        Event::SideAsked {
            question: "why?".to_string(),
            started_ms: 0,
        },
    );
    let _ = update(
        &mut state,
        Event::SideAccepted {
            request_id: RequestId(1),
            task_id: "btw_1a2b3c".to_string(),
            accepted_ms: 0,
        },
    );
    let early = update(&mut state, Event::SideClock { now_ms: 299_999 });
    assert!(early.is_empty());
    assert_eq!(state.side().pending.len(), 1);
    let _ = update(&mut state, Event::SideClock { now_ms: 300_000 });
    assert!(state.side().pending.is_empty());
    assert!(state.side().timed_out.contains("btw_1a2b3c"));
    assert_eq!(notices(&state).len(), 1);
    let _ = update(&mut state, btw(1, "btw_1a2b3c", Some("why?"), "late"));
    let cells = side_cells(&state);
    assert_eq!(cells.len(), 1);
    assert!(cells[0].late);
    assert_eq!(cells[0].origin, Origin::Here);
}

#[test]
fn timeout_after_answer_adds_nothing() {
    let mut state = fresh();
    let _ = update(
        &mut state,
        Event::SideAsked {
            question: "why?".to_string(),
            started_ms: 0,
        },
    );
    let _ = update(
        &mut state,
        Event::SideAccepted {
            request_id: RequestId(1),
            task_id: "btw_1a2b3c".to_string(),
            accepted_ms: 0,
        },
    );
    let _ = update(&mut state, btw(1, "btw_1a2b3c", Some("why?"), "early"));
    let before = state.cells().len();
    let effects = update(&mut state, Event::SideClock { now_ms: 300_000 });
    assert!(effects.is_empty());
    assert_eq!(state.cells().len(), before);
}

#[test]
fn btw_refused_and_not_sent_make_cells() {
    let mut state = fresh();
    let _ = update(
        &mut state,
        Event::SideAsked {
            question: "why?".to_string(),
            started_ms: 0,
        },
    );
    let effects = update(
        &mut state,
        Event::SideRefused {
            request_id: RequestId(1),
            reason: "not_found".to_string(),
            detail: None,
        },
    );
    assert!(effects.is_empty());
    assert_eq!(side_cells(&state).len(), 1);
    assert!(state.side().pending.is_empty());
    let mut state = fresh();
    let _ = update(
        &mut state,
        Event::SideAsked {
            question: "why?".to_string(),
            started_ms: 0,
        },
    );
    let effects = update(
        &mut state,
        Event::SideNotSent {
            request_id: RequestId(1),
            not_connected: true,
        },
    );
    assert!(effects.is_empty());
    assert_eq!(side_cells(&state).len(), 1);
}

#[test]
fn btw_complete_leaves_turn_state_alone() {
    let mut state = fresh();
    let _ = update(&mut state, submit("hello", "s1"));
    let _ = update(&mut state, ev(1, "message.start", json!({})));
    let _ = update(
        &mut state,
        ev(2, "message.delta", json!({"text": "streaming"})),
    );
    let _ = update(&mut state, Event::Interrupt);
    assert_eq!(state.turn(), &TurnState::Running);
    let generation = state.execution_generation();
    let users: Vec<String> = state
        .cells()
        .iter()
        .filter_map(|cell| match cell {
            Cell::User(cell) => Some(cell.text.clone()),
            _ => None,
        })
        .collect();
    let live: Vec<String> = state
        .cells()
        .iter()
        .filter_map(|cell| match cell {
            Cell::Assistant(cell) if !cell.sealed => Some(cell.text.clone()),
            _ => None,
        })
        .collect();
    let _ = update(&mut state, btw(3, "btw_1a2b3c", Some("q"), "answer"));
    assert_eq!(state.turn(), &TurnState::Running);
    assert_eq!(state.execution_generation(), generation);
    let users_after: Vec<String> = state
        .cells()
        .iter()
        .filter_map(|cell| match cell {
            Cell::User(cell) => Some(cell.text.clone()),
            _ => None,
        })
        .collect();
    assert_eq!(users_after, users);
    let live_after: Vec<String> = state
        .cells()
        .iter()
        .filter_map(|cell| match cell {
            Cell::Assistant(cell) if !cell.sealed => Some(cell.text.clone()),
            _ => None,
        })
        .collect();
    assert_eq!(live_after, live);
}

#[test]
fn side_cells_land_before_first_unfinished_cell() {
    let mut state = fresh();
    let _ = update(&mut state, submit("one", "s1"));
    let _ = update(
        &mut state,
        Event::SubmitFailed {
            submission_id: "s1".to_string(),
            reason: "test".to_string(),
        },
    );
    let _ = update(&mut state, ev(1, "message.start", json!({})));
    let _ = update(&mut state, ev(2, "message.delta", json!({"text": "live"})));
    assert_eq!(state.final_prefix_len(), 2);
    let _ = update(
        &mut state,
        Event::ShellRequested {
            command: "ls".to_string(),
            started_ms: 0,
        },
    );
    let _ = update(
        &mut state,
        Event::ShellFinished {
            request_id: RequestId(1),
            outcome: ShellOutcome::Ok {
                stdout: "x".to_string(),
                stderr: String::new(),
                code: 0,
            },
            duration_ms: 1,
        },
    );
    assert!(matches!(state.cells()[2], Cell::Shell(_)));
    assert_eq!(state.final_prefix_len(), 3);
}

#[test]
fn interrupt_and_rebuild_keep_side_tasks() {
    let mut state = fresh();
    let _ = update(
        &mut state,
        Event::ShellRequested {
            command: "sleep 1".to_string(),
            started_ms: 0,
        },
    );
    let _ = update(
        &mut state,
        Event::SideAsked {
            question: "why?".to_string(),
            started_ms: 0,
        },
    );
    let effects = update(&mut state, Event::Interrupt);
    assert!(
        effects
            .iter()
            .all(|effect| !matches!(effect, Effect::RunShell { .. } | Effect::AskSide { .. }))
    );
    assert_eq!(state.side().pending.len(), 2);
    let mut rebuilt =
        SessionState::from_snapshot(snapshot_json::snapshot(snapshot_json::base(SESSION)));
    carry_over(&mut state, &mut rebuilt);
    assert_eq!(rebuilt.side().pending.len(), 2);
    assert_eq!(
        rebuilt
            .side()
            .pending
            .iter()
            .filter(|row| matches!(row.kind, PendingKind::Shell { .. }))
            .count(),
        1
    );
    assert_eq!(rebuilt.side().last_request_id, 2);
}

#[test]
fn streaming_reply_and_tool_keep_their_cells_after_a_side_cell_lands() {
    let mut state = fresh();
    let _ = update(&mut state, submit("one", "s1"));
    let _ = update(&mut state, ev(1, "message.start", json!({})));
    let _ = update(
        &mut state,
        ev(
            2,
            "tool.start",
            json!({"tool_id": "t1", "name": "terminal", "context": "ls"}),
        ),
    );
    let _ = update(&mut state, ev(3, "message.delta", json!({"text": "live"})));
    let _ = update(
        &mut state,
        Event::ShellRequested {
            command: "ls".to_string(),
            started_ms: 0,
        },
    );
    let _ = update(
        &mut state,
        Event::ShellFinished {
            request_id: RequestId(1),
            outcome: ShellOutcome::Ok {
                stdout: "x".to_string(),
                stderr: String::new(),
                code: 0,
            },
            duration_ms: 1,
        },
    );
    let _ = update(
        &mut state,
        ev(4, "message.delta", json!({"text": " and more"})),
    );
    let _ = update(
        &mut state,
        ev(
            5,
            "tool.complete",
            json!({"tool_id": "t1", "name": "terminal"}),
        ),
    );
    let live: Vec<&str> = state
        .cells()
        .iter()
        .filter_map(|cell| match cell {
            Cell::Assistant(cell) => Some(cell.text.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(live, ["live and more"]);
    let tools: Vec<ToolStatus> = state
        .cells()
        .iter()
        .filter_map(|cell| match cell {
            Cell::Tool(cell) => Some(cell.status),
            _ => None,
        })
        .collect();
    assert_eq!(tools, [ToolStatus::Succeeded]);
    assert_eq!(shell_cells(&state).len(), 1);
}
