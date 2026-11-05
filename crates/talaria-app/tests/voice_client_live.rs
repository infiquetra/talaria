//! Application routing for client voice without a physical microphone or gateway.
use talaria_app::app::{AppEffect, AppMessage, AppState, UserAction, update};
use talaria_app::sessions::OpenedSession;
use talaria_core::voice::client::Input;

fn opened(remote: bool, opt_in: bool) -> AppState {
    let snapshot = serde_json::from_str(include_str!("data/running_turn.json")).unwrap();
    let mut state = AppState::new("client-voice");
    if remote {
        state.attached_to_remote_gateway();
    }
    let voice = talaria_app::config::VoiceConfig {
        backend: "client-bridge".into(),
        remote_audio: opt_in,
        ..Default::default()
    };
    state.configure_voice(voice);
    update(
        &mut state,
        AppMessage::Opened {
            opened: OpenedSession::from_snapshot(snapshot),
            notice: None,
        },
    );
    update(
        &mut state,
        AppMessage::User(UserAction::Submit("/voice on".into())),
    );
    state
}

#[test]
fn remote_opt_in_blocks_the_capture_effect_until_enabled() {
    let mut denied = opened(true, false);
    let effects = update(&mut denied, AppMessage::User(UserAction::VoiceRecord));
    assert!(
        !effects
            .iter()
            .any(|effect| matches!(effect, AppEffect::ClientVoiceCapture { .. }))
    );
    let mut allowed = opened(true, true);
    let effects = update(&mut allowed, AppMessage::User(UserAction::VoiceRecord));
    assert!(
        effects
            .iter()
            .any(|effect| matches!(effect, AppEffect::ClientVoiceCapture { epoch: 1, .. }))
    );
}

#[test]
fn client_text_becomes_one_draft_and_stale_results_are_ignored() {
    let mut state = opened(false, false);
    let effects = update(&mut state, AppMessage::User(UserAction::VoiceRecord));
    assert!(
        effects
            .iter()
            .any(|effect| matches!(effect, AppEffect::ClientVoiceCapture { epoch: 1, .. }))
    );
    assert_eq!(
        state.view().voice.unwrap().phase,
        talaria_core::voice::VoicePhase::Recording
    );
    let event = |input| AppMessage::ClientVoice {
        session_id: "local-session-1".into(),
        generation: 0,
        input,
        notice: None,
    };
    update(&mut state, event(Input::CaptureFinished { epoch: 1 }));
    update(
        &mut state,
        event(Input::Text {
            epoch: 1,
            text: "hello".into(),
        }),
    );
    update(
        &mut state,
        event(Input::Text {
            epoch: 1,
            text: "duplicate".into(),
        }),
    );
    assert_eq!(state.draft, "hello");
    update(
        &mut state,
        event(Input::Text {
            epoch: 0,
            text: "stale".into(),
        }),
    );
    assert_eq!(state.draft, "hello");
}

#[test]
fn detached_client_ignores_the_final_result() {
    let mut state = opened(false, false);
    update(&mut state, AppMessage::User(UserAction::VoiceRecord));
    let event = |input| AppMessage::ClientVoice {
        session_id: "local-session-1".into(),
        generation: 0,
        input,
        notice: None,
    };
    update(&mut state, event(Input::CaptureFinished { epoch: 1 }));
    update(&mut state, event(Input::Detach));
    update(
        &mut state,
        event(Input::Text {
            epoch: 1,
            text: "late".into(),
        }),
    );
    assert!(state.draft.is_empty());
}

#[test]
fn direct_mode_preserves_an_existing_draft() {
    let snapshot = serde_json::from_str(include_str!("data/running_turn.json")).unwrap();
    let mut state = AppState::new("client-voice-direct");
    let voice = talaria_app::config::VoiceConfig {
        backend: "client-bridge".into(),
        client_submit_mode: "direct".into(),
        ..Default::default()
    };
    state.configure_voice(voice);
    update(
        &mut state,
        AppMessage::Opened {
            opened: OpenedSession::from_snapshot(snapshot),
            notice: None,
        },
    );
    update(
        &mut state,
        AppMessage::User(UserAction::Submit("/voice on".into())),
    );
    state.draft = "typed".into();
    update(&mut state, AppMessage::User(UserAction::VoiceRecord));
    let event = |input| AppMessage::ClientVoice {
        session_id: "local-session-1".into(),
        generation: 0,
        input,
        notice: None,
    };
    update(&mut state, event(Input::CaptureFinished { epoch: 1 }));
    let effects = update(
        &mut state,
        event(Input::Text {
            epoch: 1,
            text: "spoken".into(),
        }),
    );
    assert_eq!(state.draft, "typed spoken");
    assert!(
        !effects
            .iter()
            .any(|effect| matches!(effect, AppEffect::Send(_)))
    );
}
