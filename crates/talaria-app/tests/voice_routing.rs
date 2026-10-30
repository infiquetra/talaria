//! Application routing of gateway voice events through the real session reducer.

use hermes_client::liveness::LinkEvent;
use hermes_client::{Generation, Inbound, InboundEvent};
use hermes_contract::events::EventFrame;
use hermes_contract::schemas::CanonicalSnapshot;
use serde_json::{Value, json};
use talaria_app::app::{AppEffect, AppMessage, AppState, UserAction, update};
use talaria_app::sessions::OpenedSession;
use talaria_core::session::Effect;
use talaria_core::voice::{RecordStatus, VoiceEffect, VoiceInput};

fn opened_state() -> AppState {
    let snapshot: CanonicalSnapshot = serde_json::from_str(include_str!("data/running_turn.json"))
        .expect("recorded session snapshot");
    let mut state = AppState::new("voice-routing");
    update(
        &mut state,
        AppMessage::Opened {
            opened: OpenedSession::from_snapshot(snapshot),
            notice: None,
        },
    );
    update(
        &mut state,
        AppMessage::Voice {
            session_id: "local-session-1".into(),
            generation: 0,
            input: VoiceInput::ModeRead(Some("voice_only".into())),
        },
    );
    state
}

fn event(kind: &str, payload: Value) -> AppMessage {
    let raw = json!({"type":kind, "session_id":"local-session-1", "payload":payload});
    let frame = EventFrame::from_params(&raw).expect("contract event");
    AppMessage::Link(LinkEvent::Inbound {
        item: Inbound::Event(InboundEvent {
            generation: Generation(1),
            frame,
            raw,
        }),
        replayed: false,
    })
}

#[test]
fn pending_start_ignores_another_press_and_a_draft_is_preserved() {
    let mut state = opened_state();
    let effects = update(&mut state, AppMessage::User(UserAction::VoiceRecord));
    assert!(effects.iter().any(|effect| matches!(
        effect,
        AppEffect::Voice {
            effect: VoiceEffect::Record { start: true },
            ..
        }
    )));
    update(
        &mut state,
        event("voice.status", json!({"state":"listening"})),
    );
    let ignored = update(&mut state, AppMessage::User(UserAction::VoiceRecord));
    assert!(!ignored.iter().any(|effect| matches!(
        effect,
        AppEffect::Voice {
            effect: VoiceEffect::Record { .. },
            ..
        }
    )));
    update(
        &mut state,
        AppMessage::Voice {
            session_id: "local-session-1".into(),
            generation: 0,
            input: VoiceInput::RecordReply {
                start: true,
                status: RecordStatus::Recording,
                reason: None,
            },
        },
    );
    state.draft = "already typed".into();
    let effects = update(
        &mut state,
        event("voice.transcript", json!({"text":"and spoken"})),
    );
    assert_eq!(state.draft, "already typed and spoken");
    assert!(effects.iter().any(
        |effect| matches!(effect, AppEffect::LoadDraft(text) if text == "already typed and spoken")
    ));
    assert!(
        !effects
            .iter()
            .any(|effect| matches!(effect, AppEffect::Send(Effect::SendPrompt { .. })))
    );
}

#[test]
fn first_transcript_submits_only_once_per_capture() {
    let mut state = opened_state();
    update(&mut state, AppMessage::User(UserAction::VoiceRecord));
    update(
        &mut state,
        AppMessage::Voice {
            session_id: "local-session-1".into(),
            generation: 0,
            input: VoiceInput::RecordReply {
                start: true,
                status: RecordStatus::Recording,
                reason: None,
            },
        },
    );
    let first = update(
        &mut state,
        event("voice.transcript", json!({"text":"hello gateway"})),
    );
    assert!(first.iter().any(|effect| matches!(effect, AppEffect::Send(Effect::SendPrompt { text, .. }) if text == "hello gateway")));
    let second = update(
        &mut state,
        event("voice.transcript", json!({"text":"a correction"})),
    );
    assert!(
        !second
            .iter()
            .any(|effect| matches!(effect, AppEffect::Send(Effect::SendPrompt { .. })))
    );
    assert_eq!(state.draft, "a correction");
}

#[test]
fn an_unavailable_voice_method_applies_to_every_session() {
    let mut voice = talaria_app::voice::VoiceService::new();
    voice.open("one");
    voice.open("two");
    for id in ["one", "two"] {
        voice.update(id, VoiceInput::ModeRead(Some("voice_only".into())), true);
    }
    voice.update(
        "one",
        VoiceInput::RecordFailed {
            start: true,
            code: Some(-32601),
            message: "unavailable".into(),
            dropped: false,
        },
        true,
    );
    let effects = voice.update("two", VoiceInput::Press, true);
    assert!(effects.iter().any(|effect| matches!(effect, VoiceEffect::Notice(text) if text == "Voice is not available on this gateway.")));
    assert!(
        !effects
            .iter()
            .any(|effect| matches!(effect, VoiceEffect::Record { .. }))
    );
}

#[test]
fn voice_configuration_enforces_the_speech_limit() {
    let defaults = talaria_app::config::parse("");
    assert!(!defaults.config.voice.read_aloud);
    assert_eq!(defaults.config.voice.speak_max_chars, 1500);

    let configured =
        talaria_app::config::parse("[voice]\nread_aloud = true\nspeak_max_chars = 100\n");
    assert!(configured.config.voice.read_aloud);
    assert_eq!(configured.config.voice.speak_max_chars, 100);
    assert!(configured.notices.is_empty());

    let invalid = talaria_app::config::parse("[voice]\nspeak_max_chars = 10001\n");
    assert_eq!(invalid.config.voice.speak_max_chars, 1500);
    assert!(
        invalid
            .notices
            .iter()
            .any(|notice| notice.contains("100 to 10000"))
    );
}

#[test]
fn push_to_talk_cannot_start_during_a_session_switch() {
    let mut state = opened_state();
    state.switching = true;
    let effects = update(&mut state, AppMessage::User(UserAction::VoiceRecord));
    assert!(!effects.iter().any(|effect| matches!(
        effect,
        AppEffect::Voice {
            effect: VoiceEffect::Record { .. },
            ..
        }
    )));
    assert_eq!(
        state
            .voice
            .get("local-session-1")
            .map(talaria_core::voice::VoiceState::phase),
        Some(talaria_core::voice::VoicePhase::Ready)
    );
}
