//! Hand-built events for one rule each. Payload fields stay on `raw`.
//!
//! Extra checks from specification section 12.11 live inside the named test. They are not
//! extra tests.

#[path = "support/drive.rs"]
mod drive;
#[path = "support/events.rs"]
mod events;

use hermes_contract::events::{EventFrame, Notification};
use serde_json::{Value, json};
use talaria_core::session::{
    AnswerRefusal, Cell, Effect, Event, NoticeKind, PromptState, SessionState, Settlement,
    ToolStatus, TurnOutcome, TurnState,
};

use events::{SESSION, approval_request, clarify_request, ev, ev_gen, ev_session, key, submit};

const INTERRUPTED: &str = "Interrupted";
const UNKNOWN_STATUS: &str =
    "Hermes ended the turn with a status this version of Talaria does not recognise.";
const CANNOT_ANSWER: &str = "Hermes asked a question this client cannot answer.";

fn unknown_notice(name: &str) -> String {
    format!(
        "Hermes sent an event this version of Talaria does not recognise: {name}. It is ignored."
    )
}

fn run(batch: Vec<Event>) -> (SessionState, Vec<Effect>) {
    drive::reduce(SESSION, batch)
}

fn assistants(state: &SessionState) -> Vec<&talaria_core::session::AssistantCell> {
    state
        .cells()
        .iter()
        .filter_map(|cell| match cell {
            Cell::Assistant(cell) => Some(cell),
            _ => None,
        })
        .collect()
}

fn assistant_text(state: &SessionState) -> Vec<String> {
    assistants(state)
        .into_iter()
        .map(|cell| cell.text.clone())
        .collect()
}

fn reasoning(state: &SessionState) -> Vec<&talaria_core::session::ReasoningCell> {
    state
        .cells()
        .iter()
        .filter_map(|cell| match cell {
            Cell::Reasoning(cell) => Some(cell),
            _ => None,
        })
        .collect()
}

fn tools(state: &SessionState) -> Vec<&talaria_core::session::ToolCell> {
    state
        .cells()
        .iter()
        .filter_map(|cell| match cell {
            Cell::Tool(cell) => Some(cell),
            _ => None,
        })
        .collect()
}

fn prompts(state: &SessionState) -> Vec<&talaria_core::session::PromptCell> {
    state
        .cells()
        .iter()
        .filter_map(|cell| match cell {
            Cell::Prompt(cell) => Some(cell),
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

fn users(state: &SessionState) -> Vec<String> {
    state
        .cells()
        .iter()
        .filter_map(|cell| match cell {
            Cell::User(cell) => Some(cell.text.clone()),
            _ => None,
        })
        .collect()
}

fn start() -> Event {
    ev(1, "message.start", json!({}))
}

#[test]
fn delta_null_seals_the_open_segment_without_a_cell() {
    let (state, _) = run(vec![
        start(),
        ev(2, "message.delta", json!({"text": "Hello"})),
        ev(3, "message.delta", json!({"text": null})),
    ]);
    let cells = assistants(&state);
    assert_eq!(cells.len(), 1);
    assert_eq!(cells[0].text, "Hello");
    assert!(cells[0].sealed);

    let (state, _) = run(vec![
        start(),
        ev(2, "reasoning.delta", json!({"text": "think"})),
        ev(3, "message.delta", json!({"text": "\n"})),
    ]);
    let thought = reasoning(&state);
    assert_eq!(thought.len(), 1);
    assert!(thought[0].sealed);
    assert!(assistants(&state).is_empty());

    let (state, _) = run(vec![
        start(),
        ev(2, "message.delta", json!({"text": "Hi"})),
        ev(
            3,
            "tool.start",
            json!({"tool_id": "call-1", "name": "terminal"}),
        ),
    ]);
    assert!(assistants(&state)[0].sealed);
    assert_eq!(tools(&state).len(), 1);

    let (state, _) = run(vec![
        start(),
        ev(2, "reasoning.delta", json!({"text": "think"})),
        ev(3, "message.delta", json!({"text": "Hi"})),
        ev(
            4,
            "tool.complete",
            json!({"tool_id": "call-missing", "name": "terminal"}),
        ),
    ]);
    assert!(reasoning(&state).iter().all(|cell| cell.sealed));
    assert!(assistants(&state).iter().all(|cell| cell.sealed));
    assert_eq!(tools(&state).len(), 1);
}

#[test]
fn leading_blank_lines_of_a_segment_are_dropped() {
    let (broken, _) = run(vec![
        start(),
        ev(2, "message.delta", json!({"text": "\n"})),
        ev(3, "message.delta", json!({"text": "\n"})),
        ev(4, "message.delta", json!({"text": "Hi"})),
    ]);
    assert_eq!(assistant_text(&broken), vec!["Hi".to_string()]);

    let (spaces, _) = run(vec![start(), ev(2, "message.delta", json!({"text": "  "}))]);
    assert_eq!(assistant_text(&spaces), vec!["  ".to_string()]);

    let (chunks, _) = run(vec![
        start(),
        ev(2, "message.delta", json!({"text": "  "})),
        ev(3, "message.delta", json!({"text": "x"})),
    ]);
    let (whole, _) = run(vec![
        start(),
        ev(2, "message.delta", json!({"text": "  x"})),
    ]);
    assert_eq!(chunks.cells(), whole.cells());
}

#[test]
fn rendered_text_is_never_used() {
    let (state, _) = run(vec![
        start(),
        ev(
            2,
            "message.delta",
            json!({"text": "Hello", "rendered": "\u{1b}[31mHello"}),
        ),
    ]);
    assert_eq!(assistant_text(&state), vec!["Hello".to_string()]);
    assert!(state.cells().iter().all(|cell| match cell {
        Cell::Assistant(cell) => !cell.text.contains("[31m"),
        _ => true,
    }));
}

#[test]
fn complete_text_is_not_appended_after_streaming() {
    let (same, _) = run(vec![
        start(),
        ev(2, "message.delta", json!({"text": "Hello world"})),
        ev(
            3,
            "message.complete",
            json!({"status": "complete", "text": "Hello world"}),
        ),
    ]);
    assert_eq!(assistant_text(&same), vec!["Hello world".to_string()]);

    let (spaced, _) = run(vec![
        start(),
        ev(2, "message.delta", json!({"text": "Hello world"})),
        ev(
            3,
            "message.complete",
            json!({"status": "complete", "text": "Hello  world\n"}),
        ),
    ]);
    assert_eq!(assistant_text(&spaced), vec!["Hello world".to_string()]);

    let (prefix, _) = run(vec![
        start(),
        ev(2, "message.delta", json!({"text": "Checking."})),
        ev(
            3,
            "message.complete",
            json!({"status": "complete", "text": "Checking. Done."}),
        ),
    ]);
    assert_eq!(
        assistant_text(&prefix),
        vec!["Checking.".to_string(), "Done.".to_string()]
    );
    assert!(assistants(&prefix).iter().all(|cell| cell.sealed));

    let (broken, _) = run(vec![
        start(),
        ev(2, "message.delta", json!({"text": "Checking."})),
        ev(
            3,
            "message.complete",
            json!({"status": "complete", "text": "Checking.\n\nDone."}),
        ),
    ]);
    assert_eq!(
        assistant_text(&broken),
        vec!["Checking.".to_string(), "Done.".to_string()]
    );
}

#[test]
fn complete_text_makes_the_reply_when_nothing_streamed() {
    let (alone, _) = run(vec![
        start(),
        ev(
            2,
            "message.complete",
            json!({"status": "complete", "text": "Hello world"}),
        ),
    ]);
    assert_eq!(assistant_text(&alone), vec!["Hello world".to_string()]);

    let (mismatch, _) = run(vec![
        start(),
        ev(2, "message.delta", json!({"text": "abc"})),
        ev(
            3,
            "message.complete",
            json!({"status": "complete", "text": "xyz"}),
        ),
    ]);
    assert_eq!(
        assistant_text(&mismatch),
        vec!["abc".to_string(), "xyz".to_string()]
    );

    let (blank, _) = run(vec![
        start(),
        ev(
            2,
            "message.complete",
            json!({"status": "complete", "text": " \n\t"}),
        ),
    ]);
    assert!(assistant_text(&blank).is_empty());
    assert_eq!(blank.turn(), &TurnState::Finished(TurnOutcome::Completed));
}

#[test]
fn interim_text_is_authoritative_and_seals_the_segment() {
    let (replaced, _) = run(vec![
        start(),
        ev(2, "message.delta", json!({"text": "draft"})),
        ev(
            3,
            "message.interim",
            json!({"text": "shown", "already_streamed": true}),
        ),
    ]);
    let cells = assistants(&replaced);
    assert_eq!(cells.len(), 1);
    assert_eq!(cells[0].text, "shown");
    assert!(cells[0].sealed);
    assert!(cells[0].interim);

    let (added, _) = run(vec![
        start(),
        ev(2, "message.delta", json!({"text": "draft"})),
        ev(
            3,
            "message.interim",
            json!({"text": "note", "already_streamed": false}),
        ),
    ]);
    let cells = assistants(&added);
    assert_eq!(cells.len(), 2);
    assert_eq!(cells[0].text, "draft");
    assert!(cells[0].sealed);
    assert!(!cells[0].interim);
    assert_eq!(cells[1].text, "note");
    assert!(cells[1].interim);
    assert!(cells[1].sealed);
}

#[test]
fn sealed_interim_repeat_is_kept_unless_response_previewed() {
    let (again, _) = run(vec![
        start(),
        ev(
            2,
            "message.interim",
            json!({"text": "Done.", "already_streamed": false}),
        ),
        ev(
            3,
            "message.complete",
            json!({
                "status": "complete",
                "text": "Done.",
                "response_previewed": false
            }),
        ),
    ]);
    assert_eq!(
        assistant_text(&again),
        vec!["Done.".to_string(), "Done.".to_string()]
    );

    let (kept, _) = run(vec![
        start(),
        ev(
            2,
            "message.interim",
            json!({"text": "Done.", "already_streamed": false}),
        ),
        ev(
            3,
            "message.complete",
            json!({
                "status": "complete",
                "text": "Done.",
                "response_previewed": true
            }),
        ),
    ]);
    assert_eq!(assistant_text(&kept), vec!["Done.".to_string()]);
}

#[test]
fn error_status_makes_a_notice_and_never_assistant_text() {
    let (state, _) = run(vec![
        start(),
        ev(2, "message.delta", json!({"text": "partial"})),
        ev(
            3,
            "message.complete",
            json!({
                "status": "error",
                "error": "gateway down",
                "text": "model said",
                "failure_reason": "reason",
                "warning": "heads up",
                "error_surface": {"layer": "gateway", "code": "refused", "retryable": false}
            }),
        ),
    ]);
    assert!(
        assistant_text(&state)
            .iter()
            .all(|text| text != "model said")
    );
    let notes = notices(&state);
    assert!(
        notes
            .iter()
            .any(|note| note.kind == NoticeKind::Error && note.text == "gateway down")
    );
    assert!(notes.iter().any(|note| {
        note.kind == NoticeKind::Error
            && note
                .detail
                .as_ref()
                .is_some_and(|detail| detail["code"] == "refused")
    }));
    assert!(
        notes
            .iter()
            .any(|note| note.kind == NoticeKind::Warning && note.text == "heads up")
    );
    assert_eq!(state.turn(), &TurnState::Finished(TurnOutcome::Failed));
}

#[test]
fn outcome_comes_from_status_not_from_an_error_event() {
    let (mid, _) = run(vec![start(), ev(2, "error", json!({"message": "nope"}))]);
    assert_eq!(mid.turn(), &TurnState::Running);
    assert!(
        notices(&mid)
            .iter()
            .any(|note| note.kind == NoticeKind::Error && note.text == "nope")
    );

    let (done, _) = run(vec![
        start(),
        ev(2, "error", json!({"message": "nope"})),
        ev(
            3,
            "message.complete",
            json!({"status": "complete", "text": "Hello"}),
        ),
    ]);
    assert_eq!(done.turn(), &TurnState::Finished(TurnOutcome::Completed));
    assert_eq!(assistant_text(&done), vec!["Hello".to_string()]);
}

#[test]
fn duplicate_message_complete_is_ignored() {
    let (state, _) = run(vec![
        start(),
        ev(2, "message.delta", json!({"text": "Hello"})),
        ev(
            3,
            "message.complete",
            json!({
                "status": "complete",
                "text": "Hello",
                "usage": {"input": 1, "output": 1}
            }),
        ),
        ev(
            4,
            "message.complete",
            json!({
                "status": "error",
                "text": "second",
                "usage": {"input": 9, "output": 1}
            }),
        ),
    ]);
    assert_eq!(assistant_text(&state), vec!["Hello".to_string()]);
    assert_eq!(state.turn(), &TurnState::Finished(TurnOutcome::Completed));
    assert_eq!(state.usage().input, Some(9));
    assert_eq!(state.usage().output, Some(1));
}

#[test]
fn interrupt_latch_suppresses_late_writes_until_next_start() {
    let events = vec![
        start(),
        ev(2, "message.delta", json!({"text": "Hi"})),
        ev(
            3,
            "tool.start",
            json!({"tool_id": "call-1", "name": "terminal"}),
        ),
        Event::Interrupt,
        ev(4, "message.delta", json!({"text": "NO"})),
        ev(
            5,
            "todo.updated",
            json!({"revision": 4, "todos": [{"text": "keep"}]}),
        ),
        Event::Interrupt,
    ];
    let (state, effects) = run(events);
    assert_eq!(
        effects
            .iter()
            .filter(|effect| matches!(effect, Effect::Interrupt { .. }))
            .count(),
        2
    );
    match &effects[0] {
        Effect::Interrupt {
            session_id,
            execution_generation,
        } => {
            assert_eq!(session_id.as_str(), SESSION);
            assert_eq!(*execution_generation, 1);
        }
        other => panic!("first effect was {other:?}"),
    }
    assert_eq!(assistant_text(&state), vec!["Hi".to_string()]);
    assert!(assistants(&state)[0].sealed);
    assert_eq!(tools(&state)[0].status, ToolStatus::Running);
    assert!(
        notices(&state)
            .iter()
            .all(|note| note.kind != NoticeKind::Interrupted)
    );
    assert_eq!(state.todo().unwrap().revision, 4);
    assert_eq!(state.turn(), &TurnState::Running);
}

#[test]
fn late_complete_after_interrupt_stays_interrupted() {
    let (state, _) = run(vec![
        start(),
        ev(2, "message.delta", json!({"text": "Hi"})),
        ev(
            3,
            "tool.start",
            json!({"tool_id": "call-1", "name": "terminal"}),
        ),
        Event::Interrupt,
        ev(
            4,
            "message.complete",
            json!({
                "status": "complete",
                "text": "secret",
                "usage": {"input": 5, "output": 1}
            }),
        ),
    ]);
    assert_eq!(state.turn(), &TurnState::Finished(TurnOutcome::Interrupted));
    assert_eq!(assistant_text(&state), vec!["Hi".to_string()]);
    assert!(assistants(&state)[0].sealed);
    assert_eq!(tools(&state)[0].status, ToolStatus::Unfinished);
    let interrupted: Vec<_> = notices(&state)
        .into_iter()
        .filter(|note| note.kind == NoticeKind::Interrupted)
        .collect();
    assert_eq!(interrupted.len(), 1);
    assert_eq!(interrupted[0].text, INTERRUPTED);
    assert_eq!(state.usage().input, Some(5));
}

#[test]
fn interrupt_needs_running_turn_and_failure_clears_latch() {
    let (idle, effects) = run(vec![Event::Interrupt]);
    assert!(effects.is_empty());
    assert_eq!(idle.turn(), &TurnState::Idle);

    let (early, effects) = run(vec![
        ev(1, "message.delta", json!({"text": "x"})),
        Event::Interrupt,
        ev(2, "message.delta", json!({"text": "Y"})),
    ]);
    assert!(effects.is_empty());
    assert_eq!(assistant_text(&early), vec!["xY".to_string()]);

    let (cleared, effects) = run(vec![
        start(),
        ev(2, "message.delta", json!({"text": "Hi"})),
        Event::Interrupt,
        ev(3, "message.delta", json!({"text": "NO"})),
        Event::InterruptFailed,
        ev(4, "message.delta", json!({"text": " there"})),
    ]);
    assert_eq!(effects.len(), 1);
    assert_eq!(assistant_text(&cleared), vec!["Hi there".to_string()]);
    assert_eq!(cleared.turn(), &TurnState::Running);
}

#[test]
fn interrupt_leaves_open_prompts_open() {
    let (state, _) = run(vec![
        start(),
        ev(
            2,
            "approval.request",
            approval_request("p1", 1, &["once", "deny"]),
        ),
        Event::Interrupt,
        ev(3, "message.complete", json!({"status": "interrupted"})),
    ]);
    assert_eq!(state.turn(), &TurnState::Finished(TurnOutcome::Interrupted));
    assert_eq!(state.open_prompts().count(), 1);
    let prompt = &prompts(&state)[0];
    assert!(matches!(prompt.state, PromptState::Open));
    assert!(!prompt.sealed);
}

#[test]
fn thinking_delta_is_activity_not_reasoning() {
    let (state, _) = run(vec![
        start(),
        ev(2, "thinking.delta", json!({"text": "spin"})),
    ]);
    assert_eq!(state.activity(), Some("spin"));
    assert!(reasoning(&state).is_empty());
    let (cleared, _) = run(vec![
        start(),
        ev(2, "thinking.delta", json!({"text": "spin"})),
        ev(3, "thinking.delta", json!({"text": ""})),
    ]);
    assert_eq!(cleared.activity(), None);
}

#[test]
fn reasoning_runs_are_separate_cells_complete_reasoning_once() {
    let (one, _) = run(vec![
        start(),
        ev(2, "reasoning.delta", json!({"text": "ab"})),
        ev(3, "reasoning.delta", json!({"text": "c"})),
    ]);
    assert_eq!(reasoning(&one).len(), 1);
    assert_eq!(reasoning(&one)[0].text, "abc");

    let (two, _) = run(vec![
        start(),
        ev(2, "reasoning.delta", json!({"text": "first"})),
        ev(3, "message.delta", json!({"text": "Hi"})),
        ev(4, "reasoning.delta", json!({"text": "second"})),
    ]);
    assert!(assistants(&two)[0].sealed);
    assert_eq!(reasoning(&two).len(), 2);
    assert!(reasoning(&two)[0].sealed);
    assert_eq!(reasoning(&two)[1].text, "second");

    let (from_complete, _) = run(vec![
        start(),
        ev(
            2,
            "message.complete",
            json!({"status": "complete", "text": "Hi", "reasoning": "because"}),
        ),
        ev(
            3,
            "message.complete",
            json!({"status": "complete", "text": "Hi", "reasoning": "again"}),
        ),
    ]);
    assert_eq!(reasoning(&from_complete).len(), 1);
    assert_eq!(reasoning(&from_complete)[0].text, "because");
    assert!(reasoning(&from_complete)[0].sealed);

    let (empty, _) = run(vec![start(), ev(2, "reasoning.delta", json!({"text": ""}))]);
    assert!(reasoning(&empty).is_empty());
}

#[test]
fn tool_complete_without_start_still_makes_a_cell() {
    let (state, _) = run(vec![
        start(),
        ev(2, "message.delta", json!({"text": "Hi"})),
        ev(3, "reasoning.delta", json!({"text": "think"})),
        ev(
            4,
            "tool.complete",
            json!({"tool_id": "call-9", "name": "terminal", "result": "ok"}),
        ),
    ]);
    assert!(assistants(&state)[0].sealed);
    assert!(reasoning(&state)[0].sealed);
    assert_eq!(tools(&state).len(), 1);
    assert_eq!(tools(&state)[0].tool_id, "call-9");
    assert_eq!(tools(&state)[0].status, ToolStatus::Succeeded);
    assert_eq!(tools(&state)[0].result.as_deref(), Some("ok"));
}

#[test]
fn tool_complete_keeps_start_context_and_marks_errors() {
    let (failed, _) = run(vec![
        start(),
        ev(
            2,
            "tool.start",
            json!({
                "tool_id": "call-2",
                "name": "terminal",
                "context": "ls -la"
            }),
        ),
        ev(
            3,
            "tool.complete",
            json!({
                "tool_id": "call-2",
                "name": "terminal",
                "context": "rm -rf .pytest_cache + 1 command",
                "is_error": true
            }),
        ),
    ]);
    assert_eq!(tools(&failed)[0].context.as_deref(), Some("ls -la"));
    assert_eq!(tools(&failed)[0].status, ToolStatus::Failed);

    let (filled, _) = run(vec![
        start(),
        ev(
            2,
            "tool.start",
            json!({"tool_id": "call-3", "name": "terminal"}),
        ),
        ev(
            3,
            "tool.complete",
            json!({
                "tool_id": "call-3",
                "name": "terminal",
                "context": "ls",
                "is_error": false
            }),
        ),
    ]);
    assert_eq!(tools(&filled)[0].context.as_deref(), Some("ls"));
    assert_eq!(tools(&filled)[0].status, ToolStatus::Succeeded);
}

#[test]
fn running_tools_become_unfinished_and_repeats_change_nothing() {
    let (done, _) = run(vec![
        start(),
        ev(
            2,
            "tool.start",
            json!({"tool_id": "call-1", "name": "terminal"}),
        ),
        ev(
            3,
            "tool.complete",
            json!({"tool_id": "call-1", "name": "terminal", "result": "first"}),
        ),
        ev(
            4,
            "tool.complete",
            json!({"tool_id": "call-1", "name": "terminal", "result": "second"}),
        ),
    ]);
    assert_eq!(tools(&done)[0].status, ToolStatus::Succeeded);
    assert_eq!(tools(&done)[0].result.as_deref(), Some("first"));

    let (open, _) = run(vec![
        start(),
        ev(
            2,
            "tool.start",
            json!({"tool_id": "call-1", "name": "terminal"}),
        ),
        ev(
            3,
            "message.complete",
            json!({"status": "complete", "text": "Hi"}),
        ),
        ev(
            4,
            "tool.complete",
            json!({"tool_id": "call-1", "name": "terminal", "result": "late"}),
        ),
    ]);
    assert_eq!(tools(&open)[0].status, ToolStatus::Unfinished);
    assert!(tools(&open)[0].result.is_none());
}

#[test]
fn inline_diff_is_stripped_of_escape_sequences() {
    let (coloured, _) = run(vec![
        start(),
        ev(
            2,
            "tool.complete",
            json!({
                "tool_id": "call-1",
                "name": "terminal",
                "inline_diff": "\u{1b}[31mred\u{1b}[0m"
            }),
        ),
    ]);
    assert_eq!(tools(&coloured)[0].diff.as_deref(), Some("red"));

    let (blank, _) = run(vec![
        start(),
        ev(
            2,
            "tool.complete",
            json!({
                "tool_id": "call-2",
                "name": "terminal",
                "inline_diff": "\u{1b}[31m\u{1b}[0m"
            }),
        ),
    ]);
    assert!(tools(&blank)[0].diff.is_none());

    let (kept, _) = run(vec![
        start(),
        ev(
            2,
            "tool.complete",
            json!({
                "tool_id": "call-3",
                "name": "terminal",
                "inline_diff": "review diff\na/old -> b/new"
            }),
        ),
    ]);
    let diff = tools(&kept)[0].diff.as_deref().unwrap();
    assert!(diff.contains("review diff"));
    assert!(diff.contains("a/old -> b/new"));
}

#[test]
fn todo_update_with_an_older_revision_is_ignored() {
    let (state, _) = run(vec![
        ev(1, "todo.updated", json!({"revision": 2, "todos": ["a"]})),
        ev(2, "todo.updated", json!({"revision": 1, "todos": ["b"]})),
        ev(3, "todo.updated", json!({"revision": 2, "todos": ["c"]})),
    ]);
    assert_eq!(state.todo().unwrap().revision, 2);
    assert_eq!(state.todo().unwrap().todos, vec![json!("c")]);
}

#[test]
fn repeated_status_update_is_recorded_once() {
    let (state, _) = run(vec![
        ev(
            1,
            "status.update",
            json!({"kind": "goal", "text": "working"}),
        ),
        ev(
            2,
            "status.update",
            json!({"kind": "goal", "text": "working"}),
        ),
        ev(3, "status.update", json!({"kind": "goal", "text": "done"})),
        ev(4, "status.update", json!({"kind": "goal", "text": ""})),
    ]);
    assert_eq!(state.status_log().len(), 2);
    assert_eq!(state.status_log()[0].text, "working");
    assert_eq!(state.status_log()[1].text, "done");
    assert!(state.status().is_none());
}

#[test]
fn usage_merges_field_by_field() {
    let (state, _) = run(vec![
        ev(
            1,
            "session.usage",
            json!({"usage": {"input": 1, "model": "m"}}),
        ),
        ev(2, "session.usage", json!({"usage": {"output": 4}})),
        ev(
            3,
            "session.usage",
            json!({"usage": {"input": null, "output": 5}}),
        ),
    ]);
    assert_eq!(state.usage().input, Some(1));
    assert_eq!(state.usage().output, Some(5));
    assert_eq!(state.usage().model.as_deref(), Some("m"));
}

#[test]
fn usage_missing_field_is_not_zero_and_nested_wins() {
    let (zero, _) = run(vec![ev(
        1,
        "session.usage",
        json!({"usage": {"input": 0, "output": null, "model": ""}}),
    )]);
    assert_eq!(zero.usage().input, Some(0));
    assert_eq!(zero.usage().output, None);
    assert_eq!(zero.usage().model, None);

    let (replaced, _) = run(vec![
        ev(1, "session.usage", json!({"usage": {"input": 5}})),
        ev(
            2,
            "session.usage",
            json!({"usage": {"input": 8}, "input_tokens": 1}),
        ),
        ev(3, "session.usage", json!({"usage": {"input": "no"}})),
    ]);
    assert_eq!(replaced.usage().input, Some(8));
}

#[test]
fn duplicate_approval_request_keeps_one_prompt() {
    let (state, _) = run(vec![
        ev(
            1,
            "approval.request",
            approval_request("p1", 1, &["once", "deny"]),
        ),
        ev(
            2,
            "approval.request",
            approval_request("p1", 1, &["once", "deny"]),
        ),
    ]);
    assert_eq!(prompts(&state).len(), 1);
    assert_eq!(state.open_prompts().count(), 1);

    let missing =
        json!({"kind": "approval", "command": "rm -rf .pytest_cache", "choices": ["once"]});
    let (state, _) = run(vec![ev(1, "approval.request", missing)]);
    assert!(prompts(&state).is_empty());
    assert_eq!(state.open_prompts().count(), 0);
    assert!(
        notices(&state)
            .iter()
            .any(|note| note.kind == NoticeKind::Diagnostic && note.text == CANNOT_ANSWER)
    );

    let bad_generation = json!({
        "kind": "approval",
        "prompt_id": "p1",
        "execution_generation": "nope",
        "choices": ["once"]
    });
    let (state, _) = run(vec![ev(1, "approval.request", bad_generation)]);
    assert!(prompts(&state).is_empty());
    assert_eq!(state.open_prompts().count(), 0);
    assert_eq!(
        notices(&state)
            .iter()
            .filter(|note| note.text == CANNOT_ANSWER)
            .count(),
        1
    );
}

#[test]
fn same_prompt_id_in_another_generation_is_another_prompt() {
    let (state, _) = run(vec![
        ev(1, "approval.request", approval_request("p1", 1, &["once"])),
        ev_gen(
            2,
            2,
            "approval.request",
            approval_request("p1", 2, &["once"]),
        ),
    ]);
    assert_eq!(prompts(&state).len(), 2);
    assert_eq!(state.open_prompts().count(), 2);
}

#[test]
fn answer_with_wrong_generation_or_unknown_prompt_is_refused() {
    let (state, effects) = run(vec![
        ev(
            1,
            "approval.request",
            approval_request("p1", 1, &["once", "deny"]),
        ),
        Event::AnswerApproval {
            key: key("p1", 9),
            choice: "once".to_string(),
        },
    ]);
    assert_eq!(
        effects,
        vec![Effect::RefuseAnswer {
            key: key("p1", 9),
            reason: AnswerRefusal::NoSuchPrompt
        }]
    );
    assert!(matches!(prompts(&state)[0].state, PromptState::Open));

    let (state, effects) = run(vec![
        ev(1, "approval.request", approval_request("p1", 1, &["once"])),
        Event::AnswerApproval {
            key: key("missing", 1),
            choice: "once".to_string(),
        },
    ]);
    assert!(matches!(
        effects[0],
        Effect::RefuseAnswer {
            reason: AnswerRefusal::NoSuchPrompt,
            ..
        }
    ));
    assert!(matches!(prompts(&state)[0].state, PromptState::Open));
}

#[test]
fn second_answer_is_refused_and_failure_reopens() {
    let (state, effects) = run(vec![
        ev(
            1,
            "approval.request",
            approval_request("p1", 1, &["once", "session", "deny"]),
        ),
        Event::AnswerApproval {
            key: key("p1", 1),
            choice: "once".to_string(),
        },
        Event::AnswerApproval {
            key: key("p1", 1),
            choice: "session".to_string(),
        },
        Event::AnswerFailed { key: key("p1", 1) },
        Event::AnswerApproval {
            key: key("p1", 1),
            choice: "session".to_string(),
        },
    ]);
    assert!(matches!(effects[0], Effect::RespondApproval { ref choice, .. } if choice == "once"));
    assert!(matches!(
        effects[1],
        Effect::RefuseAnswer {
            reason: AnswerRefusal::AlreadyAnswered,
            ..
        }
    ));
    assert!(
        matches!(effects[2], Effect::RespondApproval { ref choice, .. } if choice == "session")
    );
    assert!(
        matches!(prompts(&state)[0].state, PromptState::Answering(ref value) if value == "session")
    );
}

#[test]
fn settled_closes_only_matching_prompt_cancels_change_nothing() {
    let before = vec![
        ev(1, "approval.request", approval_request("p1", 1, &["once"])),
        ev(
            2,
            "clarify.request",
            clarify_request("p2", 1, "Which path?"),
        ),
    ];
    let (open, _) = run(before.clone());
    let mut events = before;
    events.push(ev(
        3,
        "approval.settled",
        json!({"prompt_id": "p1", "execution_generation": 1}),
    ));
    events.push(ev(
        4,
        "approval.cancelled",
        json!({
            "cancelled_count": 1,
            "reason": "superseded",
            "request_ids": ["p9"],
            "session_id": SESSION,
            "stored_session_id": "stored-1"
        }),
    ));
    events.push(ev(
        5,
        "request.cancel",
        json!({"id": "p2", "method": "clarify", "reason": "gone"}),
    ));
    let (state, _) = run(events);
    assert_eq!(prompts(&open).len(), prompts(&state).len());
    let cells = prompts(&state);
    let first = cells
        .iter()
        .find(|prompt| prompt.key.prompt_id == "p1")
        .unwrap();
    assert!(matches!(
        first.state,
        PromptState::Settled(Settlement::Elsewhere)
    ));
    assert!(first.sealed);
    let second = cells
        .iter()
        .find(|prompt| prompt.key.prompt_id == "p2")
        .unwrap();
    assert!(matches!(second.state, PromptState::Open));
    assert!(!second.sealed);
}

#[test]
fn approval_choice_must_be_one_offered() {
    let (refused, effects) = run(vec![
        ev(
            1,
            "approval.request",
            approval_request("p1", 1, &["once", "deny"]),
        ),
        Event::AnswerApproval {
            key: key("p1", 1),
            choice: "nope".to_string(),
        },
    ]);
    assert_eq!(
        effects,
        vec![Effect::RefuseAnswer {
            key: key("p1", 1),
            reason: AnswerRefusal::ChoiceNotOffered
        }]
    );
    assert!(matches!(prompts(&refused)[0].state, PromptState::Open));

    let (empty, effects) = run(vec![
        ev(1, "approval.request", approval_request("p-empty", 1, &[])),
        Event::AnswerApproval {
            key: key("p-empty", 1),
            choice: "once".to_string(),
        },
    ]);
    assert!(matches!(
        effects[0],
        Effect::RefuseAnswer {
            reason: AnswerRefusal::ChoiceNotOffered,
            ..
        }
    ));
    assert!(matches!(prompts(&empty)[0].state, PromptState::Open));

    let (accepted, effects) = run(vec![
        ev(
            1,
            "approval.request",
            approval_request("p1", 1, &["once", "deny"]),
        ),
        Event::AnswerApproval {
            key: key("p1", 1),
            choice: "once".to_string(),
        },
    ]);
    assert!(matches!(effects[0], Effect::RespondApproval { ref choice, .. } if choice == "once"));
    assert!(
        matches!(prompts(&accepted)[0].state, PromptState::Answering(ref value) if value == "once")
    );
}

#[test]
fn unknown_event_is_noted_once_per_connection() {
    let (state, _) = run(vec![
        ev(1, "x.bogus", json!({})),
        ev(2, "x.bogus", json!({})),
    ]);
    assert_eq!(state.unknown_event_types(), &["x.bogus".to_string()]);
    assert_eq!(notices(&state).len(), 1);
    assert_eq!(notices(&state)[0].text, unknown_notice("x.bogus"));
    assert_eq!(notices(&state)[0].kind, NoticeKind::Diagnostic);

    let (cleared, _) = run(vec![
        ev(1, "x.bogus", json!({})),
        Event::ConnectionOpened { generation: 2 },
        ev(2, "x.bogus", json!({})),
    ]);
    assert_eq!(cleared.unknown_event_types(), &["x.bogus".to_string()]);
    assert_eq!(notices(&cleared).len(), 2);

    let long_name = "n".repeat(65);
    let (long, _) = run(vec![ev(1, &long_name, json!({}))]);
    assert_eq!(notices(&long).len(), 1);
    assert_eq!(notices(&long)[0].text, unknown_notice("an unnamed type"));
    assert!(
        !long
            .unknown_event_types()
            .iter()
            .any(|name| name == &long_name)
    );

    let (spaced, _) = run(vec![ev(1, "has space", json!({}))]);
    assert_eq!(notices(&spaced).len(), 1);
    assert_eq!(notices(&spaced)[0].text, unknown_notice("an unnamed type"));

    let mut batch = Vec::new();
    for index in 1u64..=9 {
        batch.push(ev(index, &format!("x.{index}"), json!({})));
    }
    let (nine, _) = run(batch);
    assert_eq!(nine.unknown_event_types().len(), 8);
    assert_eq!(notices(&nine).len(), 8);
    assert!(!nine.unknown_event_types().iter().any(|name| name == "x.9"));
}

#[test]
fn known_but_unmodelled_events_are_silent() {
    let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../hermes-contract/vendor/gateway-contract.openrpc.json");
    let text = std::fs::read_to_string(&path).expect("vendored gateway contract");
    let document: Value = serde_json::from_str(&text).expect("vendored gateway contract is JSON");
    let names: Vec<String> = document["x-notifications"]
        .as_array()
        .expect("x-notifications")
        .iter()
        .filter_map(|item| item.get("name").and_then(Value::as_str).map(str::to_string))
        .collect();
    assert!(names.len() >= 80, "x-notifications");
    const EXTRAS: &[&str] = &["agent.step", "session.updated", "session.tools_changed"];
    for name in &names {
        let generated = generated_variant(name);
        let allowed = generated || EXTRAS.contains(&name.as_str()) || name.starts_with("subagent.");
        assert!(
            allowed,
            "{name} is neither a generated notification nor on the silent list"
        );
    }

    let (state, effects) = run(vec![
        ev(1, "agent.step", json!({})),
        ev(2, "session.updated", json!({})),
        ev(3, "session.tools_changed", json!({})),
        ev(4, "subagent.start", json!({})),
        ev(5, "notice", json!({"message": "disk"})),
        ev(
            6,
            "approval.cancelled",
            json!({
                "cancelled_count": 0,
                "reason": "none",
                "request_ids": [],
                "session_id": SESSION,
                "stored_session_id": "stored-1"
            }),
        ),
    ]);
    assert!(state.cells().is_empty());
    assert!(state.unknown_event_types().is_empty());
    assert!(effects.is_empty());
}

fn generated_variant(name: &str) -> bool {
    let raw = json!({"type": name, "session_id": SESSION, "payload": {}});
    match EventFrame::from_params(&raw) {
        Ok(frame) => !matches!(frame.notification, Notification::Unknown(_)),
        Err(_) => true,
    }
}

#[test]
fn another_sessions_event_changes_nothing() {
    let base = vec![start(), ev(2, "message.delta", json!({"text": "Hi"}))];
    let (left, _) = run(base.clone());
    let mut with_other = base.clone();
    with_other.push(ev_session(
        "other-session",
        3,
        "message.delta",
        json!({"text": "NO"}),
    ));
    with_other.push(events::session_event(
        "",
        json!({
            "type": "message.delta",
            "session_id": SESSION,
            "replay_epoch": "epoch-1",
            "seq": 4,
            "execution_generation": 1,
            "payload": {"text": "NO"}
        }),
    ));
    let (right, _) = run(with_other);
    assert_eq!(left.cells(), right.cells());
    assert_eq!(left.cursor(), right.cursor());
}

#[test]
fn replay_gap_and_epoch_change_set_needs_resume_only() {
    let base = vec![start(), ev(2, "message.delta", json!({"text": "Hi"}))];
    let (before, _) = run(base.clone());
    let mut gapped = base.clone();
    gapped.push(events::ev_noseq(
        "session.replay_gap",
        json!({"latest_seq": 9, "replay_epoch": "epoch-1"}),
    ));
    let (after_gap, _) = run(gapped);
    assert!(after_gap.needs_resume());
    assert_eq!(before.cells(), after_gap.cells());
    assert_eq!(before.cursor(), after_gap.cursor());

    let mut shifted = base;
    shifted.push(events::session_event(
        SESSION,
        json!({
            "type": "message.delta",
            "session_id": SESSION,
            "replay_epoch": "epoch-2",
            "seq": 1,
            "execution_generation": 1,
            "payload": {"text": "NO"}
        }),
    ));
    let (after_epoch, _) = run(shifted);
    assert!(after_epoch.needs_resume());
    assert_eq!(assistant_text(&after_epoch), vec!["Hi".to_string()]);

    let (adopted, _) = run(vec![ev(3, "message.delta", json!({"text": "Hi"}))]);
    assert_eq!(adopted.cursor().replay_epoch.as_deref(), Some("epoch-1"));
    assert_eq!(adopted.cursor().last_seq, 3);
    assert!(!adopted.needs_resume());

    let (repeated, _) = run(vec![
        ev(1, "message.start", json!({})),
        ev(1, "message.delta", json!({"text": "NO"})),
        ev(2, "message.delta", json!({"text": "Yes"})),
    ]);
    assert_eq!(assistant_text(&repeated), vec!["Yes".to_string()]);
    assert_eq!(repeated.cursor().last_seq, 2);
}

#[test]
fn submit_orders_user_cells_and_failed_submit_is_dropped() {
    let (idle, effects) = run(vec![submit("now", "s-now"), submit("   ", "s-blank")]);
    assert_eq!(users(&idle), vec!["now".to_string()]);
    assert_eq!(effects.len(), 1);
    match &effects[0] {
        Effect::SendPrompt {
            session_id,
            text,
            submission_id,
        } => {
            assert_eq!(session_id.as_str(), SESSION);
            assert_eq!(text, "now");
            assert_eq!(submission_id, "s-now");
        }
        other => panic!("effect was {other:?}"),
    }
    assert_eq!(idle.turn(), &TurnState::Idle);

    let (queued, effects) = run(vec![
        start(),
        submit("one", "s1"),
        submit("two", "s2"),
        Event::SubmitFailed {
            submission_id: "s1".to_string(),
            reason: "refused".to_string(),
        },
        ev(2, "message.complete", json!({"status": "complete"})),
        ev(3, "message.start", json!({})),
    ]);
    assert_eq!(users(&queued), vec!["two".to_string()]);
    assert_eq!(
        effects
            .iter()
            .filter(|effect| matches!(effect, Effect::SendPrompt { .. }))
            .count(),
        2
    );
    assert!(
        notices(&queued)
            .iter()
            .any(|note| note.kind == NoticeKind::Error && note.text == "refused")
    );

    let (placed, _) = run(vec![
        submit("keep", "s-keep"),
        submit("drop", "s-drop"),
        Event::SubmitFailed {
            submission_id: "s-drop".to_string(),
            reason: "no turn".to_string(),
        },
        start(),
    ]);
    assert_eq!(users(&placed), vec!["keep".to_string()]);
}

#[test]
fn automation_message_start_adds_a_header_notice() {
    let (auto, _) = run(vec![ev(
        1,
        "message.start",
        json!({"display_text": "Ran the check"}),
    )]);
    assert!(
        notices(&auto)
            .iter()
            .any(|note| note.kind == NoticeKind::Automation && note.text == "Ran the check")
    );
    assert_eq!(auto.turn(), &TurnState::Running);

    let (diagnostic, _) = run(vec![ev(
        1,
        "message.start",
        json!({
            "display_text": "Ran the check",
            "notification_category": "diagnostic"
        }),
    )]);
    assert!(
        notices(&diagnostic)
            .iter()
            .any(|note| note.kind == NoticeKind::Diagnostic && note.text == "Ran the check")
    );

    let (blank, _) = run(vec![ev(1, "message.start", json!({"display_text": "  "}))]);
    assert!(notices(&blank).is_empty());
}

#[test]
fn transformed_complete_replaces_open_segment_or_adds_cell() {
    let (open, _) = run(vec![
        start(),
        ev(2, "message.delta", json!({"text": "draft"})),
        ev(
            3,
            "message.complete",
            json!({
                "status": "complete",
                "text": "final",
                "response_transformed": true
            }),
        ),
    ]);
    let cells = assistants(&open);
    assert_eq!(cells.len(), 1);
    assert_eq!(cells[0].text, "final");
    assert!(cells[0].sealed);
    assert!(cells[0].transformed);

    let (sealed, _) = run(vec![
        start(),
        ev(2, "message.delta", json!({"text": "draft"})),
        ev(3, "message.delta", json!({"text": null})),
        ev(
            4,
            "message.complete",
            json!({
                "status": "complete",
                "text": "final",
                "response_transformed": true
            }),
        ),
    ]);
    let cells = assistants(&sealed);
    assert_eq!(cells.len(), 2);
    assert_eq!(cells[0].text, "draft");
    assert!(!cells[0].transformed);
    assert_eq!(cells[1].text, "final");
    assert!(cells[1].transformed);
    assert!(cells[1].sealed);
}

#[test]
fn unknown_status_string_ends_failed_with_one_notice() {
    let unknown = json!({
        "type": "message.complete",
        "session_id": SESSION,
        "payload": {"status": "nope"}
    });
    assert!(EventFrame::from_params(&unknown).is_err());
    let missing = json!({
        "type": "message.complete",
        "session_id": SESSION,
        "payload": {}
    });
    assert!(EventFrame::from_params(&missing).is_ok());
    let null_status = json!({
        "type": "message.complete",
        "session_id": SESSION,
        "payload": {"status": null, "text": null}
    });
    assert!(EventFrame::from_params(&null_status).is_ok());

    let string_text = json!({
        "type": "message.complete",
        "session_id": SESSION,
        "payload": {"status": "complete", "text": "Hello"}
    });
    assert!(
        EventFrame::from_params(&string_text).is_ok(),
        "a scalar string message.complete text decodes"
    );

    let (state, _) = run(vec![
        start(),
        ev(2, "message.complete", json!({"status": "nope"})),
    ]);
    assert_eq!(state.turn(), &TurnState::Finished(TurnOutcome::Failed));
    let notes = notices(&state);
    assert_eq!(notes.len(), 1);
    assert_eq!(notes[0].kind, NoticeKind::Error);
    assert_eq!(notes[0].text, UNKNOWN_STATUS);
    assert!(assistant_text(&state).is_empty());

    let (no_status, _) = run(vec![
        start(),
        ev(2, "message.complete", json!({"text": "Hello"})),
    ]);
    assert_eq!(
        no_status.turn(),
        &TurnState::Finished(TurnOutcome::Completed)
    );
    assert_eq!(assistant_text(&no_status), vec!["Hello".to_string()]);

    let (null, _) = run(vec![
        start(),
        ev(
            2,
            "message.complete",
            json!({"status": null, "text": "Hello"}),
        ),
    ]);
    assert_eq!(null.turn(), &TurnState::Finished(TurnOutcome::Completed));
}

#[test]
fn tool_ids_are_looked_up_within_the_current_turn() {
    let (state, _) = run(vec![
        start(),
        ev(
            2,
            "tool.start",
            json!({"tool_id": "call-1", "name": "terminal"}),
        ),
        ev(
            3,
            "tool.complete",
            json!({"tool_id": "call-1", "name": "terminal"}),
        ),
        ev(
            4,
            "message.complete",
            json!({"status": "complete", "text": "done"}),
        ),
        ev(5, "message.start", json!({})),
        ev(
            6,
            "tool.start",
            json!({"tool_id": "call-1", "name": "terminal"}),
        ),
    ]);
    let found = tools(&state);
    assert_eq!(found.len(), 2);
    assert!(found.iter().all(|tool| tool.tool_id == "call-1"));
    assert_eq!(found[0].status, ToolStatus::Succeeded);
    assert_eq!(found[1].status, ToolStatus::Running);
}

#[test]
fn refusal_order_is_prompt_then_kind_then_state_then_choice() {
    let (missing, effects) = run(vec![Event::AnswerApproval {
        key: key("nope", 1),
        choice: "not-offered".to_string(),
    }]);
    assert!(missing.cells().is_empty());
    assert_eq!(
        effects,
        vec![Effect::RefuseAnswer {
            key: key("nope", 1),
            reason: AnswerRefusal::NoSuchPrompt
        }]
    );

    let (kind, effects) = run(vec![
        ev(1, "approval.request", approval_request("p1", 1, &["once"])),
        Event::AnswerClarify {
            key: key("p1", 1),
            answer: "text".to_string(),
        },
    ]);
    assert!(matches!(
        effects[0],
        Effect::RefuseAnswer {
            reason: AnswerRefusal::WrongKind,
            ..
        }
    ));
    assert!(matches!(prompts(&kind)[0].state, PromptState::Open));

    let (state, effects) = run(vec![
        ev(
            1,
            "approval.request",
            approval_request("p1", 1, &["once", "deny"]),
        ),
        Event::AnswerApproval {
            key: key("p1", 1),
            choice: "once".to_string(),
        },
        Event::AnswerApproval {
            key: key("p1", 1),
            choice: "not-offered".to_string(),
        },
    ]);
    assert!(matches!(
        effects[1],
        Effect::RefuseAnswer {
            reason: AnswerRefusal::AlreadyAnswered,
            ..
        }
    ));
    assert!(matches!(
        prompts(&state)[0].state,
        PromptState::Answering(_)
    ));

    let (_open, effects) = run(vec![
        ev(1, "approval.request", approval_request("p1", 1, &["once"])),
        Event::AnswerApproval {
            key: key("p1", 1),
            choice: "not-offered".to_string(),
        },
    ]);
    assert!(matches!(
        effects[0],
        Effect::RefuseAnswer {
            reason: AnswerRefusal::ChoiceNotOffered,
            ..
        }
    ));
}

#[test]
fn refused_interrupt_leaves_one_reply_cell() {
    let (state, _) = run(vec![
        start(),
        ev(2, "message.delta", json!({"text": "Hello"})),
        Event::Interrupt,
        Event::InterruptFailed,
        ev(3, "message.delta", json!({"text": " world"})),
        ev(
            4,
            "message.complete",
            json!({"status": "complete", "text": "Hello world"}),
        ),
    ]);
    assert_eq!(assistant_text(&state), vec!["Hello world".to_string()]);
    assert!(
        notices(&state)
            .iter()
            .all(|note| note.kind != NoticeKind::Interrupted)
    );
    assert_eq!(state.turn(), &TurnState::Finished(TurnOutcome::Completed));
}

#[test]
fn answer_already_resolved_settles_elsewhere() {
    let (resolved, _) = run(vec![
        start(),
        ev(2, "approval.request", approval_request("p1", 1, &["once"])),
        Event::AnswerApproval {
            key: key("p1", 1),
            choice: "once".to_string(),
        },
        ev(
            3,
            "approval.settled",
            json!({"prompt_id": "p1", "execution_generation": 1}),
        ),
        Event::AnswerAlreadyResolved { key: key("p1", 1) },
    ]);
    let prompt = prompts(&resolved)[0];
    assert!(matches!(
        prompt.state,
        PromptState::Settled(Settlement::Elsewhere)
    ));
    assert!(prompt.sealed);

    let (sealed, _) = run(vec![
        start(),
        ev(2, "approval.request", approval_request("p1", 1, &["once"])),
        Event::AnswerApproval {
            key: key("p1", 1),
            choice: "once".to_string(),
        },
        ev(
            3,
            "approval.settled",
            json!({"prompt_id": "p1", "execution_generation": 1}),
        ),
        ev(
            4,
            "message.complete",
            json!({"status": "complete", "text": "done"}),
        ),
        Event::AnswerAlreadyResolved { key: key("p1", 1) },
    ]);
    let prompt = prompts(&sealed)[0];
    assert!(
        matches!(prompt.state, PromptState::Settled(Settlement::Answered(ref value)) if value == "once")
    );
    assert!(prompt.sealed);

    let (again, _) = run(vec![
        start(),
        ev(2, "approval.request", approval_request("p1", 1, &["once"])),
        Event::AnswerApproval {
            key: key("p1", 1),
            choice: "once".to_string(),
        },
        ev(
            3,
            "approval.settled",
            json!({"prompt_id": "p1", "execution_generation": 1}),
        ),
        ev(
            4,
            "approval.settled",
            json!({"prompt_id": "p1", "execution_generation": 1}),
        ),
    ]);
    let prompt = prompts(&again)[0];
    assert!(
        matches!(prompt.state, PromptState::Settled(Settlement::Answered(ref value)) if value == "once")
    );
    assert!(!prompt.sealed);
}

#[test]
fn status_log_keeps_last_fifty_and_drops_latest_repeat() {
    let mut batch = Vec::new();
    for index in 0..51 {
        batch.push(ev(
            (index + 1) as u64,
            "status.update",
            json!({"kind": "goal", "text": format!("n{index}")}),
        ));
    }
    batch.push(ev(
        52,
        "status.update",
        json!({"kind": "goal", "text": "n50"}),
    ));
    let (state, _) = run(batch);
    assert_eq!(state.status_log().len(), 50);
    assert_eq!(state.status_log()[0].text, "n1");
    assert_eq!(state.status_log().last().unwrap().text, "n50");
    assert!(state.status_log().iter().all(|note| note.text != "n0"));
    assert_eq!(state.status().unwrap().text, "n50");
}

#[test]
fn usage_top_level_counter_fills_only_an_empty_field() {
    let (filled, _) = run(vec![ev(
        1,
        "session.usage",
        json!({
            "usage": {"model": "minimax-m3.1-flash", "input": 2},
            "cache_read_tokens": 7,
            "input_tokens": 99
        }),
    )]);
    assert_eq!(filled.usage().cache_read, Some(7));
    assert_eq!(filled.usage().input, Some(2));

    let (kept, _) = run(vec![
        ev(
            1,
            "session.usage",
            json!({
                "usage": {"model": "m"},
                "cache_read_tokens": 0
            }),
        ),
        ev(
            2,
            "session.usage",
            json!({
                "usage": {"input": 2},
                "cache_read_tokens": 1
            }),
        ),
        ev(3, "session.usage", json!({"usage": {"cache_read": 4}})),
    ]);
    assert_eq!(kept.usage().cache_read, Some(4));
}
