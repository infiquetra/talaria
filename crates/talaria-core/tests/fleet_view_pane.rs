//! Opening a fleet row in a new multiplexer pane. Issue #246.

use talaria_core::directory::rows::RowKey;
use talaria_core::fleet::{FleetRow, Outcome, ProfileStatus, RowState};
use talaria_core::fleet_view::{FleetAction, FleetViewEffect, FleetViewInput, update};
use talaria_core::fleet_view::{FleetViewState, PeekStatus};

const NOW: f64 = 1_792_700_000.0;
const PAGE: usize = 4;

fn key(profile: &str, id: &str) -> RowKey {
    RowKey {
        profile: profile.to_owned(),
        session_id: id.to_owned(),
    }
}

fn row(profile: &str, id: &str, title: &str, state: RowState) -> FleetRow {
    FleetRow {
        key: key(profile, id),
        title: title.to_owned(),
        model: Some("ollama-cloud/minimax-m3.1-flash".to_owned()),
        source: Some("tui".to_owned()),
        started_at: Some(NOW - 60.0),
        message_count: 2,
        state,
        state_since: NOW - 20.0,
        outcome: None,
        foreground: false,
        subscribed: false,
        stale: false,
        gone: false,
        unlisted: false,
        activity: None,
        cost_usd: None,
    }
}

fn snap(rows: Vec<FleetRow>) -> FleetViewInput {
    FleetViewInput::Snapshot {
        version: 1,
        rows,
        queue: Vec::new(),
        profiles: vec![
            (
                "fixture".to_owned(),
                ProfileStatus::Listed { truncated: false },
            ),
            (
                "default".to_owned(),
                ProfileStatus::Listed { truncated: false },
            ),
        ],
        scope_note: None,
        link_note: None,
    }
}

fn three_rows() -> (FleetRow, FleetRow, FleetRow) {
    let mut foreground = row("fixture", "s-front", "compile", RowState::Working);
    foreground.foreground = true;
    let working = row("fixture", "s-work", "migrate", RowState::Working);
    let mut done = row("fixture", "s-done", "ship", RowState::Done);
    done.outcome = Some(Outcome::Completed);
    (foreground, working, done)
}

#[test]
fn o_on_background_row_emits_open_in_pane() {
    let (foreground, working, done) = three_rows();
    // The cursor starts on the first row, so the working row goes first.
    let mut state = FleetViewState::new(Some(foreground.key.clone()), NOW);
    update(
        &mut state,
        snap(vec![working.clone(), foreground, done]),
        PAGE,
    );
    let effects = update(
        &mut state,
        FleetViewInput::Action(FleetAction::OpenInPane),
        PAGE,
    );
    assert_eq!(
        effects,
        vec![FleetViewEffect::OpenInPane(working.key.clone())]
    );

    // With a peek open on the working row: the same effect, and the peek stays.
    let (foreground, working, done) = three_rows();
    let mut state = FleetViewState::new(Some(foreground.key.clone()), NOW);
    update(
        &mut state,
        snap(vec![working.clone(), foreground, done]),
        PAGE,
    );
    let peeked = update(&mut state, FleetViewInput::Action(FleetAction::Peek), PAGE);
    assert_eq!(peeked, vec![FleetViewEffect::OpenPeek(working.key.clone())]);
    update(
        &mut state,
        FleetViewInput::Peek {
            key: working.key.clone(),
            status: PeekStatus::Open {
                has_open_prompt: false,
                running: true,
            },
        },
        PAGE,
    );
    let effects = update(
        &mut state,
        FleetViewInput::Action(FleetAction::OpenInPane),
        PAGE,
    );
    assert_eq!(
        effects,
        vec![FleetViewEffect::OpenInPane(working.key.clone())]
    );
    assert!(
        state.peek().is_some(),
        "the peek closed when the pane opened"
    );
}

#[test]
fn o_on_foreground_row_refuses_with_notice() {
    let (foreground, working, done) = three_rows();
    let mut state = FleetViewState::new(Some(foreground.key.clone()), NOW);
    update(&mut state, snap(vec![foreground, working, done]), PAGE);
    let effects = update(
        &mut state,
        FleetViewInput::Action(FleetAction::OpenInPane),
        PAGE,
    );
    assert!(effects.is_empty(), "the foreground row emitted {effects:?}");
    assert_eq!(
        state.notice(),
        Some("This session is already open in this pane.")
    );
}

#[test]
fn o_in_filter_reply_and_confirm_stop_is_not_an_open() {
    let (foreground, working, done) = three_rows();
    let mut state = FleetViewState::new(Some(foreground.key.clone()), NOW);
    update(
        &mut state,
        snap(vec![working.clone(), foreground, done]),
        PAGE,
    );
    // In Filter focus `o` is text.
    update(
        &mut state,
        FleetViewInput::Action(FleetAction::Filter),
        PAGE,
    );
    let effects = update(&mut state, FleetViewInput::Text('o'), PAGE);
    assert!(
        !effects
            .iter()
            .any(|effect| matches!(effect, FleetViewEffect::OpenInPane(_))),
        "filter text opened a pane: {effects:?}"
    );
    update(&mut state, FleetViewInput::Escape, PAGE);
    // In Reply focus `o` is text.
    update(&mut state, FleetViewInput::Action(FleetAction::Reply), PAGE);
    let effects = update(&mut state, FleetViewInput::Text('o'), PAGE);
    assert!(
        !effects
            .iter()
            .any(|effect| matches!(effect, FleetViewEffect::OpenInPane(_))),
        "reply text opened a pane: {effects:?}"
    );
    update(&mut state, FleetViewInput::Escape, PAGE);
    // In ConfirmStop `o` cancels the stop, like every other key.
    update(&mut state, FleetViewInput::Action(FleetAction::Stop), PAGE);
    let effects = update(
        &mut state,
        FleetViewInput::Action(FleetAction::OpenInPane),
        PAGE,
    );
    assert!(
        !effects
            .iter()
            .any(|effect| matches!(effect, FleetViewEffect::OpenInPane(_))),
        "a stop cancel opened a pane: {effects:?}"
    );
    assert_eq!(state.notice(), Some("Stop cancelled."));
    // In Answer focus `o` belongs to the card.
    update(&mut state, FleetViewInput::Action(FleetAction::Peek), PAGE);
    update(
        &mut state,
        FleetViewInput::Peek {
            key: working.key.clone(),
            status: PeekStatus::Open {
                has_open_prompt: true,
                running: false,
            },
        },
        PAGE,
    );
    update(
        &mut state,
        FleetViewInput::Action(FleetAction::Answer),
        PAGE,
    );
    let effects = update(
        &mut state,
        FleetViewInput::Action(FleetAction::OpenInPane),
        PAGE,
    );
    assert!(
        !effects
            .iter()
            .any(|effect| matches!(effect, FleetViewEffect::OpenInPane(_))),
        "the card's key opened a pane: {effects:?}"
    );
    // Back in the list, `o` opens again.
    update(&mut state, FleetViewInput::Escape, PAGE);
    let effects = update(
        &mut state,
        FleetViewInput::Action(FleetAction::OpenInPane),
        PAGE,
    );
    assert_eq!(
        effects,
        vec![FleetViewEffect::OpenInPane(working.key.clone())]
    );
}

#[test]
fn o_with_no_rows_does_nothing() {
    let mut state = FleetViewState::new(None, NOW);
    let effects = update(
        &mut state,
        FleetViewInput::Action(FleetAction::OpenInPane),
        PAGE,
    );
    assert!(effects.is_empty(), "no rows but an effect: {effects:?}");
    let working = row("fixture", "s-work", "migrate", RowState::Working);
    update(&mut state, snap(vec![working.clone()]), PAGE);
    let effects = update(
        &mut state,
        FleetViewInput::Action(FleetAction::OpenInPane),
        PAGE,
    );
    assert_eq!(
        effects,
        vec![FleetViewEffect::OpenInPane(working.key.clone())]
    );
}
