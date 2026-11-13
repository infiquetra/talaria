//! Session actions and model picks number their `session.mutate` requests under their own
//! prefixes, and the fences one reply carries reach the other ledger before its event does.

use hermes_client::replay::Cursor;
use serde_json::json;
use talaria_app::app::{AppEffect, AppMessage, AppState, UserAction, update};
use talaria_app::sessions::{OpenedSession, SessionInfoSummary};
use talaria_core::models::{ModelsEffect, ModelsInput};
use talaria_core::mutation::{MutationOutcome, MutationRequest};
use talaria_core::session::{SessionId, SessionState};

const LAUNCH: &str = "launch-1";
const SESSION: &str = "local-session-1";

fn opened_state() -> AppState {
    let mut state = AppState::new(LAUNCH);
    update(
        &mut state,
        AppMessage::Opened {
            opened: OpenedSession {
                state: SessionState::empty(SessionId::new(SESSION)),
                session_id: SESSION.to_owned(),
                stored_session_id: SESSION.to_owned(),
                subscription_id: "sub-1".to_owned(),
                info: SessionInfoSummary {
                    model: Some("minimax-m3.1-flash".to_owned()),
                    provider: Some("ollama-cloud".to_owned()),
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
                launch_notices: Vec::new(),
                effort: None,
            },
            notice: None,
        },
    );
    state
}

fn submit(state: &mut AppState, line: &str) -> Vec<AppEffect> {
    update(state, AppMessage::User(UserAction::Submit(line.to_owned())))
}

fn rename_request(effects: &[AppEffect]) -> MutationRequest {
    effects
        .iter()
        .find_map(|effect| match effect {
            AppEffect::Mutate { request, .. } if request.operation == "rename" => {
                Some(request.clone())
            }
            _ => None,
        })
        .expect("/rename sends a rename")
}

fn model_request(effects: &[AppEffect]) -> MutationRequest {
    effects
        .iter()
        .find_map(|effect| match effect {
            AppEffect::Models(ModelsEffect::Mutate(request)) if request.operation == "model" => {
                Some(request.clone())
            }
            _ => None,
        })
        .expect("/model sends a model pick")
}

fn committed(revision: i64) -> MutationOutcome {
    MutationOutcome::Committed(json!({"revision": revision, "execution_generation": 1}))
}

#[test]
fn rename_then_model_pick_use_distinct_ids_and_the_new_revision() {
    let mut state = opened_state();
    let rename = rename_request(&submit(&mut state, "/rename New title"));
    assert_eq!(rename.request_id, "launch-1-m1");
    assert_eq!(rename.expected_revision, 1);
    update(
        &mut state,
        AppMessage::Mutated {
            request: rename,
            outcome: committed(2),
        },
    );

    let pick = model_request(&submit(&mut state, "/model glm-5"));
    assert_eq!(pick.request_id, "launch-1-model-m1");
    assert_eq!(pick.expected_revision, 2);
}

#[test]
fn model_pick_then_rename_use_distinct_ids_and_the_new_revision() {
    let mut state = opened_state();
    let pick = model_request(&submit(&mut state, "/model glm-5"));
    assert_eq!(pick.request_id, "launch-1-model-m1");
    assert_eq!(pick.expected_revision, 1);
    update(
        &mut state,
        AppMessage::Models(ModelsInput::MutationReturned {
            request: pick,
            outcome: committed(2),
        }),
    );

    let rename = rename_request(&submit(&mut state, "/rename New title"));
    assert_eq!(rename.request_id, "launch-1-m1");
    assert_eq!(rename.expected_revision, 2);
}
