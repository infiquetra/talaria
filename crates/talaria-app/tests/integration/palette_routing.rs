//! Foreground key order while a slash palette is open. Issue #223.

use std::time::Instant;

use serde_json::json;
use talaria_app::app::{AppEffect, AppMessage, AppState, UserAction, update};
use talaria_app::commands::CommandReply;
use talaria_app::sessions::OpenedSession;
use talaria_core::commands::{CommandsInput, PaletteInput};
use talaria_core::session::Cell;

#[test]
fn routing_order_card_queue_palette_composer() {
    let mut state = AppState::new("launch-1");
    state.draft = "/he".into();
    state.palette.open = true;

    let effects = update(
        &mut state,
        AppMessage::ForegroundKey(UserAction::InterruptKey),
    );
    assert!(!state.palette.open, "Esc closes the palette");
    assert_eq!(state.draft, "/he");
    assert!(
        effects
            .iter()
            .all(|effect| !matches!(effect, AppEffect::Interrupt(_))),
        "Esc does not interrupt while the palette is open"
    );

    state.draft.clear();
    state.palette.open = false;
    let card = update(&mut state, AppMessage::User(UserAction::Interrupt));
    assert!(!state.quitting);
    let _ = card;
}

fn edited(text: &str, recalled: bool) -> AppMessage {
    AppMessage::ForegroundKey(UserAction::DraftEdited {
        text: text.into(),
        caret: text.len(),
        typed: !recalled,
        recalled,
        now: Instant::now(),
    })
}

#[test]
fn escape_clears_only_the_bare_slash() {
    let mut state = AppState::new("palette-dismiss");
    for text in ["/", "/plan task"] {
        let _ = update(&mut state, edited(text, false));
        assert!(state.palette.open);
        let effects = update(
            &mut state,
            AppMessage::ForegroundKey(UserAction::Palette(PaletteInput::Escape)),
        );
        assert!(!state.palette.open);
        if text == "/" {
            assert!(state.draft.is_empty());
            assert!(
                effects
                    .iter()
                    .any(|effect| matches!(effect, AppEffect::LoadDraft(text) if text.is_empty()))
            );
        } else {
            assert_eq!(state.draft, text);
            let _ = update(&mut state, edited(text, false));
            assert!(!state.palette.open);
            let _ = update(&mut state, edited("/plan tasks", false));
            assert!(state.palette.open);
        }
    }
}

#[test]
fn programmatic_close_preserves_the_bare_slash() {
    let mut state = AppState::new("palette-close");
    let _ = update(&mut state, edited("/", false));
    let effects = update(
        &mut state,
        AppMessage::User(UserAction::Palette(PaletteInput::Close)),
    );
    assert_eq!(state.draft, "/");
    assert!(!state.palette.open);
    assert!(
        !effects
            .iter()
            .any(|effect| matches!(effect, AppEffect::LoadDraft(_)))
    );
}

fn open_session(state: &mut AppState, id: &str, stored_id: &str) {
    let mut value: serde_json::Value =
        serde_json::from_str(include_str!("../data/running_turn.json")).expect("snapshot");
    value["session_id"] = json!(id);
    value["stored_session_id"] = json!(stored_id);
    value["running"] = json!(false);
    let snapshot = serde_json::from_value(value).expect("contract snapshot");
    let _ = update(
        state,
        AppMessage::Opened {
            opened: OpenedSession::from_snapshot(snapshot),
            notice: None,
        },
    );
}

fn collision_reply(
    state: &mut AppState,
    session: Option<&str>,
    request_id: u64,
    reply_id: u64,
) -> Vec<AppEffect> {
    state.commands.bound_session = session.map(str::to_owned);
    state.commands.in_flight = Some(request_id);
    update(
        state,
        AppMessage::Commands(CommandReply::Commands(CommandsInput::CatalogArrived {
            request_id: reply_id,
            result: serde_json::from_value(json!({
                "pairs": [["/plan", "Plan"]],
                "warning": "slash command /plan unavailable — name taken by built-in; use /skill plan"
            }))
            .expect("command catalogue"),
        })),
    )
}

fn collision_cells(state: &AppState) -> Vec<usize> {
    state
        .opened
        .as_ref()
        .expect("open session")
        .state
        .cells()
        .iter()
        .enumerate()
        .filter_map(|(index, cell)| match cell {
            Cell::Notice(notice)
                if notice.text.contains("built-in /plan")
                    && notice.text.contains("/skill plan") =>
            {
                Some(index)
            }
            _ => None,
        })
        .collect()
}

#[test]
fn plan_collision_commits_one_notice_for_each_affected_session() {
    let mut state = AppState::new("plan-collision");
    let _ = collision_reply(&mut state, None, 1, 1);
    open_session(&mut state, "session-one", "stored-one");
    let _ = collision_reply(&mut state, Some("session-one"), 2, 1);
    assert!(collision_cells(&state).is_empty(), "stale reply");
    let _ = collision_reply(&mut state, Some("another-session"), 2, 2);
    assert!(
        collision_cells(&state).is_empty(),
        "reply for another session"
    );

    let effects = collision_reply(&mut state, Some("session-one"), 3, 3);
    let cells = collision_cells(&state);
    assert_eq!(cells.len(), 1);
    assert!(effects.iter().any(
        |effect| matches!(effect, AppEffect::CommitCells(range) if range.contains(&cells[0]))
    ));

    let _ = collision_reply(&mut state, Some("session-one"), 4, 4);
    let _ = update(&mut state, edited("/plan", false));
    assert!(state.palette.footer.is_none());
    assert_eq!(collision_cells(&state), cells);

    open_session(&mut state, "session-two", "stored-two");
    let _ = collision_reply(&mut state, Some("session-two"), 5, 5);
    assert_eq!(collision_cells(&state).len(), 1);
}

fn submitted(state: &mut AppState, text: &str) -> Vec<AppEffect> {
    update(
        state,
        AppMessage::ForegroundKey(UserAction::Submit(text.into())),
    )
}

fn recorded(effects: &[AppEffect]) -> Vec<String> {
    effects
        .iter()
        .filter_map(|effect| match effect {
            AppEffect::RecordSubmit(text) => Some(text.clone()),
            _ => None,
        })
        .collect()
}

#[test]
fn recalled_slash_entry_does_not_open_the_palette() {
    let mut state = AppState::new("history-recall");
    let _ = update(&mut state, edited("/profiles", true));
    assert_eq!(state.draft, "/profiles");
    assert!(!state.palette.open, "Up and Down stay with history");

    let _ = update(&mut state, edited("/profile", false));
    assert!(state.palette.open, "an edit opens the list again");
}

#[test]
fn picked_command_is_recorded_as_the_line_that_ran() {
    let mut state = AppState::new("history-pick");
    let _ = update(&mut state, edited("/profi", false));
    assert!(state.palette.open);
    let effects = submitted(&mut state, "/profi");
    assert_eq!(recorded(&effects), ["/profiles"]);
}

#[test]
fn enter_on_a_bare_slash_records_nothing() {
    let mut state = AppState::new("history-bare");
    let _ = update(&mut state, edited("/", false));
    assert!(state.palette.open);
    let effects = submitted(&mut state, "/");
    assert!(recorded(&effects).is_empty());
    assert!(
        effects
            .iter()
            .any(|effect| matches!(effect, AppEffect::LoadDraft(text) if text == "/"))
    );
}

#[test]
fn argument_command_records_once_with_its_argument_line() {
    let mut state = AppState::new("history-argument");
    let _ = update(&mut state, edited("/compa", false));
    let first = submitted(&mut state, "/compa");
    assert!(
        recorded(&first).is_empty(),
        "completing to /compact records nothing"
    );
    assert_eq!(state.draft, "/compact ");
    assert!(state.palette.open);

    let _ = update(&mut state, edited("/compact keep it short", false));
    let second = submitted(&mut state, "/compact keep it short");
    assert_eq!(recorded(&second), ["/compact keep it short"]);
}

#[test]
fn a_run_line_is_recorded_trimmed_so_it_can_be_taken_back() {
    let mut state = AppState::new("history-trim");
    let _ = update(&mut state, edited("/compa", false));
    let _ = submitted(&mut state, "/compa");
    assert_eq!(state.draft, "/compact ");
    let effects = submitted(&mut state, "/compact");
    assert_eq!(recorded(&effects), ["/compact"]);
    assert!(effects.iter().all(|effect| match effect {
        AppEffect::UnrecordSubmit(text) => text == "/compact",
        _ => true,
    }));
}
