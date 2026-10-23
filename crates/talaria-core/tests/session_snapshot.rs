//! Snapshot construction and the resume comparison.
//!
//! `resume_snapshot.jsonl` is built from the contract and the wire example in the reducer
//! specification. It is not a recording. Issue 201 has not produced `tool-diff.jsonl`.

#[path = "support/drive.rs"]
mod drive;
#[path = "support/events.rs"]
mod events;
#[path = "support/frames.rs"]
mod frames;
#[path = "support/snapshot_json.rs"]
mod snapshot_json;

use serde_json::{Value, json};
use talaria_core::session::{Cell, NoticeKind, SessionState, ToolStatus, TurnState};

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
