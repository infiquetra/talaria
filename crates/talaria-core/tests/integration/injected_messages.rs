//! Turns Hermes writes for the agent, such as returned subagent results, a goal continuation or
//! the tool-call limit nudge. They draw as one notice, never as a prompt the person typed, and
//! look the same live and after a resume.

use crate::events;

use events::{EPOCH, SESSION, ev, session_event, submit};
use hermes_contract::schemas::CanonicalSnapshot;
use serde_json::{Value, json};
use talaria_core::session::{Cell, NoticeKind, SessionId, SessionState, update};

const DELEGATION_BODY: &str = "[ASYNC DELEGATION BATCH COMPLETE — deleg_0001]\nTask 1: write the parser\nResult: {\"status\": \"ok\"}\nTask 2: write the tests\nResult: {\"status\": \"ok\"}";
const DELEGATION_LABEL: &str =
    "Subagent Tasks Completed: write the parser, write the tests (2 tasks)";
const GOAL: &str = "[Continuing toward your standing goal]\nGoal: ship it\n\nKeep going.";
const TOOL_LIMIT: &str = "You've reached the maximum number of tool-calling iterations allowed. Please provide a final response summarizing what you've found and accomplished so far, without calling any more tools.";

/// What one cell shows: `> text` for a prompt, `· text` for a notice, `= text` for a reply.
fn shown(state: &SessionState) -> Vec<String> {
    state
        .cells()
        .iter()
        .filter_map(|cell| match cell {
            Cell::User(user) => Some(format!("> {}", user.text)),
            Cell::Notice(notice) => Some(format!("· {}", notice.text)),
            Cell::Assistant(reply) => Some(format!("= {}", reply.text)),
            _ => None,
        })
        .collect()
}

/// The kind and body of every notice.
fn notice_bodies(state: &SessionState) -> Vec<(NoticeKind, Option<String>)> {
    state
        .cells()
        .iter()
        .filter_map(|cell| match cell {
            Cell::Notice(notice) => Some((notice.kind, notice.body.clone())),
            _ => None,
        })
        .collect()
}

fn resumed(messages: Value) -> SessionState {
    let snapshot: CanonicalSnapshot = serde_json::from_value(json!({
        "authority_epoch": 1,
        "execution_generation": 1,
        "info": {"desktop_protocol": "hermes-gateway-v1"},
        "last_sequence": 0,
        "message_count": 1,
        "messages": [],
        "display_messages": messages,
        "pending": [],
        "prompts": [],
        "replay_epoch": EPOCH,
        "revision": 1,
        "running": false,
        "session_id": SESSION,
        "stored_session_id": "stored",
        "subscription_id": "sub"
    }))
    .expect("contract schema CanonicalSnapshot");
    SessionState::from_snapshot(snapshot)
}

fn pending_row(id: &str, text: &str, status: &str) -> Value {
    json!({
        "admission_id": id,
        "authority_epoch": 1,
        "execution_generation": null,
        "input_id": format!("in-{id}"),
        "outcome": null,
        "ref": {"profile_id": "fixture", "session_id": SESSION},
        "sequence": 2,
        "status": status,
        "text": text,
        "controls": ["steer", "replace"],
        "steering": false
    })
}

fn fanout(seq: u64, rows: Vec<Value>) -> talaria_core::session::Event {
    session_event(
        SESSION,
        json!({
            "type": "session.info",
            "session_id": SESSION,
            "replay_epoch": EPOCH,
            "seq": seq,
            "execution_generation": 1,
            "authority_epoch": 1,
            "payload": {
                "pending": rows,
                "running": true,
                "execution_generation": 1,
                "authority_epoch": 1,
                "desktop_protocol": "hermes-gateway-v1",
                "stored_session_id": "stored",
                "revision": 1
            }
        }),
    )
}

fn start_with(seq: u64, admission: &str, payload: Value) -> talaria_core::session::Event {
    session_event(
        SESSION,
        json!({
            "type": "message.start",
            "session_id": SESSION,
            "replay_epoch": EPOCH,
            "seq": seq,
            "execution_generation": seq,
            "authority_epoch": 1,
            "admission_id": admission,
            "payload": payload
        }),
    )
}

fn idle() -> SessionState {
    SessionState::empty(SessionId::new(SESSION))
}

#[test]
fn snapshot_delegation_row_is_one_notice_with_display_text_and_body() {
    let state = resumed(json!([
        {"role": "user", "text": "start the subagents"},
        {"role": "assistant", "text": "Started two subagents."},
        {
            "role": "user",
            "text": DELEGATION_BODY,
            "display_kind": "async_delegation_complete",
            "display_metadata": {"display_text": DELEGATION_LABEL}
        },
    ]));
    assert_eq!(
        shown(&state),
        [
            "> start the subagents".to_string(),
            "= Started two subagents.".to_string(),
            format!("· {DELEGATION_LABEL}"),
        ]
    );
    assert_eq!(
        notice_bodies(&state),
        [(NoticeKind::Automation, Some(DELEGATION_BODY.to_string()))]
    );
}

#[test]
fn snapshot_kind_without_display_text_uses_kind_label() {
    let state = resumed(json!([
        {"role": "user", "text": "[process finished]\nexit 0", "display_kind": "process_complete"},
        {"role": "user", "text": "[delegation]\n{}", "display_kind": "async_delegation_complete",
         "display_metadata": {"display_text": "   "}},
        {"role": "user", "text": "[note]\nkeep going", "display_kind": "internal_notification"},
        {"role": "user", "text": "switched", "display_kind": "model_switch"},
        {"role": "user", "text": "[System note: resumed]", "display_kind": "auto_continue"},
        {"role": "user", "text": "switched", "display_kind": "personality_switch"},
    ]));
    assert_eq!(
        shown(&state),
        [
            "· Background process finished",
            "· Subagent results returned to the agent",
            "· Hermes note to the agent",
            "· Model changed",
            "· Resumed interrupted turn",
            "· Personality changed",
        ]
    );
}

#[test]
fn snapshot_unknown_kind_degrades_to_first_line_notice() {
    let long = "x".repeat(120);
    let state = resumed(json!([
        {"role": "user", "text": "[A NEW KIND]\nsecond line", "display_kind": "brand_new_kind"},
        {"role": "user", "text": long, "display_kind": "another_new_kind"},
    ]));
    assert_eq!(
        shown(&state),
        [
            "· Hermes message to the agent: [A NEW KIND]".to_string(),
            format!("· Hermes message to the agent: {}…", "x".repeat(80)),
        ]
    );
    assert!(
        notice_bodies(&state)
            .iter()
            .all(|(kind, body)| *kind == NoticeKind::Automation && body.is_some())
    );
}

#[test]
fn snapshot_tool_limit_nudge_is_a_notice() {
    let state = resumed(json!([
        {"role": "user", "text": "fix the build"},
        {"role": "user", "text": TOOL_LIMIT},
        {"role": "assistant", "text": "Here is what I found."},
    ]));
    assert_eq!(
        shown(&state),
        [
            "> fix the build",
            "· Tool-call limit reached: Hermes asked the agent for a final summary",
            "= Here is what I found.",
        ]
    );
}

#[test]
fn snapshot_typed_kinds_stay_user_cells() {
    let state = resumed(json!([
        {"role": "user", "text": "/review the diff", "display_kind": "skill_invocation"},
        {"role": "user", "text": "a refused ask", "display_kind": "content_refused"},
        {"role": "user", "text": "plain words"},
    ]));
    assert_eq!(
        shown(&state),
        ["> /review the diff", "> a refused ask", "> plain words"]
    );
}

#[test]
fn snapshot_hidden_row_draws_nothing() {
    let state = resumed(json!([
        {"role": "user", "text": "a wake, not a message", "display_kind": "hidden"},
        {"role": "user", "text": "hello"},
    ]));
    assert_eq!(shown(&state), ["> hello"]);
}

#[test]
fn snapshot_goal_continuation_without_kind_is_a_notice() {
    let state = resumed(json!([{"role": "user", "text": GOAL}]));
    assert_eq!(shown(&state), ["· ↻ goal continues: ship it"]);
}

#[test]
fn automation_message_start_keeps_body_and_draws_no_user_cell() {
    let mut state = idle();
    update(
        &mut state,
        fanout(1, vec![pending_row("adm-auto", DELEGATION_BODY, "queued")]),
    );
    update(
        &mut state,
        start_with(
            2,
            "adm-auto",
            json!({
                "display_kind": "async_delegation_complete",
                "display_text": DELEGATION_LABEL,
                "text": DELEGATION_BODY
            }),
        ),
    );
    update(
        &mut state,
        fanout(3, vec![pending_row("adm-auto", DELEGATION_BODY, "started")]),
    );
    update(
        &mut state,
        ev(
            4,
            "message.delta",
            json!({"text": "Both subagents finished."}),
        ),
    );
    assert_eq!(
        shown(&state),
        [
            format!("· {DELEGATION_LABEL}"),
            "= Both subagents finished.".to_string(),
        ]
    );
    assert_eq!(
        notice_bodies(&state),
        [(NoticeKind::Automation, Some(DELEGATION_BODY.to_string()))]
    );
}

#[test]
fn diagnostic_automation_start_keeps_its_kind() {
    let mut state = idle();
    update(
        &mut state,
        start_with(
            1,
            "adm-auto",
            json!({
                "display_kind": "process_complete",
                "display_text": "Ran the check",
                "text": "[process]\nexit 1",
                "notification_category": "diagnostic"
            }),
        ),
    );
    assert_eq!(shown(&state), ["· Ran the check"]);
    assert_eq!(
        notice_bodies(&state),
        [(
            NoticeKind::Diagnostic,
            Some("[process]\nexit 1".to_string())
        )]
    );
}

#[test]
fn live_and_resumed_diagnostic_look_the_same() {
    // Hermes stores the category in the row's display metadata, so a resumed diagnostic is not
    // drawn as an automation notice; a reconnect compares the two kinds.
    let mut live = idle();
    update(
        &mut live,
        start_with(
            1,
            "adm-auto",
            json!({
                "display_kind": "process_complete",
                "display_text": "Ran the check",
                "text": "[process]\nexit 1",
                "notification_category": "diagnostic"
            }),
        ),
    );
    let resumed = resumed(json!([{
        "role": "user",
        "text": "[process]\nexit 1",
        "display_kind": "process_complete",
        "display_metadata": {
            "display_text": "Ran the check",
            "notification_category": "diagnostic"
        }
    }]));
    assert_eq!(shown(&live), shown(&resumed));
    assert_eq!(notice_bodies(&live), notice_bodies(&resumed));
    assert_eq!(notice_bodies(&resumed)[0].0, NoticeKind::Diagnostic);
}

#[test]
fn a_body_that_repeats_the_label_is_kept() {
    // The body marks the notice as one a resumed snapshot draws again, even when it adds nothing.
    let state = resumed(json!([{
        "role": "user",
        "text": "Ran the check",
        "display_kind": "process_complete",
        "display_metadata": {"display_text": "Ran the check"}
    }]));
    assert_eq!(
        notice_bodies(&state),
        [(NoticeKind::Automation, Some("Ran the check".to_string()))]
    );
}

#[test]
fn automation_start_does_not_take_a_waiting_submit() {
    let mut state = idle();
    update(&mut state, ev(1, "message.start", json!({})));
    update(
        &mut state,
        ev(2, "message.delta", json!({"text": "working"})),
    );
    update(&mut state, submit("my next question", "sub-1"));
    update(
        &mut state,
        ev(3, "message.complete", json!({"status": "complete"})),
    );
    update(
        &mut state,
        start_with(
            4,
            "adm-auto",
            json!({
                "display_kind": "async_delegation_complete",
                "display_text": DELEGATION_LABEL,
                "text": DELEGATION_BODY
            }),
        ),
    );
    update(
        &mut state,
        ev(5, "message.delta", json!({"text": "Results are in."})),
    );
    update(
        &mut state,
        ev(6, "message.complete", json!({"status": "complete"})),
    );
    update(&mut state, start_with(7, "adm-mine", json!({})));
    assert_eq!(
        shown(&state),
        [
            "= working".to_string(),
            format!("· {DELEGATION_LABEL}"),
            "= Results are in.".to_string(),
            "> my next question".to_string(),
        ]
    );
}

#[test]
fn queued_goal_continuation_starts_as_a_notice() {
    let mut state = idle();
    update(
        &mut state,
        fanout(1, vec![pending_row("adm-goal", GOAL, "queued")]),
    );
    update(&mut state, start_with(2, "adm-goal", json!({})));
    update(
        &mut state,
        fanout(3, vec![pending_row("adm-goal", GOAL, "started")]),
    );
    assert_eq!(shown(&state), ["· ↻ goal continues: ship it"]);
    assert_eq!(
        notice_bodies(&state),
        [(NoticeKind::Automation, Some(GOAL.to_string()))]
    );
}

#[test]
fn live_and_resumed_delegation_look_the_same() {
    let mut live = idle();
    update(
        &mut live,
        fanout(1, vec![pending_row("adm-auto", DELEGATION_BODY, "queued")]),
    );
    update(
        &mut live,
        start_with(
            2,
            "adm-auto",
            json!({
                "display_kind": "async_delegation_complete",
                "display_text": DELEGATION_LABEL,
                "text": DELEGATION_BODY
            }),
        ),
    );
    let resumed = resumed(json!([{
        "role": "user",
        "text": DELEGATION_BODY,
        "display_kind": "async_delegation_complete",
        "display_metadata": {"display_text": DELEGATION_LABEL}
    }]));
    assert_eq!(shown(&live), shown(&resumed));
    assert_eq!(notice_bodies(&live), notice_bodies(&resumed));
}
