//! A prompt Hivecode handles as a workflow reaches the model wrapped: the first one carries the
//! whole workflow skill, later ones a one-line reminder. The screen shows only what the person
//! typed, live, queued, in the queue preview and after a resume.

use crate::events;

use events::{EPOCH, SESSION, ev, session_event};
use hermes_contract::schemas::CanonicalSnapshot;
use serde_json::{Value, json};
use talaria_core::hivecode::{
    FOLLOW_UP_REMINDER, follow_up_text, notice, replayed_request, retyped, typed_text,
};
use talaria_core::queue::{
    AdmissionId, Controls, IdSource, QueueIntent, QueuedRow, RowStatus, SubmissionId, update_queue,
};
use talaria_core::rewind::rewind_turns;
use talaria_core::session::{Cell, Effect, Event, SessionId, SessionState, update};

/// What Hermes expands a routed prompt into: the skill's header and body, then the request.
const FULL: &str = "[IMPORTANT: The user has invoked the \"talaria-workflows\" skill, indicating they want you to follow its instructions. The full skill content is loaded below.]\n\n# Fixture workflow guide\nPropose a workflow, then ask before starting it.\n\n[Skill directory: fixture-skills/talaria-workflows]\n\nThe user has provided the following instruction alongside the skill invocation: fix the parser";

/// What Hermes writes in place of a routed prompt when it replays the session.
const PROJECTED: &str = "/talaria-workflows fix the parser";

/// Text that must never reach the screen.
const HIDDEN: [&str; 4] = [
    "/talaria-workflows",
    "[IMPORTANT",
    "Fixture workflow guide",
    FOLLOW_UP_REMINDER,
];

struct Ids(u32);

impl IdSource for Ids {
    fn submission_id(&mut self) -> SubmissionId {
        self.0 += 1;
        SubmissionId(format!("sub-{}", self.0))
    }
}

/// Every line the transcript shows: `> text` for a prompt, `· text` for a notice.
fn shown(state: &SessionState) -> Vec<String> {
    state
        .cells()
        .iter()
        .filter_map(|cell| match cell {
            Cell::User(user) => Some(format!("> {}", user.text)),
            Cell::Notice(notice) => Some(format!("· {}", notice.text)),
            _ => None,
        })
        .collect()
}

fn users(state: &SessionState) -> Vec<String> {
    state
        .cells()
        .iter()
        .filter_map(|cell| match cell {
            Cell::User(user) => Some(user.text.clone()),
            _ => None,
        })
        .collect()
}

fn assert_hidden(lines: &[String]) {
    for line in lines {
        for hidden in HIDDEN {
            assert!(
                !line.contains(hidden),
                "{hidden:?} reached the screen: {line:?}"
            );
        }
    }
}

fn running() -> SessionState {
    let mut state = SessionState::empty(SessionId::new(SESSION));
    update(&mut state, ev(1, "message.start", json!({})));
    update(
        &mut state,
        ev(2, "message.delta", json!({"text": "reply so far"})),
    );
    state
}

fn routed(text: &str, display: &str, id: &str) -> Event {
    Event::SubmitCommand {
        text: text.to_owned(),
        display: display.to_owned(),
        submission_id: id.to_owned(),
    }
}

fn queued(id: &str, text: &str, seq: i64) -> QueuedRow {
    QueuedRow {
        admission_id: AdmissionId(id.to_owned()),
        input_id: SubmissionId(format!("in-{id}")),
        text: text.to_owned(),
        status: RowStatus::Queued,
        outcome: None,
        controls: Controls {
            steer: true,
            replace: true,
        },
        steering: false,
        sequence: seq,
        execution_generation: Some(1),
    }
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

#[test]
fn reminder_is_one_model_facing_line_in_front_of_the_typed_text() {
    assert_eq!(FOLLOW_UP_REMINDER.lines().count(), 1);
    assert!(FOLLOW_UP_REMINDER.contains("talaria-workflows"));
    let sent = follow_up_text("go ahead");
    assert!(sent.starts_with(FOLLOW_UP_REMINDER), "{sent}");
    assert!(sent.ends_with("\n\ngo ahead"), "{sent}");
    assert_eq!(typed_text(&sent), Some("go ahead"));
    assert_eq!(typed_text(FULL), Some("fix the parser"));
    let runtime = format!("{FULL}\n\n[Runtime note: fixture]");
    assert_eq!(typed_text(&runtime), Some("fix the parser"));
    for other in [
        "go ahead",
        PROJECTED,
        "[IMPORTANT: The user has invoked the \"review\" skill, indicating they want you to follow its instructions. The full skill content is loaded below.]\n\nThe user has provided the following instruction alongside the skill invocation: x",
    ] {
        assert_eq!(typed_text(other), None, "{other:?}");
    }
}

#[test]
fn routed_prompt_placed_at_once_shows_what_was_typed() {
    let mut state = SessionState::empty(SessionId::new(SESSION));
    let effects = update(&mut state, routed(FULL, "fix the parser", "sub-1"));
    assert!(
        effects
            .iter()
            .any(|effect| matches!(effect, Effect::SendPrompt { text, .. } if text == FULL)),
        "the model still gets the whole skill: {effects:?}"
    );
    assert_eq!(users(&state), ["fix the parser"]);
}

#[test]
fn queued_routed_prompts_show_what_was_typed_when_they_start() {
    let mut state = running();
    let follow_up = follow_up_text("go ahead");
    update(&mut state, routed(FULL, "fix the parser", "sub-1"));
    update(&mut state, routed(&follow_up, "go ahead", "sub-2"));
    assert!(users(&state).is_empty(), "nothing is drawn while queued");
    update(
        &mut state,
        ev(3, "message.complete", json!({"text": "done"})),
    );
    update(&mut state, ev(4, "message.start", json!({})));
    update(
        &mut state,
        ev(5, "message.complete", json!({"text": "done"})),
    );
    update(&mut state, ev(6, "message.start", json!({})));
    assert_eq!(users(&state), ["fix the parser", "go ahead"]);
    assert_hidden(&shown(&state));
}

#[test]
fn queue_preview_and_started_rows_show_what_was_typed() {
    let mut state = running();
    let follow_up = follow_up_text("go ahead");
    update(
        &mut state,
        session_event(
            SESSION,
            json!({
                "type": "session.info",
                "session_id": SESSION,
                "replay_epoch": EPOCH,
                "seq": 3,
                "execution_generation": 1,
                "authority_epoch": 1,
                "payload": {
                    "pending": [
                        {"admission_id": "adm-a", "authority_epoch": 1, "execution_generation": null,
                         "input_id": "in-a", "outcome": null,
                         "ref": {"profile_id": "fixture", "session_id": SESSION},
                         "sequence": 1, "status": "queued", "text": FULL,
                         "controls": ["steer", "replace"], "steering": false},
                        {"admission_id": "adm-b", "authority_epoch": 1, "execution_generation": null,
                         "input_id": "in-b", "outcome": null,
                         "ref": {"profile_id": "fixture", "session_id": SESSION},
                         "sequence": 2, "status": "queued", "text": follow_up,
                         "controls": ["steer", "replace"], "steering": false}
                    ],
                    "running": true,
                    "execution_generation": 1,
                    "authority_epoch": 1,
                    "desktop_protocol": "hermes-gateway-v1",
                    "stored_session_id": "stored",
                    "revision": 1
                }
            }),
        ),
    );
    let view = state.queue.view(true);
    let texts: Vec<&str> = view.rows.iter().map(|row| row.text.as_str()).collect();
    assert_eq!(texts, ["fix the parser", "go ahead"]);
    update(
        &mut state,
        ev(4, "message.complete", json!({"text": "done"})),
    );
    update(
        &mut state,
        session_event(
            SESSION,
            json!({
                "type": "message.start",
                "session_id": SESSION,
                "replay_epoch": EPOCH,
                "seq": 5,
                "execution_generation": 2,
                "authority_epoch": 1,
                "admission_id": "adm-a",
                "payload": {"text": FULL}
            }),
        ),
    );
    assert_eq!(users(&state), ["fix the parser"]);
    assert_hidden(&shown(&state));
}

#[test]
fn editing_a_queued_routed_prompt_shows_the_typed_text_and_keeps_it_routed() {
    let follow_up = follow_up_text("go ahead");
    for (sent, typed, edited, expected) in [
        (
            follow_up.as_str(),
            "go ahead",
            "go ahead, smaller steps",
            follow_up_text("go ahead, smaller steps"),
        ),
        (
            FULL,
            "fix the parser",
            "fix the lexer",
            FULL.replace("fix the parser", "fix the lexer"),
        ),
    ] {
        let mut state = running();
        state.queue.rows = vec![queued("adm-a", sent, 1)];
        state.queue.selection = Some(AdmissionId("adm-a".into()));
        let mut ids = Ids(0);
        update_queue(&mut state, QueueIntent::BeginEdit, &mut ids);
        let edit = state.queue.edit.clone().expect("edit opens");
        assert_eq!(edit.original_text, typed);
        let effects = update_queue(&mut state, QueueIntent::SaveEdit(edited.into()), &mut ids);
        assert!(
            effects.iter().any(
                |effect| matches!(effect, Effect::ReplaceQueued { text, .. } if *text == expected)
            ),
            "{effects:?}"
        );
    }
}

#[test]
fn resumed_routed_prompts_show_what_was_typed() {
    let follow_up = |typed: &str| follow_up_text(typed);
    let state = resumed(json!([
        // The first prompt of a session, as Hermes replays it.
        {"role": "user", "text": PROJECTED, "display_kind": "skill_invocation"},
        {"role": "assistant", "text": "Here is the proposed workflow."},
        // A short reply.
        {"role": "user", "text": follow_up("go ahead")},
        // A new task in the same conversation.
        {"role": "user", "text": follow_up("add a test for the lexer")},
        // The first prompt after a compaction, and after an interrupted turn.
        {"role": "user", "text": "/talaria-workflows   tidy the docs  ", "display_kind": "skill_invocation"},
        {"role": "user", "text": follow_up("keep going")},
        // A gateway that hands back the stored row unprojected.
        {"role": "user", "text": FULL},
        // Another skill the person typed stays as typed.
        {"role": "user", "text": "/review the diff", "display_kind": "skill_invocation"},
    ]));
    let lines = shown(&state);
    assert_eq!(
        users(&state),
        [
            "fix the parser",
            "go ahead",
            "add a test for the lexer",
            "tidy the docs",
            "keep going",
            "fix the parser",
            "/review the diff",
        ]
    );
    let lines: Vec<String> = lines
        .into_iter()
        .filter(|line| line != "> /review the diff")
        .collect();
    assert_hidden(&lines);
}

#[test]
fn rewind_list_previews_what_was_typed() {
    let messages = vec![
        json!({"role": "user", "row_id": 1, "content": FULL}),
        json!({"role": "user", "row_id": 3, "content": follow_up_text("go ahead")}),
    ];
    let turns = rewind_turns(&messages);
    let previews: Vec<&str> = turns.iter().map(|turn| turn.preview.as_str()).collect();
    assert_eq!(previews, ["go ahead", "fix the parser"]);
    assert_eq!(
        turns[1].prompt_text.as_deref(),
        Some(FULL),
        "a retry still sends the whole skill"
    );
}

#[test]
fn no_routing_notice_names_the_skill_command_or_wrapper() {
    let mut texts = vec![
        notice::ON.to_owned(),
        notice::ON_REMOTE.to_owned(),
        notice::AUTHORING_TITLE.to_owned(),
        notice::SKILL_ROLE.to_owned(),
        notice::bypassed("goal"),
        notice::collision("fixture"),
        notice::missing("fixture"),
        notice::unavailable("fixture"),
        notice::unknown("offline"),
    ];
    texts.retain(|text| !text.is_empty());
    assert_hidden(&texts);
}

#[test]
fn the_session_model_line_is_for_the_model_and_never_shown() {
    let line = "Session model: model-a on provider-a";
    // As sent: the line follows the request on its own row.
    let sent = format!("{FULL}\n{line}");
    assert_eq!(typed_text(&sent), Some("fix the parser"));
    let runtime = format!("{FULL}\n{line}\n\n[Runtime note: fixture]");
    assert_eq!(typed_text(&runtime), Some("fix the parser"));
    // As Hermes replays it: one row, line breaks folded to spaces.
    assert_eq!(
        replayed_request(&format!("/talaria-workflows fix the parser {line}")),
        Some("fix the parser")
    );
    assert_eq!(
        replayed_request(&format!("/talaria-workflows fix the parser\n{line}")),
        Some("fix the parser")
    );
    // Text that only mentions the label is the person's own.
    for own in [
        "/talaria-workflows explain the Session model: field",
        "/talaria-workflows rename Session model: to Model",
    ] {
        assert_eq!(
            replayed_request(own),
            own.strip_prefix("/talaria-workflows "),
            "{own}"
        );
    }
}

#[test]
fn an_edited_routed_prompt_keeps_the_session_model_line() {
    let line = "\nSession model: model-a on provider-a";
    let sent = format!("{FULL}{line}");
    let edited = retyped(&sent, "fix the lexer");
    assert_eq!(
        edited,
        format!("{}{line}", FULL.replace("fix the parser", "fix the lexer"))
    );
    assert_eq!(typed_text(&edited), Some("fix the lexer"));
    assert_eq!(retyped("plain text", "new text"), "new text");
}
