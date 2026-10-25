//! Tests 25 and 26 (#237): the permissions wiring through the pure app update.

#[path = "support/mock_gateway.rs"]
mod mock_gateway;

use hermes_client::{CallOptions, Client};
use hermes_contract::events::EventFrame;
use mock_gateway::{MockGateway, recorded_resume_snapshot};
use serde_json::{Value, json};
use talaria_app::app::{AppEffect, AppMessage, AppState, ToggleContext, UserAction, update};
use talaria_app::launch::SessionRequest;
use talaria_app::sessions::SessionService;
use talaria_core::permissions::{ApprovalMode, PermissionsInput};
use talaria_core::session::{Event, SessionId};

const LAUNCH_ID: &str = "0123456789abcdef";

/// Deliver one read value for every plausible generation. The app's read
/// generations are its own sequencing; replies for any other generation drop
/// silently, so the matching replies stick whatever the app chose.
fn deliver_reads(state: &mut AppState, session: &str) {
    for generation in 0..4 {
        update(
            state,
            AppMessage::Permissions(PermissionsInput::ProfileModeRead {
                generation,
                result: Ok(ApprovalMode::Manual),
            }),
        );
        update(
            state,
            AppMessage::Permissions(PermissionsInput::SessionOffRead {
                session_id: session.to_owned(),
                generation,
                result: Ok(true),
            }),
        );
    }
}

fn deliver_bypass_off(state: &mut AppState, session: &str) {
    for generation in 0..4 {
        update(
            state,
            AppMessage::Permissions(PermissionsInput::ProfileModeRead {
                generation,
                result: Ok(ApprovalMode::Smart),
            }),
        );
        update(
            state,
            AppMessage::Permissions(PermissionsInput::SessionOffRead {
                session_id: session.to_owned(),
                generation,
                result: Ok(false),
            }),
        );
    }
}

fn toggles(effects: &[AppEffect]) -> Vec<(String, u64, bool)> {
    effects
        .iter()
        .filter_map(|effect| match effect {
            AppEffect::SetSessionOff {
                session_id,
                generation,
                off,
            } => Some((session_id.clone(), *generation, *off)),
            _ => None,
        })
        .collect()
}

fn open_cards(state: &AppState) -> usize {
    state
        .opened
        .as_ref()
        .map(|opened| opened.state.open_prompts().count())
        .unwrap_or(0)
}

async fn resume_recorded(
    mock: &MockGateway,
    client: &Client,
) -> (String, talaria_app::sessions::OpenedSession) {
    let snapshot = recorded_resume_snapshot();
    let session = snapshot["session_id"]
        .as_str()
        .unwrap_or_default()
        .to_owned();
    mock.set_resume_snapshot(snapshot);
    let service = SessionService::with_client(LAUNCH_ID, client.clone());
    let opened = service
        .open(SessionRequest::ById {
            session_id: session.clone(),
        })
        .await
        .expect("the snapshot opens");
    (session, opened)
}

#[tokio::test]
async fn restarted_snapshot_bypass_is_ignored() {
    let (mock, client, _inbound) = MockGateway::start().await;
    mock.set_restarted(true);
    let snapshot = recorded_resume_snapshot();
    let session = snapshot["session_id"]
        .as_str()
        .unwrap_or_default()
        .to_owned();
    mock.set_session_bypass(&session, true);
    mock.set_resume_snapshot(snapshot);

    // The restarted gateway reports the bypass off in the snapshot while the read
    // reports it on.
    let service = SessionService::with_client(LAUNCH_ID, client.clone());
    let opened = service
        .open(SessionRequest::ById {
            session_id: session.clone(),
        })
        .await
        .expect("the snapshot opens");
    let raw = client
        .call_raw(
            "session.resume",
            json!({"session_id": session}),
            CallOptions { timeout: None },
        )
        .await
        .expect("a raw snapshot");
    assert_eq!(raw["info"]["yolo"], Value::Bool(false));
    let read = client
        .call_raw(
            "config.get",
            json!({"session_id": session, "key": "yolo"}),
            CallOptions { timeout: None },
        )
        .await
        .expect("a raw bypass read");
    assert_eq!(read["value"], Value::String("1".to_owned()));

    // The app follows the reads, not the snapshot.
    let mut state = AppState::new(LAUNCH_ID);
    update(
        &mut state,
        AppMessage::Opened {
            opened,
            notice: None,
        },
    );
    deliver_reads(&mut state, &session);
    let segment = state.view().approvals.expect("the segment shows");
    assert_eq!(segment.text, "approvals: off (this session)");

    // A later `session.info` reporting the bypass off changes nothing.
    let mut state = AppState::new(LAUNCH_ID);
    let (_, opened) = resume_recorded(&mock, &client).await;
    update(
        &mut state,
        AppMessage::Opened {
            opened,
            notice: None,
        },
    );
    deliver_reads(&mut state, &session);
    let before = state.view().approvals.map(|segment| segment.text);
    let params = json!({
        "type": "session.info",
        "session_id": session,
        "seq": 1,
        "replay_epoch": "epoch-1",
        "payload": {"cwd": "/work/proj", "yolo": false},
    });
    let frame = EventFrame::from_params(&params).expect("a session.info frame");
    update(
        &mut state,
        AppMessage::PromptDone {
            events: vec![Event::Session {
                session_id: SessionId::new(&session),
                frame,
                raw: params,
            }],
            notices: Vec::new(),
            masked: None,
        },
    );
    let after = state.view().approvals.map(|segment| segment.text);
    assert_eq!(before, after);
}

#[tokio::test]
async fn toggle_routing_by_context() {
    let (mock, client, _inbound) = MockGateway::start().await;
    let (session, opened) = resume_recorded(&mock, &client).await;
    let mut state = AppState::new(LAUNCH_ID);
    update(
        &mut state,
        AppMessage::Opened {
            opened,
            notice: None,
        },
    );
    assert_eq!(open_cards(&state), 1);
    deliver_bypass_off(&mut state, &session);

    // A completion list owns the key: nothing is sent.
    let effects = update(
        &mut state,
        AppMessage::User(UserAction::ApprovalsToggle {
            context: ToggleContext::Completion,
        }),
    );
    assert!(toggles(&effects).is_empty());
    assert_eq!(open_cards(&state), 1);

    // A focused card owns the key: nothing is sent and the card keeps its state.
    let effects = update(
        &mut state,
        AppMessage::User(UserAction::ApprovalsToggle {
            context: ToggleContext::Card,
        }),
    );
    assert!(toggles(&effects).is_empty());
    assert_eq!(open_cards(&state), 1);

    // The composer owns the key: exactly one bypass write goes out.
    let effects = update(
        &mut state,
        AppMessage::User(UserAction::ApprovalsToggle {
            context: ToggleContext::Composer,
        }),
    );
    assert_eq!(
        toggles(&effects)
            .iter()
            .map(|(session, _, off)| (session.clone(), *off))
            .collect::<Vec<_>>(),
        vec![(session, true)]
    );
}
