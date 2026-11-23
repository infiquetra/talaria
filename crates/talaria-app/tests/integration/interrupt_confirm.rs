//! The interrupt key asks for a second press while a turn runs, and a held key never interrupts;
//! Ctrl+C interrupts at once.

use hermes_contract::events::EventFrame;
use hermes_contract::schemas::CanonicalSnapshot;
use serde_json::json;
use talaria_app::app::{AppEffect, AppMessage, AppState, UserAction, update};
use talaria_app::sessions::OpenedSession;
use talaria_core::rewind::{INTERRUPT_CONFIRM_WINDOW_MS, INTERRUPT_SETTLE_MS};
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

/// Move the clock to `now_ms` and return what the tick did.
fn at(state: &mut AppState, now_ms: u64) -> Vec<AppEffect> {
    update(state, AppMessage::Tick { now_ms })
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
fn second_press_inside_the_window_interrupts_once_when_it_settles() {
    let mut state = running_state();
    at(&mut state, 1_000);
    assert_eq!(interrupts(&press(&mut state)), 0);
    let second = 1_000 + INTERRUPT_CONFIRM_WINDOW_MS - 1;
    at(&mut state, second);
    assert_eq!(
        interrupts(&press(&mut state)),
        0,
        "the press itself sends nothing"
    );
    assert!(
        state.view().interrupt_armed,
        "the working row still says again"
    );
    assert_eq!(
        interrupts(&at(&mut state, second + INTERRUPT_SETTLE_MS - 1)),
        0
    );
    assert_eq!(
        interrupts(&at(&mut state, second + INTERRUPT_SETTLE_MS)),
        1,
        "the settle time ends in one interrupt"
    );
    assert_eq!(
        interrupts(&at(&mut state, second + 2 * INTERRUPT_SETTLE_MS)),
        0,
        "and in no second one"
    );
    assert!(!state.view().interrupt_armed, "a settled press disarms");
}

#[test]
fn a_late_tick_with_the_key_press_first_still_interrupts_once() {
    let mut state = running_state();
    at(&mut state, 1_000);
    let _ = press(&mut state);
    at(&mut state, 1_300);
    let _ = press(&mut state);
    // The settle time passed with no tick; the next key press settles it first.
    let _ = update(
        &mut state,
        AppMessage::Tick {
            now_ms: 1_300 + INTERRUPT_SETTLE_MS - 1,
        },
    );
    state.now_ms = 1_300 + INTERRUPT_SETTLE_MS + 5;
    assert_eq!(
        interrupts(&press(&mut state)),
        1,
        "the settled press interrupts"
    );
}

#[test]
fn a_held_key_sends_no_interrupt_and_the_next_taps_still_work() {
    let mut state = running_state();
    let mut now = 1_000;
    at(&mut state, now);
    assert_eq!(interrupts(&press(&mut state)), 0);
    // The operating system starts repeating after a gap, then every 40 ms.
    now += 400;
    let mut total = 0;
    for _ in 0..40 {
        total += interrupts(&at(&mut state, now));
        total += interrupts(&press(&mut state));
        now += 40;
    }
    assert_eq!(total, 0, "a held key never interrupts");
    assert!(
        !state.view().interrupt_armed,
        "a held key shows the normal hint"
    );
    let end = now - 40 + INTERRUPT_SETTLE_MS;
    assert_eq!(
        interrupts(&at(&mut state, end)),
        0,
        "the held state just ends"
    );
    // Tap, tap now interrupts.
    now = end + 500;
    at(&mut state, now);
    assert_eq!(interrupts(&press(&mut state)), 0);
    now += 300;
    at(&mut state, now);
    assert_eq!(interrupts(&press(&mut state)), 0);
    assert_eq!(interrupts(&at(&mut state, now + INTERRUPT_SETTLE_MS)), 1);
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
    assert_eq!(interrupts(&press(&mut state)), 0, "the second press waits");
    let settled = 1_000 + INTERRUPT_CONFIRM_WINDOW_MS + INTERRUPT_SETTLE_MS;
    assert_eq!(interrupts(&at(&mut state, settled)), 1);
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

#[test]
fn a_turn_that_ends_while_a_press_is_pending_interrupts_nothing() {
    let mut state = running_state();
    at(&mut state, 1_000);
    let _ = press(&mut state);
    at(&mut state, 1_300);
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
    let effects = at(&mut state, 1_300 + INTERRUPT_SETTLE_MS);
    assert_eq!(interrupts(&effects), 0, "nothing is left to interrupt");
}

#[test]
fn a_session_change_while_a_press_is_pending_interrupts_nothing() {
    let mut state = running_state();
    at(&mut state, 1_000);
    let _ = press(&mut state);
    at(&mut state, 1_300);
    assert_eq!(interrupts(&press(&mut state)), 0);
    let _ = update(
        &mut state,
        AppMessage::Opened {
            opened: snapshot("local-session-2", true),
            notice: None,
        },
    );
    assert!(
        state.view().turn_running(),
        "the other session is running too"
    );
    let effects = at(&mut state, 1_300 + INTERRUPT_SETTLE_MS);
    assert_eq!(
        interrupts(&effects),
        0,
        "the other session is not interrupted"
    );
    assert!(!state.view().interrupt_armed);
}
