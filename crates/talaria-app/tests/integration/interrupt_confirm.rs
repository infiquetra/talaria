//! The interrupt key asks for a second press while a turn runs; Ctrl+C does not.

use hermes_contract::events::EventFrame;
use hermes_contract::schemas::CanonicalSnapshot;
use serde_json::json;
use talaria_app::app::{AppEffect, AppMessage, AppState, UserAction, update};
use talaria_app::sessions::OpenedSession;
use talaria_core::rewind::INTERRUPT_CONFIRM_WINDOW_MS;
use talaria_core::session::{Event, SessionId};

const SESSION: &str = "local-session-1";

fn snapshot(session: &str, running: bool) -> OpenedSession {
    let mut value: serde_json::Value =
        serde_json::from_str(include_str!("../data/running_turn.json")).expect("snapshot");
    value["session_id"] = json!(session);
    value["running"] = json!(running);
    let snapshot: CanonicalSnapshot = serde_json::from_value(value).expect("contract snapshot");
    OpenedSession::from_snapshot(snapshot)
}

fn running_state() -> AppState {
    let mut state = AppState::new("interrupt-confirm");
    let _ = update(
        &mut state,
        AppMessage::Opened {
            opened: snapshot(SESSION, true),
            notice: None,
        },
    );
    state
}

fn at(state: &mut AppState, now_ms: u64) {
    let _ = update(state, AppMessage::Tick { now_ms });
}

fn press(state: &mut AppState) -> Vec<AppEffect> {
    update(state, AppMessage::ForegroundKey(UserAction::InterruptKey))
}

fn interrupts(effects: &[AppEffect]) -> usize {
    effects
        .iter()
        .filter(|effect| matches!(effect, AppEffect::Interrupt(_)))
        .count()
}

#[test]
fn first_press_arms_and_sends_no_interrupt() {
    let mut state = running_state();
    at(&mut state, 1_000);
    assert!(!state.view().interrupt_armed);
    let effects = press(&mut state);
    assert_eq!(interrupts(&effects), 0);
    assert!(state.view().interrupt_armed, "the working row says again");
}

#[test]
fn second_press_inside_the_window_sends_exactly_one_interrupt() {
    let mut state = running_state();
    at(&mut state, 1_000);
    assert_eq!(interrupts(&press(&mut state)), 0);
    at(&mut state, 1_000 + INTERRUPT_CONFIRM_WINDOW_MS - 1);
    assert_eq!(interrupts(&press(&mut state)), 1);
    assert!(!state.view().interrupt_armed, "a confirmed press disarms");
}

#[test]
fn hint_returns_to_normal_after_the_window_and_the_next_press_arms_again() {
    let mut state = running_state();
    at(&mut state, 1_000);
    assert_eq!(interrupts(&press(&mut state)), 0);
    at(&mut state, 1_000 + INTERRUPT_CONFIRM_WINDOW_MS);
    assert!(!state.view().interrupt_armed, "the hint reverted on time");
    assert_eq!(interrupts(&press(&mut state)), 0, "a late press arms again");
    assert!(state.view().interrupt_armed);
    assert_eq!(interrupts(&press(&mut state)), 1);
}

#[test]
fn ctrl_c_interrupts_on_a_single_press() {
    let mut state = running_state();
    at(&mut state, 1_000);
    let effects = update(&mut state, AppMessage::User(UserAction::Interrupt));
    assert_eq!(interrupts(&effects), 1);
    assert!(!state.view().interrupt_armed);
}

#[test]
fn an_open_palette_closes_in_one_press_and_arms_nothing() {
    let mut state = running_state();
    at(&mut state, 1_000);
    state.draft = "/he".into();
    state.palette.open = true;
    let effects = press(&mut state);
    assert!(!state.palette.open, "the first press closes the palette");
    assert_eq!(interrupts(&effects), 0);
    assert!(!state.view().interrupt_armed, "closing a list arms nothing");
    assert_eq!(
        interrupts(&press(&mut state)),
        0,
        "the next press only arms"
    );
}

#[test]
fn a_turn_that_ends_between_the_presses_leaves_nothing_to_confirm() {
    let mut state = running_state();
    at(&mut state, 1_000);
    assert_eq!(interrupts(&press(&mut state)), 0);
    let params = json!({
        "type": "message.complete",
        "session_id": SESSION,
        "payload": {"text": "done", "status": "complete"},
    });
    let frame = EventFrame::from_params(&params).expect("contract-shaped event");
    let _ = update(
        &mut state,
        AppMessage::PromptDone {
            events: vec![Event::Session {
                session_id: SessionId::new(SESSION),
                frame,
                raw: params,
            }],
            notices: Vec::new(),
            masked: None,
        },
    );
    assert!(!state.view().turn_running(), "the turn ended");
    assert!(!state.view().interrupt_armed);
    at(&mut state, 1_500);
    let effects = press(&mut state);
    assert_eq!(interrupts(&effects), 0, "nothing is left to interrupt");
    assert!(
        effects
            .iter()
            .all(|effect| !matches!(effect, AppEffect::Interrupt(_)))
    );
    assert!(!state.view().interrupt_armed);
}

#[test]
fn an_armed_press_does_not_carry_into_another_session() {
    let mut state = running_state();
    at(&mut state, 1_000);
    assert_eq!(interrupts(&press(&mut state)), 0);
    let _ = update(
        &mut state,
        AppMessage::Opened {
            opened: snapshot("local-session-2", true),
            notice: None,
        },
    );
    at(&mut state, 1_500);
    assert!(!state.view().interrupt_armed);
    assert!(state.view().turn_running());
    assert_eq!(
        interrupts(&press(&mut state)),
        0,
        "the first press there arms"
    );
}
