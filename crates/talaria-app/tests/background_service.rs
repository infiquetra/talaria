//! Service acceptance checks reuse the original independently authored suite.

#[path = "background_wire.rs"]
mod wire;

use hermes_client::liveness::{LinkEvent, LinkState};
use talaria_app::app::{AppEffect, AppMessage, AppState, UserAction, update};
use talaria_app::sessions::{OpenError, OpenedSession, SessionInfoSummary};
use talaria_core::background::{BackgroundEffect, BackgroundEvent, BgState};
use talaria_core::session::{SessionId, SessionState};

#[test]
fn unknown_output_uses_current_app_time_without_a_frame_tick() {
    use hermes_client::Inbound;
    use hermes_client::channel::{Generation, InboundEvent};
    use hermes_contract::events::EventFrame;
    use serde_json::json;

    let mut app = AppState::new("launch-background-clock");
    let effects = update(
        &mut app,
        AppMessage::Opened {
            opened: opened_fixture(),
            notice: None,
        },
    );
    let list_request = |effects: &[AppEffect]| {
        effects.iter().find_map(|effect| match effect {
            AppEffect::Background {
                effect: BackgroundEffect::ListProcesses { request },
                ..
            } => Some(*request),
            _ => None,
        })
    };
    let mut request = list_request(&effects).expect("opening lists once");
    for (sequence, now_ms) in [(1, 1_100), (2, 2_200)] {
        let _ = update(
            &mut app,
            AppMessage::BackgroundDone {
                session_id: SessionId::new("session-background-safety"),
                generation: 1,
                event: BackgroundEvent::Listed {
                    request,
                    processes: Vec::new(),
                    now_ms: now_ms - 1_100,
                },
            },
        );
        app.now_ms = now_ms;
        let raw = json!({
            "session_id": "session-background-safety",
            "replay_epoch": "epoch-background-safety",
            "seq": sequence,
            "type": "agent.terminal.output",
            "payload": {"process_id": "proc_new", "chunk": "tick\n"}
        });
        let effects = update(
            &mut app,
            AppMessage::Link(LinkEvent::Inbound {
                item: Inbound::Event(InboundEvent {
                    generation: Generation(1),
                    frame: EventFrame::from_params(&raw).expect("output frame"),
                    raw,
                }),
                replayed: false,
            }),
        );
        request = list_request(&effects).expect("a later unknown chunk can list again");
        assert!(
            !effects
                .iter()
                .any(|effect| matches!(effect, AppEffect::Redraw)),
            "closed-overlay chunks must not draw at event frequency"
        );
    }
}

fn opened_fixture() -> OpenedSession {
    let id = "session-background-safety";
    OpenedSession {
        state: SessionState::empty(SessionId::new(id)),
        session_id: id.into(),
        stored_session_id: id.into(),
        subscription_id: "subscription-background-safety".into(),
        info: SessionInfoSummary {
            model: None,
            provider: None,
            cwd: None,
            branch: None,
        },
        title: None,
        cursor: hermes_client::replay::Cursor {
            replay_epoch: "epoch-background-safety".into(),
            last_sequence: 0,
        },
        revision: 0,
        execution_generation: 0,
        launch_notices: Vec::new(),
        effort: None,
    }
}

#[test]
fn a_missing_recovery_closes_both_process_overlay_states() {
    let mut app = AppState::new("launch-background-safety");
    let _ = update(
        &mut app,
        AppMessage::Opened {
            opened: opened_fixture(),
            notice: None,
        },
    );
    let _ = update(
        &mut app,
        AppMessage::User(UserAction::Submit("/processes".into())),
    );
    assert!(
        app.processes_open,
        "the process overlay was actually opened"
    );
    assert!(
        app.opened
            .as_ref()
            .expect("open session")
            .state
            .background()
            .overlay_open()
    );

    let _ = update(
        &mut app,
        AppMessage::BackgroundRecovered {
            session_id: "session-background-safety".into(),
            result: Err(OpenError::NotFound),
        },
    );
    assert!(app.session_gone);
    assert!(!app.processes_open);
    assert!(
        !app.opened
            .as_ref()
            .expect("retained gone session")
            .state
            .background()
            .overlay_open()
    );
}

#[test]
fn a_background_reply_from_an_older_connection_is_dropped() {
    let mut app = AppState::new("launch-background-safety");
    let _ = update(
        &mut app,
        AppMessage::Opened {
            opened: opened_fixture(),
            notice: None,
        },
    );
    let _ = update(
        &mut app,
        AppMessage::Link(LinkEvent::State(LinkState::Connected { generation: 2 })),
    );
    let session_id = SessionId::new("session-background-safety");
    let effects = update(
        &mut app,
        AppMessage::BackgroundDone {
            session_id: session_id.clone(),
            generation: 2,
            event: BackgroundEvent::BgCommand {
                text: "Reply with exactly: bg-ok".into(),
                now_ms: 1,
            },
        },
    );
    let request = effects
        .iter()
        .find_map(|effect| match effect {
            AppEffect::Background {
                effect: BackgroundEffect::StartBackground { request, .. },
                ..
            } => Some(*request),
            _ => None,
        })
        .expect("the current connection starts a real background request");
    let before = app
        .opened
        .as_ref()
        .expect("open session")
        .state
        .background()
        .bg_tasks()
        .to_vec();
    assert!(matches!(before[0].state, BgState::Starting));
    let cells_before = app
        .opened
        .as_ref()
        .expect("open session")
        .state
        .cells()
        .len();
    let event = BackgroundEvent::BgAccepted {
        request,
        task_id: "bg_safety".into(),
        now_ms: 2,
    };
    let stale = update(
        &mut app,
        AppMessage::BackgroundDone {
            session_id: session_id.clone(),
            generation: 1,
            event: event.clone(),
        },
    );
    assert!(
        stale.is_empty(),
        "the stale reply cannot draw or schedule a call"
    );
    let state = &app.opened.as_ref().expect("open session").state;
    assert_eq!(state.background().bg_tasks(), before);
    assert_eq!(state.cells().len(), cells_before);

    let _ = update(
        &mut app,
        AppMessage::BackgroundDone {
            session_id,
            generation: 2,
            event,
        },
    );
    let state = &app.opened.as_ref().expect("open session").state;
    assert!(
        matches!(&state.background().bg_tasks()[0].state, BgState::Running { task_id, .. } if task_id == "bg_safety")
    );
    assert!(
        state.cells().len() > cells_before,
        "the same reply on the current connection is applied"
    );
}
