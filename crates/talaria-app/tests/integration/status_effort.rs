//! Effort reported by session info and subsequent committed changes.
use hermes_client::replay::Cursor;
use serde_json::json;
use talaria_app::app::{AppEffect, AppMessage, AppState, UserAction, update};
use talaria_app::sessions::{OpenedSession, SessionInfoSummary};
use talaria_core::models::{ModelsEffect, ModelsInput};
use talaria_core::mutation::MutationOutcome;
use talaria_core::session::{SessionId, SessionState};

#[test]
fn wire_effort_is_cleared_and_refreshed() {
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
                    model: Some("m".into()),
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
    let info = || {
        AppMessage::Models(ModelsInput::SessionEvent {
            session_id: "s".into(),
            type_name: "session.info".into(),
            payload: json!({"reasoning_effort":"max", "reasoning_effort_wire":"high"}),
        })
    };
    update(&mut state, info());
    assert_eq!(
        (state.view().effort, state.view().effort_wire),
        (Some("max"), Some("high"))
    );
    let effects = update(
        &mut state,
        AppMessage::User(UserAction::Submit("/effort low".into())),
    );
    let request = effects
        .into_iter()
        .find_map(|e| {
            if let AppEffect::Models(ModelsEffect::Mutate(r)) = e {
                Some(r)
            } else {
                None
            }
        })
        .unwrap();
    update(
        &mut state,
        AppMessage::Models(ModelsInput::MutationReturned {
            request,
            outcome: MutationOutcome::Committed(
                json!({"effort":"low", "revision":2, "execution_generation":1}),
            ),
        }),
    );
    assert_eq!(
        (state.view().effort, state.view().effort_wire),
        (Some("low"), None)
    );
    update(&mut state, info());
    assert_eq!(state.view().effort_wire, Some("high"));
}
