//! The snapshot's stored list reaches the tracker on attach.

use serde_json::{Value, json};
use talaria_core::checklist::{ChecklistTracker, CommitKind};

#[test]
fn snapshot_todo_state_reaches_the_tracker() {
    let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/data/resume_snapshot.jsonl");
    let text = std::fs::read_to_string(&path).expect("the hand-built resume exchange");
    let mut result: Option<Value> = None;
    for (number, line) in text.lines().enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        let entry: Value = serde_json::from_str(line)
            .unwrap_or_else(|_| panic!("line {} is not JSON", number + 1));
        if entry["kind"] == "frame"
            && entry["dir"] == "in"
            && entry["frame"]["result"]["session_id"].is_string()
        {
            result = Some(entry["frame"]["result"].clone());
        }
    }
    let mut result = result.expect("a session.resume result");
    result["todo_state"] = json!({
        "todos": [{"id": "a", "content": "Write the plan", "status": "in_progress"}],
        "revision": 4,
    });
    let snapshot: hermes_contract::schemas::CanonicalSnapshot =
        serde_json::from_value(result).expect("a contract-shaped snapshot");
    let state = talaria_core::session::SessionState::from_snapshot(snapshot);
    let stored = state.todo().expect("from_snapshot keeps todo_state");
    assert_eq!(stored.revision, 4);
    let mut tracker = ChecklistTracker::new();
    let commit = tracker
        .attach(Some(stored), false)
        .expect("one resumed commit");
    assert_eq!(commit.kind, CommitKind::Resumed);
    assert_eq!(commit.checklist.revision, 4);
    assert_eq!(commit.checklist.items.len(), 1);
    assert_eq!(commit.checklist.items[0].content, "Write the plan");
}
