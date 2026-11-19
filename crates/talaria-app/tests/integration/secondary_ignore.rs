//! Secondary links leave server requests for the foreground viewer.

use crate::mock_gateway;

use std::time::Duration;

use hermes_client::Inbound;
use hermes_client::liveness::LinkEvent;
use hermes_client::replay::Cursor;
use mock_gateway::{MockGateway, StandaloneGateway};
use serde_json::json;
use talaria_app::app::{AppEffect, AppMessage, AppState, update};
use talaria_app::prompts::PromptService;
use talaria_app::sessions::OpenedSession;
use talaria_app::side_connection::SideConnection;
use talaria_core::session::{SessionId, SessionState};

#[tokio::test]
async fn side_link_ignores_server_requests() {
    let home = tempfile::tempdir().expect("a home");
    let gateway = StandaloneGateway::start(home.path());
    let connection = SideConnection::open(home.path(), "fixture")
        .await
        .expect("the side connection opens");
    let id = gateway.server_request("sudo").await;
    tokio::time::sleep(Duration::from_millis(200)).await;
    let answers = gateway.answers_for(&id);
    assert!(
        answers.is_empty(),
        "the side link wrote an answer: {answers:?}"
    );
    connection.close().await;
    tokio::time::sleep(Duration::from_millis(100)).await;
    let answers = gateway.answers_for(&id);
    assert!(
        answers.is_empty(),
        "closing the side link wrote an answer: {answers:?}"
    );
}

#[tokio::test]
async fn foreground_ignores_monitored_workflow_session_request() {
    let (gateway, _client, mut inbound) = MockGateway::start().await;
    let mut state = AppState::new("launch");
    state.opened = Some(OpenedSession {
        state: SessionState::empty(SessionId::new("operator-session")),
        session_id: "operator-session".into(),
        stored_session_id: "operator-session".into(),
        subscription_id: "subscription".into(),
        title: None,
        revision: 0,
        execution_generation: 0,
        launch_notices: Vec::new(),
        effort: None,
        info: talaria_app::sessions::SessionInfoSummary {
            model: None,
            provider: None,
            cwd: None,
            branch: None,
        },
        cursor: Cursor {
            replay_epoch: "epoch".into(),
            last_sequence: 0,
        },
    });
    let id = gateway
        .server_request_with("approval", json!({"session_id": "workflow-session"}))
        .await;
    let item = tokio::time::timeout(Duration::from_secs(2), inbound.next())
        .await
        .expect("the request arrives")
        .expect("the foreground connection stays open");
    let request = match item {
        Inbound::ServerRequest(request) => request,
        Inbound::Event(_) => panic!("expected a server request, got an event"),
        Inbound::Malformed(_) => panic!("expected a server request, got a malformed frame"),
        Inbound::Closed(_) => panic!("expected a server request, got a close"),
    };
    assert_eq!(request.session_id.as_deref(), Some("workflow-session"));
    let mut prompts = PromptService::new(Box::new(|| "submission".to_owned()));
    for effect in update(
        &mut state,
        AppMessage::Link(LinkEvent::Inbound {
            item: Inbound::ServerRequest(request),
            replayed: false,
        }),
    ) {
        if let AppEffect::ServerRequest(request) = effect {
            let _ = prompts
                .on_server_request_for_session(request, Some("operator-session"))
                .await;
        }
    }
    drop(prompts);
    tokio::time::sleep(Duration::from_millis(50)).await;
    let answers = gateway.answers_for(&id);
    assert!(
        answers.is_empty(),
        "the foreground connection answered a workflow session: {answers:?}"
    );
}
