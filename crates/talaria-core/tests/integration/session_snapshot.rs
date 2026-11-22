//! Snapshot construction and the resume comparison.
//!
//! `resume_snapshot.jsonl` is built from the contract and the wire example in the reducer
//! specification. It is not a recording. Issue 201 has not produced `tool-diff.jsonl`.

use crate::drive;
use crate::events;
use crate::frames;
use crate::snapshot_json;

use serde_json::{Value, json};
use talaria_core::session::{Cell, NoticeKind, PromptKind, SessionState, ToolStatus, TurnState};

use events::{SESSION, ev, ev_noseq};

fn data_file(name: &str) -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/data")
        .join(name)
}

fn user_texts(state: &SessionState) -> Vec<String> {
    state
        .cells()
        .iter()
        .filter_map(|cell| match cell {
            Cell::User(cell) => Some(cell.text.clone()),
            _ => None,
        })
        .collect()
}

fn assistant_texts(state: &SessionState) -> Vec<String> {
    state
        .cells()
        .iter()
        .filter_map(|cell| match cell {
            Cell::Assistant(cell) => Some(cell.text.clone()),
            _ => None,
        })
        .collect()
}

fn comparable(state: &SessionState) -> Vec<(String, String, String)> {
    state
        .cells()
        .iter()
        .filter_map(|cell| match cell {
            Cell::User(cell) => Some(("user".to_string(), cell.text.clone(), String::new())),
            Cell::Assistant(cell) => {
                Some(("assistant".to_string(), cell.text.clone(), String::new()))
            }
            Cell::Tool(cell) => Some(("tool".to_string(), cell.name.clone(), cell.tool_id.clone())),
            _ => None,
        })
        .collect()
}

#[test]
fn resume_fixture_snapshot_equals_replayed_cells() {
    let resume_path = data_file("resume_snapshot.jsonl");
    let resume_text = std::fs::read_to_string(&resume_path).expect("resume_snapshot.jsonl");
    let resume = frames::read_str(&resume_text).expect("resume snapshot log");
    let snapshot = resume.snapshot().expect("session.resume result").clone();
    assert!(
        snapshot.display_messages.is_some(),
        "resume snapshot has no display_messages"
    );
    assert!(
        !snapshot.prompts.is_empty(),
        "resume snapshot has no prompts"
    );

    let fixture = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/frames/tool-diff.jsonl");
    if !fixture.is_file() {
        panic!(
            "missing role tool call: fixtures/frames/tool-diff.jsonl is not recorded; report it to issue 201"
        );
    }
    let recorded = std::fs::read_to_string(&fixture).unwrap_or_else(|_| {
        panic!("missing role tool call: fixtures/frames/tool-diff.jsonl could not be read; report it to issue 201")
    });
    let log = frames::read_str(&recorded).expect("tool call");
    let session = log
        .events
        .iter()
        .find_map(|event| match event {
            talaria_core::session::Event::Session { session_id, .. }
                if !session_id.as_str().is_empty() =>
            {
                Some(session_id.as_str().to_string())
            }
            _ => None,
        })
        .unwrap_or_else(|| SESSION.to_string());
    let (replayed, _) = drive::reduce(&session, log.events);
    let restored = SessionState::from_snapshot(snapshot);
    assert_eq!(comparable(&replayed), comparable(&restored));
}

#[test]
fn snapshot_prefers_display_messages_over_messages() {
    let mut shown = snapshot_json::base(SESSION);
    shown["messages"] = json!([{"role": "user", "text": "hidden"}]);
    shown["display_messages"] = json!([{"role": "user", "text": "shown"}]);
    shown["message_count"] = json!(1);
    let state = SessionState::from_snapshot(snapshot_json::snapshot(shown));
    assert_eq!(user_texts(&state), vec!["shown".to_string()]);

    let mut absent = snapshot_json::base(SESSION);
    absent["messages"] = json!([{"role": "user", "text": "from messages"}]);
    absent["message_count"] = json!(1);
    let state = SessionState::from_snapshot(snapshot_json::snapshot(absent));
    assert_eq!(user_texts(&state), vec!["from messages".to_string()]);

    let mut null_display = snapshot_json::base(SESSION);
    null_display["messages"] = json!([{"role": "user", "text": "from messages"}]);
    null_display["display_messages"] = Value::Null;
    null_display["message_count"] = json!(1);
    let state = SessionState::from_snapshot(snapshot_json::snapshot(null_display));
    assert_eq!(user_texts(&state), vec!["from messages".to_string()]);
}

#[test]
fn snapshot_open_prompts_come_from_prompts_not_pending() {
    let text = std::fs::read_to_string(data_file("snapshot_pending_vs_prompts.jsonl"))
        .expect("snapshot_pending_vs_prompts.jsonl");
    let log = frames::read_str(&text).expect("pending versus prompts");
    assert_eq!(log.queued_admissions_ignored, 1);
    let snapshot = log.snapshot().expect("session.resume result").clone();
    let state = SessionState::from_snapshot(snapshot);
    assert_eq!(state.queued_admissions_ignored(), 1);
    assert_eq!(
        user_texts(&state),
        vec!["Show the pending list.".to_string()]
    );
    assert_eq!(
        assistant_texts(&state),
        vec!["There is one queued admission.".to_string()]
    );
    assert_eq!(state.cells().len(), 3);
    assert_eq!(state.final_prefix_len(), 2);
    assert_eq!(state.open_prompts().count(), 1);
    let prompt = state.open_prompts().next().unwrap();
    assert_eq!(prompt.command.as_deref(), Some("rm -rf .pytest_cache"));
    assert!(state.cells().iter().all(|cell| match cell {
        Cell::User(cell) => cell.text != "queued sentence that is not a question",
        Cell::Assistant(cell) => cell.text != "queued sentence that is not a question",
        Cell::Notice(cell) => cell.text != "queued sentence that is not a question",
        _ => true,
    }));
    assert_eq!(state.turn(), &TurnState::Idle);
}

#[test]
fn snapshot_sets_cursor_generation_running_and_subscription() {
    let mut value = snapshot_json::base(SESSION);
    value["last_sequence"] = json!(12);
    value["replay_epoch"] = json!("epoch-9");
    value["execution_generation"] = json!(4);
    value["subscription_id"] = json!("sub-9");
    value["running"] = json!(true);
    let state = SessionState::from_snapshot(snapshot_json::snapshot(value));
    assert_eq!(state.cursor().replay_epoch.as_deref(), Some("epoch-9"));
    assert_eq!(state.cursor().last_seq, 12);
    assert_eq!(state.execution_generation(), Some(4));
    assert_eq!(state.subscription_id(), Some("sub-9"));
    assert_eq!(state.turn(), &TurnState::Running);
    assert!(!state.needs_resume());
}

#[test]
fn snapshot_tool_rows_become_final_cells_with_unknown_outcome() {
    let mut value = snapshot_json::base(SESSION);
    value["messages"] = json!([
        {
            "role": "tool",
            "name": "terminal",
            "tool_call_id": "call-2",
            "context": "ls -la"
        },
        {
            "role": "tool",
            "name": "write_file",
            "tool_call_id": "call-3",
            "content": "fn main() {}"
        },
        {"role": "system", "text": "restored"},
        {"role": "thought", "text": "skip me"}
    ]);
    value["message_count"] = json!(4);
    let state = SessionState::from_snapshot(snapshot_json::snapshot(value));
    let tools: Vec<_> = state
        .cells()
        .iter()
        .filter_map(|cell| match cell {
            Cell::Tool(cell) => Some(cell),
            _ => None,
        })
        .collect();
    assert_eq!(tools.len(), 2);
    assert_eq!(tools[0].tool_id, "call-2");
    assert_eq!(tools[0].name, "terminal");
    assert_eq!(tools[0].context.as_deref(), Some("ls -la"));
    assert_eq!(tools[0].status, ToolStatus::Unknown);
    assert!(tools[0].diff.is_none());
    assert_eq!(tools[1].name, "write_file");
    assert_eq!(tools[1].result.as_deref(), Some("fn main() {}"));
    assert_eq!(tools[1].status, ToolStatus::Unknown);
    assert!(state.cells().iter().any(|cell| {
        matches!(cell, Cell::Notice(notice) if notice.kind == NoticeKind::System && notice.text == "restored")
    }));
    assert!(state.cells().iter().all(|cell| match cell {
        Cell::Notice(notice) => notice.text != "skip me",
        Cell::User(cell) => cell.text != "skip me",
        Cell::Assistant(cell) => cell.text != "skip me",
        _ => true,
    }));
    assert_eq!(state.final_prefix_len(), state.cells().len());
}

#[test]
fn detach_reply_does_not_decode_as_a_snapshot() {
    let detach = json!({
        "session_id": "local-session-1",
        "subscription_id": "sub-1",
        "detached": true
    });
    assert!(serde_json::from_value::<hermes_contract::schemas::CanonicalSnapshot>(detach).is_err());
    let log = "\
{\"kind\":\"header\",\"version\":3}\n\
{\"kind\":\"frame\",\"seq\":1,\"tMs\":1,\"generation\":1,\"dir\":\"in\",\"frame\":{\"jsonrpc\":\"2.0\",\"id\":1,\"result\":{\"session_id\":\"local-session-1\",\"subscription_id\":\"sub-1\",\"detached\":true}}}\n";
    let parsed = frames::read_str(log).expect("detach log");
    assert!(parsed.snapshot().is_none());
    assert!(parsed.events.is_empty());
}

#[test]
fn events_at_or_below_the_snapshot_cursor_are_ignored() {
    let mut value = snapshot_json::base(SESSION);
    value["last_sequence"] = json!(5);
    value["replay_epoch"] = json!("epoch-1");
    let snapshot = snapshot_json::snapshot(value);
    let events = vec![
        ev(5, "message.delta", json!({"text": "NO"})),
        ev(6, "message.delta", json!({"text": "Yes"})),
        ev(4, "message.delta", json!({"text": "OLD"})),
        ev_noseq("message.delta", json!({"text": " tail"})),
    ];
    let (state, _) = drive::reduce_snapshot(snapshot, events);
    assert_eq!(assistant_texts(&state), vec!["Yes tail".to_string()]);
    assert_eq!(state.cursor().last_seq, 6);
    assert_eq!(state.cursor().replay_epoch.as_deref(), Some("epoch-1"));
}

#[test]
fn open_requests_become_cards_keyed_by_request_id() {
    let approval = json!({
        "prompt_id": "app-1",
        "kind": "approval",
        "execution_generation": 1,
        "command": "ls",
        "choices": [],
    });

    let mut value = snapshot_json::base(SESSION);
    value["prompts"] = json!([approval.clone()]);
    value["open_requests"] = json!([
        {"id": "req-1", "method": "sudo", "params": {}},
    ]);
    let state = SessionState::from_snapshot(snapshot_json::snapshot(value));
    let server_cards: Vec<_> = state
        .open_prompts()
        .filter(|cell| cell.kind == PromptKind::ServerRequest)
        .collect();
    assert_eq!(server_cards.len(), 1);
    assert_eq!(server_cards[0].key.prompt_id, "req-1");
    assert_eq!(server_cards[0].command.as_deref(), Some("sudo"));
    assert!(
        state
            .open_prompts()
            .any(|cell| cell.kind == PromptKind::Approval && cell.key.prompt_id == "app-1")
    );

    let mut dup_and_clarify = snapshot_json::base(SESSION);
    dup_and_clarify["prompts"] = json!([approval.clone()]);
    dup_and_clarify["open_requests"] = json!([
        {"id": "req-1", "method": "sudo", "params": {}},
        {"id": "req-1", "method": "sudo", "params": {}},
        {"id": "req-2", "method": "clarify", "params": {}},
    ]);
    let deduped = SessionState::from_snapshot(snapshot_json::snapshot(dup_and_clarify));
    let deduped_cards: Vec<_> = deduped
        .open_prompts()
        .filter(|cell| cell.kind == PromptKind::ServerRequest)
        .collect();
    assert_eq!(deduped_cards.len(), 1);
    assert_eq!(deduped_cards[0].key.prompt_id, "req-1");
    assert!(
        !deduped
            .open_prompts()
            .any(|cell| cell.key.prompt_id == "req-2")
    );

    let mut no_key = snapshot_json::base(SESSION);
    no_key["prompts"] = json!([approval]);
    let absent = SessionState::from_snapshot(snapshot_json::snapshot(no_key));
    assert!(
        absent
            .open_prompts()
            .any(|cell| cell.kind == PromptKind::Approval && cell.key.prompt_id == "app-1")
    );
    assert!(
        !absent
            .open_prompts()
            .any(|cell| cell.kind == PromptKind::ServerRequest)
    );
}

#[test]
fn server_request_answered_settles_without_a_value() {
    let mut value = snapshot_json::base(SESSION);
    value["open_requests"] = json!([
        {"id": "req-1", "method": "sudo", "params": {}},
    ]);
    let mut state = SessionState::from_snapshot(snapshot_json::snapshot(value));
    let _effects = talaria_core::session::update(
        &mut state,
        talaria_core::session::Event::ServerRequestAnswered {
            id: "req-1".to_owned(),
        },
    );
    assert!(
        !state
            .open_prompts()
            .any(|cell| cell.key.prompt_id == "req-1"),
        "answered card must no longer be open"
    );
    let settled = state.cells().iter().find_map(|cell| match cell {
        Cell::Prompt(prompt) if prompt.key.prompt_id == "req-1" => Some(prompt),
        _ => None,
    });
    let card = settled.expect("recovered card must remain in cells");
    assert_eq!(
        card.state,
        talaria_core::session::PromptState::Settled(talaria_core::session::Settlement::Sent)
    );
}

#[test]
fn pending_connection_becomes_the_same_card() {
    let prompt = json!({
        "prompt_id": "p-1",
        "execution_generation": 1,
        "kind": "approval",
        "choices": ["once"],
        "command": "ls",
    });
    let pending_connection = json!({
        "op_id": "op-1",
        "seq": 1,
        "deadline_at": 1_700_000_090.0,
        "timeout_seconds": 600,
        "targets": [{
            "action": "connect",
            "kind": "connector",
            "name": "github",
            "state": "pending",
        }],
    });

    let mut with_conn = snapshot_json::base(SESSION);
    with_conn["pending_connection"] = pending_connection;
    with_conn["prompts"] = json!([prompt.clone()]);
    let state = SessionState::from_snapshot(snapshot_json::snapshot(with_conn));
    let card = state
        .connection_card()
        .expect("pending_connection opens a connection card");
    assert_eq!(card.op_id, "op-1");
    assert!(
        state
            .open_prompts()
            .any(|cell| cell.kind == PromptKind::Approval && cell.key.prompt_id == "p-1"),
        "approval p-1 must remain open"
    );

    let mut without_conn = snapshot_json::base(SESSION);
    without_conn["pending_connection"] = Value::Null;
    without_conn["prompts"] = json!([prompt]);
    let cleared = SessionState::from_snapshot(snapshot_json::snapshot(without_conn));
    assert!(cleared.connection_card().is_none());
    assert!(
        cleared
            .open_prompts()
            .any(|cell| cell.kind == PromptKind::Approval && cell.key.prompt_id == "p-1"),
        "approval p-1 must remain open when pending_connection is null"
    );
}

/// The kind of each reasoning cell: whether it is marked as the latest step's thinking.
fn reasoning_marks(state: &SessionState) -> Vec<(String, bool)> {
    state
        .cells()
        .iter()
        .filter_map(|cell| match cell {
            Cell::Reasoning(cell) => Some((cell.text.clone(), cell.latest_step)),
            _ => None,
        })
        .collect()
}

fn resumed_mid_turn(messages: Value) -> SessionState {
    let mut value = snapshot_json::base(SESSION);
    value["last_sequence"] = json!(5);
    value["replay_epoch"] = json!("epoch-1");
    value["running"] = json!(true);
    value["message_count"] = json!(messages.as_array().map_or(0, Vec::len));
    value["messages"] = messages;
    let events = vec![ev(
        6,
        "message.complete",
        json!({"status": "complete", "text": "Done", "reasoning": "why it is done"}),
    )];
    drive::reduce_snapshot(snapshot_json::snapshot(value), events).0
}

#[test]
fn end_of_turn_reasoning_after_a_resume_mid_tool_turn_is_the_latest_step() {
    let state = resumed_mid_turn(json!([
        {"role": "user", "text": "list the files"},
        {"role": "tool", "name": "terminal", "tool_call_id": "call-1", "context": "ls"}
    ]));
    assert_eq!(
        reasoning_marks(&state),
        vec![("why it is done".to_owned(), true)]
    );
}

#[test]
fn end_of_turn_reasoning_after_a_resume_is_not_the_latest_step_when_only_an_earlier_turn_ran_tools()
{
    let state = resumed_mid_turn(json!([
        {"role": "user", "text": "list the files"},
        {"role": "tool", "name": "terminal", "tool_call_id": "call-1", "context": "ls"},
        {"role": "assistant", "text": "Listed."},
        {"role": "user", "text": "thanks"}
    ]));
    assert_eq!(
        reasoning_marks(&state),
        vec![("why it is done".to_owned(), false)]
    );
}
