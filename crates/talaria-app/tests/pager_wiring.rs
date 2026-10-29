//! The transcript is reached through the application update. Issue 222.
//!
//! Control-O opens over an approval card and commits nothing new until it closes.
//! Rebinding the open key to g closes it. Opening and closing send no gateway request.

#[path = "support/mock_gateway.rs"]
mod mock_gateway;

use hermes_contract::schemas::CanonicalSnapshot;
use mock_gateway::MockGateway;
use talaria_app::app::{AppEffect, AppMessage, AppState, InputCode, InputKey, update};
use talaria_app::launch::SessionRequest;
use talaria_app::sessions::{OpenedSession, SessionService};

const OPEN_APPROVAL: &str = include_str!("data/open_approval.json");
const LAUNCH: &str = "0123456789abcdef";

fn key(code: InputCode, ctrl: bool) -> AppMessage {
    AppMessage::Input(InputKey {
        code,
        ctrl,
        alt: false,
        shift: false,
    })
}

fn approval_card() -> OpenedSession {
    let snapshot: CanonicalSnapshot =
        serde_json::from_str(OPEN_APPROVAL).expect("approval snapshot");
    OpenedSession::from_snapshot(snapshot)
}

#[test]
fn ctrl_o_routes_keys_and_commits_after_close() {
    let opened = approval_card();
    assert!(
        opened.state.open_prompts().next().is_some(),
        "the approval card is open"
    );
    let mut state = AppState::new(LAUNCH);
    state.profile = "fixture".to_string();
    state
        .key_bindings
        .insert("transcript_open".to_string(), vec!["ctrl+o".to_string()]);
    let effects = update(
        &mut state,
        AppMessage::Opened {
            opened,
            notice: None,
        },
    );
    assert!(
        effects
            .iter()
            .any(|effect| matches!(effect, AppEffect::Track { .. })),
        "the session is tracked"
    );
    assert!(
        effects
            .iter()
            .all(|effect| !matches!(effect, AppEffect::Interrupt(_))),
        "opening the session does not interrupt"
    );
    assert!(
        state
            .opened
            .as_ref()
            .unwrap()
            .state
            .open_prompts()
            .next()
            .is_some(),
        "the card is still open"
    );
    let committed = state.committed_cells;

    let opened_effects = update(&mut state, key(InputCode::Char('o'), true));
    assert!(state.pager_open, "Control-O opens the transcript");
    assert_eq!(
        state.committed_cells, committed,
        "nothing commits while it is open"
    );
    assert!(opened_effects.iter().all(|effect| {
        !matches!(
            effect,
            AppEffect::Send(_) | AppEffect::Interrupt(_) | AppEffect::ServerRequest(_)
        )
    }));

    let _ = update(&mut state, key(InputCode::Char('q'), false));
    assert!(!state.pager_open, "q closes");
    assert_eq!(
        state.committed_cells, committed,
        "no cell became final while the transcript was open"
    );
    assert!(
        state
            .opened
            .as_ref()
            .unwrap()
            .state
            .open_prompts()
            .next()
            .is_some(),
        "the card is still openable"
    );

    state
        .key_bindings
        .insert("transcript_open".to_string(), vec!["g".to_string()]);
    let _ = update(&mut state, key(InputCode::Char('g'), false));
    assert!(
        state.pager_open,
        "g opens when transcript_open is rebound to g"
    );
    let _ = update(&mut state, key(InputCode::Char('g'), false));
    assert!(!state.pager_open, "the same binding closes");
    assert!(
        state
            .opened
            .as_ref()
            .unwrap()
            .state
            .open_prompts()
            .next()
            .is_some(),
        "closing does not answer the card"
    );
}

#[tokio::test]
async fn pager_sends_no_frames() {
    let (mock, client, _inbound) = MockGateway::start().await;
    let snapshot = mock_gateway::data_snapshot("running_turn.json");
    let session_id = snapshot["session_id"]
        .as_str()
        .unwrap_or("local-session-1")
        .to_string();
    mock.set_resume_snapshot(snapshot);
    let service = SessionService::with_client(LAUNCH, client);
    let opened = service
        .open(SessionRequest::ById { session_id })
        .await
        .expect("the snapshot opens");
    let history = mock.requests("session.history").len();
    let resume = mock.requests("session.resume").len();
    let frames = mock.frame_count();

    let mut state = AppState::new(LAUNCH);
    let _ = update(
        &mut state,
        AppMessage::Opened {
            opened,
            notice: None,
        },
    );
    assert_eq!(mock.requests("session.history").len(), history);
    assert_eq!(mock.requests("session.resume").len(), resume);
    assert_eq!(mock.frame_count(), frames);

    let keys = [
        key(InputCode::Char('o'), true),
        key(InputCode::Char('/'), false),
        key(InputCode::Char('h'), false),
        key(InputCode::Char('e'), false),
        key(InputCode::Char('r'), false),
        key(InputCode::Char('o'), false),
        key(InputCode::Char('n'), false),
        key(InputCode::Enter, false),
        key(InputCode::Char('n'), false),
        key(InputCode::PageUp, false),
        key(InputCode::Char('q'), false),
    ];
    for message in keys {
        let _ = update(&mut state, message);
        assert_eq!(
            mock.requests("session.history").len(),
            history,
            "no session.history"
        );
        assert_eq!(
            mock.requests("session.resume").len(),
            resume,
            "no second session.resume"
        );
        assert_eq!(mock.frame_count(), frames, "no new frame");
    }
}

#[test]
fn shifted_named_close_binding_does_not_match_plain_navigation() {
    let mut state = AppState::new(LAUNCH);
    state
        .key_bindings
        .insert("pager_close".into(), vec!["shift+up".into()]);
    update(&mut state, key(InputCode::Char('o'), true));
    assert!(state.pager_open);
    update(&mut state, key(InputCode::Up, false));
    assert!(state.pager_open, "plain Up must navigate, not close");
    update(
        &mut state,
        AppMessage::Input(InputKey {
            code: InputCode::Up,
            ctrl: false,
            alt: false,
            shift: true,
        }),
    );
    assert!(!state.pager_open, "Shift+Up closes");
}

#[test]
fn named_pager_chords_agree_with_terminal_modifier_matching() {
    use talaria_tui::keys::{Key, KeyChord, KeyCode, KeyContext, KeyKind, Mods};
    for binding in ["shift+up", "ctrl+up", "alt+up", "ctrl+alt+shift+up"] {
        let chord = KeyChord::parse(binding, KeyContext::Pager).unwrap();
        for bits in 0..8 {
            let ctrl = bits & 1 != 0;
            let alt = bits & 2 != 0;
            let shift = bits & 4 != 0;
            let terminal_key = Key {
                code: KeyCode::Up,
                mods: Mods {
                    ctrl,
                    alt,
                    shift,
                    ..Mods::default()
                },
                kind: KeyKind::Press,
            };
            let mut state = AppState::new(LAUNCH);
            state.pager_open = true;
            state
                .key_bindings
                .insert("pager_close".into(), vec![binding.into()]);
            update(
                &mut state,
                AppMessage::Input(InputKey {
                    code: InputCode::Up,
                    ctrl,
                    alt,
                    shift,
                }),
            );
            assert_eq!(
                !state.pager_open,
                chord.matches(&terminal_key),
                "{binding}, modifiers {bits}"
            );
        }
    }
}
