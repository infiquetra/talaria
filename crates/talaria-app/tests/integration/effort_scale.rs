//! Bare `/effort` opens a scale under the composer. Left and Right move it, Tab stages
//! Ultracode, the `/model` picker's scope key stages saving for new sessions, and Enter sends
//! exactly what the typed `/effort` and `/ultracode` commands send. Esc sends nothing.

use hermes_client::replay::Cursor;
use serde_json::json;
use talaria_app::app::{AppEffect, AppMessage, AppState, UserAction, update};
use talaria_app::sessions::{OpenedSession, SessionInfoSummary};
use talaria_core::models::{Effort, ModelsEffect, ModelsInput, ScaleKey};
use talaria_core::mutation::{MutationOutcome, MutationRequest};
use talaria_core::session::{SessionId, SessionState};
use talaria_core::ultracode::notice;

/// A session `s` on `model-a` at effort high.
fn opened() -> AppState {
    let mut state = AppState::new("L");
    update(
        &mut state,
        AppMessage::Opened {
            opened: OpenedSession {
                state: SessionState::empty(SessionId::new("s")),
                session_id: "s".into(),
                stored_session_id: "s".into(),
                subscription_id: "sub".into(),
                info: SessionInfoSummary {
                    model: Some("model-a".into()),
                    provider: Some("p".into()),
                    cwd: None,
                    branch: None,
                },
                title: None,
                cursor: Cursor {
                    replay_epoch: String::new(),
                    last_sequence: 0,
                },
                revision: 1,
                execution_generation: 1,
                launch_notices: vec![],
                effort: None,
            },
            notice: None,
        },
    );
    update(
        &mut state,
        AppMessage::Models(ModelsInput::SessionEvent {
            session_id: "s".into(),
            type_name: "session.info".into(),
            payload: json!({"model":"model-a", "provider":"p", "reasoning_effort":"high"}),
        }),
    );
    state
}

fn submit(state: &mut AppState, text: &str) -> Vec<AppEffect> {
    update(state, AppMessage::User(UserAction::Submit(text.into())))
}

fn keys(state: &mut AppState, keys: &[ScaleKey]) -> Vec<AppEffect> {
    let mut effects = Vec::new();
    for key in keys {
        effects.extend(update(
            state,
            AppMessage::User(UserAction::EffortScale(*key)),
        ));
    }
    effects
}

fn mutations(effects: &[AppEffect]) -> Vec<MutationRequest> {
    effects
        .iter()
        .filter_map(|effect| match effect {
            AppEffect::Models(ModelsEffect::Mutate(request)) => Some(request.clone()),
            _ => None,
        })
        .collect()
}

fn models(effects: &[AppEffect]) -> Vec<ModelsEffect> {
    effects
        .iter()
        .filter_map(|effect| match effect {
            AppEffect::Models(effect) => Some(effect.clone()),
            _ => None,
        })
        .collect()
}

fn lines(effects: &[AppEffect]) -> Vec<String> {
    effects
        .iter()
        .filter_map(|effect| match effect {
            AppEffect::CommitLines(lines) => Some(lines.join("\n")),
            _ => None,
        })
        .collect()
}

fn ultracode_work(effects: &[AppEffect]) -> usize {
    effects
        .iter()
        .filter(|effect| matches!(effect, AppEffect::Ultracode { .. }))
        .count()
}

/// Nothing but a redraw: no write, no read, no notice.
fn only_redraws(effects: &[AppEffect]) -> bool {
    effects
        .iter()
        .all(|effect| matches!(effect, AppEffect::Redraw))
}

fn selected(state: &AppState) -> Option<Effort> {
    state.view().effort_scale.map(|scale| scale.selected())
}

fn commit(state: &mut AppState, request: MutationRequest, effort: &str) -> Vec<AppEffect> {
    update(
        state,
        AppMessage::Models(ModelsInput::MutationReturned {
            request,
            outcome: MutationOutcome::Committed(
                json!({"effort": effort, "revision": 2, "execution_generation": 1}),
            ),
        }),
    )
}

#[test]
fn bare_effort_opens_the_scale_on_the_current_level_and_reads_it_quietly() {
    let mut state = opened();
    let effects = submit(&mut state, "/effort");
    assert_eq!(selected(&state), Some(Effort::High));
    assert!(
        models(&effects).contains(&ModelsEffect::ReadEffort {
            session_id: "s".into()
        }),
        "the scale reads the session's level"
    );
    // The read answers: no "Effort for this session" line, and the untouched marker follows.
    let effects = update(
        &mut state,
        AppMessage::Models(ModelsInput::EffortRead {
            session_id: "s".into(),
            value: Some("max".into()),
        }),
    );
    assert!(
        !models(&effects)
            .iter()
            .any(|effect| matches!(effect, ModelsEffect::Notice { .. })),
        "the quiet read prints nothing"
    );
    assert!(lines(&effects).is_empty());
    assert_eq!(selected(&state), Some(Effort::Max));
}

#[test]
fn the_quiet_read_keeps_the_route_note_while_the_level_is_unchanged() {
    let mut state = opened();
    let info = |effort: &str| {
        AppMessage::Models(ModelsInput::SessionEvent {
            session_id: "s".into(),
            type_name: "session.info".into(),
            payload: json!({"reasoning_effort": effort, "reasoning_effort_wire": "high"}),
        })
    };
    let read = |effort: &str| {
        AppMessage::Models(ModelsInput::EffortRead {
            session_id: "s".into(),
            value: Some(effort.into()),
        })
    };
    let note = |state: &AppState| state.view().effort_scale.and_then(|scale| scale.note());
    let sends_high = Some("Hermes sends high on this route.".to_owned());
    update(&mut state, info("max"));
    submit(&mut state, "/effort");
    assert_eq!(note(&state), sends_high);
    update(&mut state, read("max"));
    assert_eq!(note(&state), sends_high, "the same level keeps the note");
    assert_eq!(state.view().effort_wire, Some("high"));
    update(&mut state, read("low"));
    assert_eq!(note(&state), None, "a new level drops the old route's note");
    assert_eq!(state.view().effort_wire, None);
}

#[test]
fn right_then_enter_sends_what_typed_effort_sends() {
    let mut scale = opened();
    submit(&mut scale, "/effort");
    let moved = keys(&mut scale, &[ScaleKey::Higher]);
    assert!(only_redraws(&moved), "moving writes nothing");
    let applied = keys(&mut scale, &[ScaleKey::Apply]);
    assert!(
        scale.view().effort_scale.is_none(),
        "Enter closes the scale"
    );

    let mut typed = opened();
    let expected = submit(&mut typed, "/effort xhigh");
    let (got, want) = (mutations(&applied), mutations(&expected));
    assert_eq!(got.len(), 1);
    assert_eq!(
        got.iter()
            .map(|r| (&r.operation, &r.payload))
            .collect::<Vec<_>>(),
        want.iter()
            .map(|r| (&r.operation, &r.payload))
            .collect::<Vec<_>>()
    );
    assert_eq!(got[0].payload.get("effort"), Some(&json!("xhigh")));
    let notices = |effects: &[AppEffect]| {
        models(effects)
            .into_iter()
            .filter_map(|effect| match effect {
                ModelsEffect::Notice { text, .. } => Some(text),
                _ => None,
            })
            .collect::<Vec<_>>()
    };
    let request = got[0].clone();
    assert_eq!(
        notices(&commit(&mut scale, request, "xhigh")),
        ["Effort: xhigh for this session."]
    );
}

#[test]
fn the_save_switch_sends_what_typed_effort_default_sends() {
    let mut scale = opened();
    submit(&mut scale, "/effort");
    let applied = keys(&mut scale, &[ScaleKey::ToggleSave, ScaleKey::Apply]);
    let mut typed = opened();
    let expected = submit(&mut typed, "/effort high --default");
    let (got, want) = (mutations(&applied), mutations(&expected));
    assert_eq!(got.len(), 1, "saving sends the level even when unchanged");
    assert_eq!(
        (&got[0].operation, &got[0].payload),
        (&want[0].operation, &want[0].payload)
    );
    let after_scale = models(&commit(&mut scale, got[0].clone(), "high"));
    let after_typed = models(&commit(&mut typed, want[0].clone(), "high"));
    assert_eq!(after_scale, after_typed, "the receipt saves alike");
}

#[test]
fn tab_then_enter_turns_ultracode_off_as_the_typed_command_does() {
    let mut state = opened();
    state.ultracode = talaria_app::ultracode::UltracodeService::new(true);
    assert!(state.view().ultracode);
    submit(&mut state, "/effort");
    let staged = keys(&mut state, &[ScaleKey::ToggleUltracode]);
    assert!(only_redraws(&staged), "Tab only stages the switch");
    assert!(state.view().ultracode, "nothing changes before Enter");
    let effects = keys(&mut state, &[ScaleKey::Apply]);
    assert!(!state.view().ultracode);
    assert_eq!(lines(&effects), [format!("\u{b7} {}", notice::OFF)]);
    assert!(mutations(&effects).is_empty(), "the level was not changed");
}

#[test]
fn tab_then_enter_asks_for_ultracode_on_as_the_typed_command_does() {
    let mut scale = opened();
    submit(&mut scale, "/effort");
    let effects = keys(
        &mut scale,
        &[ScaleKey::Lower, ScaleKey::ToggleUltracode, ScaleKey::Apply],
    );
    let mut typed = opened();
    let expected = submit(&mut typed, "/ultracode on");
    assert_eq!(ultracode_work(&expected), 1);
    assert_eq!(ultracode_work(&effects), 1, "the skill check runs");
    assert_eq!(
        mutations(&effects)[0].payload.get("effort"),
        Some(&json!("medium")),
        "effort and Ultracode apply together"
    );
}

#[test]
fn escape_drops_everything_staged() {
    let mut state = opened();
    submit(&mut state, "/effort");
    let effects = keys(
        &mut state,
        &[
            ScaleKey::Higher,
            ScaleKey::ToggleUltracode,
            ScaleKey::ToggleSave,
            ScaleKey::Cancel,
        ],
    );
    assert!(only_redraws(&effects), "Esc sends nothing");
    assert!(state.view().effort_scale.is_none());
    assert!(!state.view().ultracode);
    assert_eq!(state.view().effort, Some("high"));
}

#[test]
fn enter_with_nothing_changed_closes_silently() {
    let mut state = opened();
    submit(&mut state, "/effort");
    let effects = keys(&mut state, &[ScaleKey::Apply]);
    assert!(only_redraws(&effects));
    assert!(state.view().effort_scale.is_none());
}

#[test]
fn a_typed_level_still_sets_directly_and_opens_no_scale() {
    let mut state = opened();
    let effects = submit(&mut state, "/effort low");
    assert!(state.view().effort_scale.is_none());
    assert_eq!(mutations(&effects).len(), 1);
}

#[test]
fn a_paste_or_a_session_switch_closes_the_scale_with_nothing_sent() {
    let mut state = opened();
    submit(&mut state, "/effort");
    keys(&mut state, &[ScaleKey::Higher]);
    let effects = update(
        &mut state,
        AppMessage::User(UserAction::Paste {
            text: "pasted words".into(),
            draft: String::new(),
            caret: 0,
        }),
    );
    assert!(state.view().effort_scale.is_none(), "a paste closes it");
    assert!(mutations(&effects).is_empty());

    submit(&mut state, "/effort");
    keys(&mut state, &[ScaleKey::Higher]);
    let effects = submit(&mut state, "/resume other-session");
    assert!(state.view().effort_scale.is_none(), "a switch closes it");
    assert!(mutations(&effects).is_empty());
}

#[test]
fn without_a_session_bare_effort_answers_as_before() {
    let mut state = AppState::new("L");
    submit(&mut state, "/effort");
    assert!(state.view().effort_scale.is_none());
}

#[test]
fn a_second_selector_replaces_the_first() {
    let mut state = opened();
    submit(&mut state, "/effort");
    assert!(state.view().choice.is_none(), "the scale is not a list");
    keys(&mut state, &[ScaleKey::Higher]);
    submit(&mut state, "/effort");
    assert_eq!(
        selected(&state),
        Some(Effort::High),
        "a fresh scale opens on the current level"
    );
}

#[test]
fn enter_after_the_foreground_session_changed_sends_nothing() {
    let mut state = opened();
    submit(&mut state, "/effort");
    keys(&mut state, &[ScaleKey::Higher]);
    if let Some(opened) = &mut state.opened {
        opened.session_id = "t".into();
    }
    let effects = keys(&mut state, &[ScaleKey::Apply]);
    assert!(mutations(&effects).is_empty());
    assert_eq!(
        lines(&effects),
        ["\u{b7} The session changed while the effort selector was open, so nothing was changed."]
    );
}
