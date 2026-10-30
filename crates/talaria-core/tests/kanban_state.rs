//! Move offers follow the workflow table.

use std::collections::BTreeMap;

use talaria_core::kanban::{KanbanBoardState, MoveTable};

#[test]
fn moves_offer_only_workflow_allowed() {
    let mut state = KanbanBoardState::blank();
    state.load_workflow(Some(workflow()));
    let mut offered = state.offered("ready");
    offered.sort();
    assert_eq!(
        offered,
        vec![
            "archived".to_string(),
            "blocked".to_string(),
            "done".to_string(),
            "review".to_string(),
            "scheduled".to_string(),
            "todo".to_string(),
            "triage".to_string(),
        ],
        "state missing"
    );
    assert!(
        !state
            .offered("ready")
            .iter()
            .any(|status| status == "running"),
        "state missing"
    );
    assert!(state.writes_allowed(), "state missing");

    let mut bare = KanbanBoardState::blank();
    bare.load_workflow(None);
    assert!(!bare.writes_allowed(), "state missing");
    assert!(bare.offered("ready").is_empty(), "state missing");
    assert!(bare.offered("running").is_empty(), "state missing");
}

fn workflow() -> MoveTable {
    let mut manual = BTreeMap::new();
    manual.insert(
        "ready".into(),
        vec![
            "blocked".into(),
            "done".into(),
            "review".into(),
            "scheduled".into(),
            "todo".into(),
            "triage".into(),
        ],
    );
    manual.insert("running".into(), vec!["done".into()]);
    MoveTable {
        columns: vec![
            "triage".into(),
            "todo".into(),
            "scheduled".into(),
            "ready".into(),
            "running".into(),
            "blocked".into(),
            "review".into(),
            "done".into(),
        ],
        manual,
    }
}
