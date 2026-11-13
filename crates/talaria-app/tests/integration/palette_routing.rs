//! Foreground key order while a slash palette is open. Issue #223.

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
