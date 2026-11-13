//! Folding resumed scaffold prompts. Behavior contract: issue 236.

use serde_json::json;
use talaria_core::automation::{FoldKind, fold_user_text};
use talaria_core::session::{Cell, SessionState};

const PLAN: &str = "[/plan — plan mode]\n\nTask to plan:\nwrite a haiku about rain\n\nWrite the plan for an implementer with zero context for the codebase and save it.";
const PLAN_MULTILINE: &str = "[/plan — plan mode]\n\nTask to plan:\nline one\nline two\n\nWrite the plan for an implementer with zero context.";
const PLAN_NO_TASK: &str =
    "[/plan — plan mode]\n\nNo explicit task was given with /plan, so plan from context.";
const GOAL: &str = "[Continuing toward your standing goal]\nGoal: ship it\n\nKeep going.";
const GOAL_CONTRACT: &str = "[Continuing toward your standing goal]\nGoal: ship it\n\nCompletion contract:\n- done when shipped";
const GOAL_SUBGOALS: &str = "[Continuing toward your standing goal]\nGoal: ship it\n\nAdditional criteria the user added mid-loop:\n- fast";
const GOAL_GATE: &str = "[Continuing toward your standing goal — a quality gate failed]\nGoal: ship it\n\nThe gate failed.";
const WAKEUP: &str =
    "[/loop wakeup #3, every 45m]\nRecurring task: check the build\n\nRun the checks.";

#[test]
fn fold_table() {
    let folded = fold_user_text(PLAN).expect("a plan folds");
    assert_eq!(folded.display, "/plan write a haiku about rain");
    assert_eq!(folded.kind, FoldKind::Plan);

    let folded = fold_user_text(PLAN_MULTILINE).expect("a multi-line task folds");
    assert_eq!(folded.display, "/plan line one\nline two");

    let folded = fold_user_text(PLAN_NO_TASK).expect("a plan without a task folds");
    assert_eq!(folded.display, "/plan");

    for variant in [GOAL, GOAL_CONTRACT, GOAL_SUBGOALS] {
        let folded = fold_user_text(variant).expect("a goal continuation folds");
        assert_eq!(folded.display, "↻ goal continues: ship it");
        assert_eq!(folded.kind, FoldKind::GoalContinuation);
    }

    let folded = fold_user_text(GOAL_GATE).expect("a failed gate folds");
    assert_eq!(
        folded.display,
        "↻ goal continues after a failed quality gate: ship it"
    );
    assert_eq!(folded.kind, FoldKind::GoalGateFailed);

    let folded = fold_user_text(WAKEUP).expect("a wakeup folds");
    assert_eq!(folded.display, "↻ loop run #3: check the build");
    assert_eq!(folded.kind, FoldKind::LoopWakeup);

    let folded = fold_user_text(
        "  [/plan — plan mode]\n\nTask to plan:\nx\n\nWrite the plan for an implementer y.",
    )
    .expect("leading whitespace folds");
    assert_eq!(folded.display, "/plan x");

    let folded = fold_user_text("[Continuing toward your standing goal]\nno goal line here")
        .expect("a missing goal line still folds");
    assert_eq!(folded.display, "↻ goal continues");

    let folded = fold_user_text("[/loop wakeup #3, every 45m]\nno task line here")
        .expect("a missing task line still folds");
    assert_eq!(folded.display, "↻ loop run #3");

    for plain in [
        "hello",
        "",
        "[/Plan — plan mode]",
        "[/plan - plan mode]",
        "[Continuing toward your standing Goal]\nGoal: ship it",
    ] {
        assert_eq!(fold_user_text(plain), None, "{plain:?} does not fold");
    }
}

#[test]
fn snapshot_user_rows_are_folded() {
    let snapshot: hermes_contract::schemas::CanonicalSnapshot = serde_json::from_value(json!({
        "authority_epoch": 1,
        "execution_generation": 1,
        "info": {"desktop_protocol": "x"},
        "last_sequence": 0,
        "message_count": 4,
        "messages": [
            {"role": "user", "text": PLAN, "display_kind": "internal_notification"},
            {"role": "user", "text": GOAL, "display_kind": "internal_notification"},
            {"role": "user", "text": WAKEUP, "display_kind": "internal_notification"},
            {"role": "user", "text": "hello"},
        ],
        "pending": [],
        "prompts": [],
        "replay_epoch": "e1",
        "revision": 1,
        "running": false,
        "session_id": "s1",
        "stored_session_id": "s1",
        "subscription_id": "sub1",
    }))
    .expect("the hand-built snapshot deserializes");
    let state = SessionState::from_snapshot(snapshot);
    let users: Vec<&str> = state
        .cells()
        .iter()
        .filter_map(|cell| match cell {
            Cell::User(user) => Some(user.text.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(
        users,
        [
            "/plan write a haiku about rain",
            "↻ goal continues: ship it",
            "↻ loop run #3: check the build",
            "hello",
        ]
    );
}
