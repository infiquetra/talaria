//! Foreground key order while a slash palette is open. Issue #223.

use std::time::Instant;

use talaria_app::app::{AppEffect, AppMessage, AppState, UserAction, update};

#[test]
fn routing_order_card_queue_palette_composer() {
    let mut state = AppState::new("launch-1");
    state.draft = "/he".into();
    state.palette.open = true;

    let effects = update(&mut state, AppMessage::ForegroundKey(UserAction::Interrupt));
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
