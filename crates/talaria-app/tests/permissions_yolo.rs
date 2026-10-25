//! The session bypass and the standing yolo row read the same Hermes value: a bypass write
//! or read through the permissions reducer reads the yolo mode again.

use hermes_client::replay::Cursor;
use talaria_app::app::{AppEffect, AppMessage, AppState, ToggleContext, UserAction, update};
use talaria_app::sessions::{OpenedSession, SessionInfoSummary};
use talaria_core::automation::{AutomationEffect, AutomationEvent, ModeKey, ModeValue, rows};
use talaria_core::permissions::{ApprovalMode, CallFailure, PermissionsInput};
use talaria_core::session::{SessionId, SessionState};

const SESSION: &str = "local-session-1";

fn opened() -> OpenedSession {
    OpenedSession {
        state: SessionState::empty(SessionId::new(SESSION)),
        session_id: SESSION.to_owned(),
        stored_session_id: SESSION.to_owned(),
        subscription_id: "sub-1".to_owned(),
        info: SessionInfoSummary {
            model: None,
            provider: None,
            cwd: None,
            branch: None,
        },
        title: None,
        cursor: Cursor {
            replay_epoch: String::new(),
            last_sequence: 0,
        },
        revision: 0,
        execution_generation: 0,
        launch_notices: Vec::new(),
        effort: None,
    }
}

/// The yolo mode reads among `effects`, as (session, token).
fn yolo_reads(effects: &[AppEffect]) -> Vec<(String, u64)> {
    effects
        .iter()
        .filter_map(|effect| match effect {
            AppEffect::Automation(AutomationEffect::ReadMode {
                session_id,
                key: ModeKey::Yolo,
                token,
            }) => Some((session_id.clone(), *token)),
            _ => None,
        })
        .collect()
}

fn answer_yolo(state: &mut AppState, token: u64, value: &str) {
    update(
        state,
        AppMessage::Automation(AutomationEvent::ModeRead {
            session_id: SESSION.to_owned(),
            key: ModeKey::Yolo,
            token,
            result: Ok(Some(value.to_owned())),
        }),
    );
}

fn yolo_row(state: &AppState) -> Option<String> {
    rows(&state.automation, 0)
        .into_iter()
        .find(|row| row.label == "⚠ YOLO")
        .map(|row| row.detail)
}

/// An open session with the bypass read off, the approval mode manual and every yolo read
/// answered off.
fn opened_with_bypass_off() -> AppState {
    let mut state = AppState::new("yolo-sync");
    let mut reads = yolo_reads(&update(
        &mut state,
        AppMessage::Opened {
            opened: opened(),
            notice: None,
        },
    ));
    // Replies for any generation but the pending one drop silently.
    for generation in 0..4 {
        update(
            &mut state,
            AppMessage::Permissions(PermissionsInput::ProfileModeRead {
                generation,
                result: Ok(ApprovalMode::Manual),
            }),
        );
        reads.extend(yolo_reads(&update(
            &mut state,
            AppMessage::Permissions(PermissionsInput::SessionOffRead {
                session_id: SESSION.to_owned(),
                generation,
                result: Ok(false),
            }),
        )));
    }
    let (_, token) = reads.last().cloned().expect("the yolo mode is read");
    answer_yolo(&mut state, token, "0");
    assert_eq!(state.automation.modes().0, ModeValue::Known(false));
    assert_eq!(yolo_row(&state), None);
    state
}

#[test]
fn bypass_write_refreshes_the_yolo_row() {
    let mut state = opened_with_bypass_off();
    let effects = update(
        &mut state,
        AppMessage::User(UserAction::ApprovalsToggle {
            context: ToggleContext::Composer,
        }),
    );
    let (generation, off) = effects
        .iter()
        .find_map(|effect| match effect {
            AppEffect::SetSessionOff {
                generation, off, ..
            } => Some((*generation, *off)),
            _ => None,
        })
        .expect("Shift+Tab writes the bypass");
    assert!(off);

    let effects = update(
        &mut state,
        AppMessage::Permissions(PermissionsInput::SessionOffSet {
            session_id: SESSION.to_owned(),
            generation,
            sent: true,
            result: Ok(true),
        }),
    );
    let reads = yolo_reads(&effects);
    assert_eq!(reads.len(), 1);
    assert_eq!(reads[0].0, SESSION);
    answer_yolo(&mut state, reads[0].1, "1");

    assert_eq!(
        state
            .view()
            .approvals
            .map(|segment| segment.text)
            .as_deref(),
        Some("approvals: off (this session)")
    );
    assert_eq!(
        yolo_row(&state).as_deref(),
        Some("commands run without approval in this session")
    );
}

#[test]
fn bypass_read_refreshes_the_yolo_row_and_an_unsent_call_does_not() {
    let mut state = opened_with_bypass_off();
    let read = PermissionsInput::SessionOffRead {
        session_id: SESSION.to_owned(),
        generation: 0,
        result: Ok(true),
    };
    assert_eq!(
        yolo_reads(&update(&mut state, AppMessage::Permissions(read))).len(),
        1
    );

    for input in [
        PermissionsInput::SessionOffSet {
            session_id: SESSION.to_owned(),
            generation: 0,
            sent: true,
            result: Err(CallFailure::NotSent),
        },
        PermissionsInput::SessionOffRead {
            session_id: SESSION.to_owned(),
            generation: 0,
            result: Err(CallFailure::NotSent),
        },
    ] {
        let effects = update(&mut state, AppMessage::Permissions(input));
        assert!(yolo_reads(&effects).is_empty());
    }
}
