//! Drafting while a tool call is still being written. Issue 221.

#[path = "support/bake.rs"]
mod bake;
#[path = "support/events.rs"]
mod events;

use serde_json::json;
use talaria_core::session::{Cell, Event, TurnState};

const TS: &str = "../../docs/handoff/2026-10-22-next-steps/reference-slices/typescript/tests/fixtures/live-two-turns.jsonl";
const RS: &str = "../../docs/handoff/2026-10-22-next-steps/reference-slices/rust/tests/fixtures/live_two_turns.jsonl";

#[test]
fn generating_sets_drafting_until_tool_start() {
    // source: TypeScript bake-off recording, seq 30 then seq 33
    let typescript = bake::recording(TS);
    let mut state = bake::empty();
    let before = state.cells().len();
    bake::apply(&mut state, &bake::by_seq(&typescript, 30));
    assert_eq!(state.cells().len(), before, "generating adds no cell");
    assert_eq!(state.drafting_tool(), Some("terminal"));
    bake::apply(&mut state, &bake::by_seq(&typescript, 33));
    assert_eq!(state.drafting_tool(), None);

    // source: Rust bake-off recording, seq 9 then seq 10
    let rust = bake::recording(RS);
    let mut state = bake::empty();
    bake::apply(&mut state, &bake::by_seq(&rust, 9));
    bake::apply(&mut state, &bake::by_seq(&rust, 10));
    assert_eq!(state.drafting_tool(), Some("terminal"));
    assert!(
        state
            .cells()
            .iter()
            .all(|cell| !matches!(cell, Cell::Tool(_)))
    );
}

#[test]
fn drafting_ignored_while_latched_and_cleared_at_turn_end() {
    // source: TypeScript bake-off recording, the first message.start
    let typescript = bake::recording(TS);
    let started = bake::by_seq(&typescript, 2);
    let clearing = ["message.complete", "message.start", "error"];
    for (offset, type_name) in clearing.into_iter().enumerate() {
        let mut state = bake::empty();
        bake::apply(&mut state, &started);
        assert!(
            matches!(state.turn(), TurnState::Running),
            "the turn is running before drafting"
        );
        let drafting = bake::retarget(&started, 30, "tool.generating", json!({"name": "terminal"}));
        bake::apply(&mut state, &drafting);
        assert_eq!(state.drafting_tool(), Some("terminal"), "{type_name}");
        let end = bake::retarget(&started, 40 + offset as u64, type_name, json!({}));
        bake::apply(&mut state, &end);
        assert_eq!(state.drafting_tool(), None, "{type_name}");
    }

    let mut state = bake::empty();
    bake::apply(&mut state, &started);
    talaria_core::session::update(&mut state, Event::Interrupt);
    let drafting = bake::retarget(&started, 31, "tool.generating", json!({"name": "terminal"}));
    bake::apply(&mut state, &drafting);
    assert_eq!(state.drafting_tool(), None);
}

#[test]
fn progress_frame_reaches_the_view() {
    // source: TypeScript bake-off recording, seq 33. synthetic: tool id changed to call-2
    let typescript = bake::recording(TS);
    let start = bake::by_seq(&typescript, 33);
    let mut state = bake::empty();
    bake::apply(&mut state, &start);
    let progress = bake::retarget(
        &start,
        34,
        "tool.progress",
        json!({
            "tool_id": "call-2",
            "name": "mcp_indexer_build",
            "progress": 42.0,
            "total": 100.0,
            "percent": 42,
            "message": "indexing"
        }),
    );
    bake::apply(&mut state, &progress);
    let tool = state.cells().iter().find_map(|cell| match cell {
        Cell::Tool(tool) if tool.tool_id == "call-2" => Some(tool),
        _ => None,
    });
    let tool = tool.expect("the started call is still there");
    assert!(
        tool.progress.is_some(),
        "the reducer stored the progress frame"
    );
    let view = talaria_core::session::tool_view(tool);
    assert_eq!(view.progress.as_deref(), Some("42% · indexing"));
}
