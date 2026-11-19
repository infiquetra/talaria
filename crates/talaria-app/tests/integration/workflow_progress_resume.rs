//! A workflow resume may bind the attempt's stored id to its runtime id, and nothing else.

use std::time::Duration;

use serde_json::{Value, json};
use talaria_app::app::AppMessage;
use talaria_app::workflow_progress::ProgressService;
use talaria_workflow::journal::{NewManifest, OnUnknown, Record, RecordLine, RunDir};
use talaria_workflow::progress::RunProgress;
use talaria_workflow::progress::live::{self, ProgressEffect, ProgressInput, ProgressState};
use tokio::sync::mpsc::unbounded_channel;
use tokio::time::timeout;

use crate::lines;
use crate::mock_gateway::MockGateway;

fn journal(runtime: &str, stored: &str) -> Vec<RecordLine> {
    let mut records = lines::live_running(lines::KEY_RUNNING, runtime, 4);
    for line in &mut records {
        if let Record::SessionBound {
            stored_session_id, ..
        } = &mut line.record
        {
            *stored_session_id = stored.to_string();
        }
    }
    records
}

fn resume_as(session_id: &str, stored: &str) -> Value {
    let mut snapshot = crate::mock_gateway::recorded_resume_snapshot();
    snapshot["session_id"] = json!(session_id);
    snapshot["stored_session_id"] = json!(stored);
    snapshot
}

async fn open_stored(runtime: &str, stored: &str, snapshot: Value) -> ProgressState {
    let (mock, client, _inbound) = MockGateway::start().await;
    mock.queue_resume_snapshot(snapshot);
    let temp = tempfile::tempdir().expect("run directory");
    let run = RunDir::create(
        temp.path(),
        NewManifest {
            workflow_name: "resume-check".into(),
            args: None,
            started_at_ms: 1,
            seed: 1,
            profile: "fixture".into(),
            on_unknown: OnUnknown::Wait,
            talaria_version: "dev".into(),
            hermes: json!({}),
            roster: json!({}),
            approval_policy: json!({}),
        },
        "agent running\n",
    )
    .expect("create run");
    let mut state = ProgressState::new(RunProgress::from_records(
        &lines::manifest(),
        &journal(runtime, stored),
    ));
    let mut service = ProgressService::new(run);
    let (sender, mut receiver) = unbounded_channel();
    service.set_connection(Some(client), sender);
    service
        .execute(
            &mut state,
            vec![ProgressEffect::OpenSession {
                session_id: stored.into(),
            }],
        )
        .await
        .expect("open starts");
    let mut first = true;
    loop {
        let wait = if first {
            Duration::from_secs(15)
        } else {
            Duration::from_millis(300)
        };
        match timeout(wait, receiver.recv()).await {
            Ok(Some(AppMessage::WorkflowProgress { input, .. })) => {
                first = false;
                let opened = matches!(input, ProgressInput::Opened(_));
                let follow = live::update(&mut state, input);
                if !follow.is_empty() {
                    service.execute(&mut state, follow).await.expect("follow");
                }
                if opened {
                    break;
                }
            }
            _ => break,
        }
    }
    drop(temp);
    state
}

#[tokio::test]
async fn open_agent_rejects_a_snapshot_for_another_session() {
    let state = open_stored("runtime-a", "stored-a", resume_as("session-b", "stored-a")).await;
    assert!(
        state.opened_session.is_none(),
        "a substituted runtime id is not the switch target"
    );
    assert_eq!(
        state
            .progress
            .agent(lines::KEY_RUNNING)
            .and_then(|agent| agent.session_id.as_deref()),
        Some("runtime-a"),
        "a substituted snapshot does not rebind the attempt"
    );
}

#[tokio::test]
async fn open_agent_accepts_the_attempts_runtime_id() {
    let state = open_stored("runtime-a", "stored-a", resume_as("runtime-a", "stored-a")).await;
    assert_eq!(state.opened_session.as_deref(), Some("runtime-a"));
}

#[tokio::test]
async fn open_agent_rejects_an_unknown_runtime_id_when_ids_coincide() {
    let state = open_stored(
        "session-a",
        "session-a",
        resume_as("session-b", "session-a"),
    )
    .await;
    assert!(state.opened_session.is_none());
    assert_eq!(
        state
            .progress
            .agent(lines::KEY_RUNNING)
            .and_then(|agent| agent.session_id.as_deref()),
        Some("session-a")
    );
}

#[tokio::test]
async fn open_agent_accepts_a_matching_snapshot_when_ids_coincide() {
    let state = open_stored(
        "session-a",
        "session-a",
        resume_as("session-a", "session-a"),
    )
    .await;
    assert_eq!(state.opened_session.as_deref(), Some("session-a"));
}
