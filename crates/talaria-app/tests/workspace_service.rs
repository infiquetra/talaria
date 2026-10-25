//! Test 20 (#237): the workspace service checks folders and moves sessions.

#[path = "support/mock_gateway.rs"]
mod mock_gateway;

use mock_gateway::{Behavior, MockGateway, next_event};
use serde_json::{Map, Value, json};
use talaria_app::mutation::MutationClient;
use talaria_app::workspace::WorkspaceService;
use talaria_core::mutation::{MutationOutcome, MutationRequest};
use talaria_core::workspace::{CdEffect, CdInput, FolderCheck};
use tempfile::TempDir;

const SID: &str = "local-session-1";

fn request(request_id: &str, revision: i64, generation: i64, cwd: &str) -> MutationRequest {
    let mut payload = Map::new();
    payload.insert("cwd".to_owned(), Value::String(cwd.to_owned()));
    MutationRequest {
        session_id: SID.to_owned(),
        request_id: request_id.to_owned(),
        expected_revision: revision,
        expected_generation: Some(generation),
        operation: "workspace".to_owned(),
        payload,
    }
}

fn outcome_of(input: Option<CdInput>) -> (MutationRequest, MutationOutcome) {
    match input {
        Some(CdInput::Outcome { request, outcome }) => (request, outcome),
        other => panic!("a send feeds back an outcome, got {other:?}"),
    }
}

#[tokio::test]
async fn move_round_trips() {
    let home = TempDir::new().expect("a home folder");
    let from = TempDir::new().expect("a starting folder");
    let to = TempDir::new().expect("a target folder");
    let from = from.path().to_string_lossy().into_owned();
    let to = to.path().to_string_lossy().into_owned();
    let _ = home;

    // The folder check runs on real folders.
    let (_mock, client, _inbound) = MockGateway::start().await;
    let service = WorkspaceService::new(MutationClient::new(client));
    let input = service
        .execute(CdEffect::CheckFolder {
            session_id: SID.to_owned(),
            target: from.clone().into(),
        })
        .await;
    assert_eq!(
        input,
        Some(CdInput::Checked {
            session_id: SID.to_owned(),
            target: from.clone().into(),
            check: FolderCheck::Directory,
        })
    );
    let missing = format!("{from}-missing");
    let input = service
        .execute(CdEffect::CheckFolder {
            session_id: SID.to_owned(),
            target: missing.clone().into(),
        })
        .await;
    assert_eq!(
        input,
        Some(CdInput::Checked {
            session_id: SID.to_owned(),
            target: missing.into(),
            check: FolderCheck::Missing,
        })
    );
    let file = tempfile::NamedTempFile::new().expect("a file");
    let input = service
        .execute(CdEffect::CheckFolder {
            session_id: SID.to_owned(),
            target: file.path().to_path_buf(),
        })
        .await;
    assert!(matches!(
        input,
        Some(CdInput::Checked {
            check: FolderCheck::NotADirectory,
            ..
        })
    ));

    // A move sends the exact wire params, commits, and pushes both events.
    let (mock, client, mut inbound) = MockGateway::start().await;
    mock.set_mutate_folders(&[&from, &to]);
    mock.set_mutate_branch(Some("main"));
    let service = WorkspaceService::new(MutationClient::new(client));
    let sent = request("testlaunch-cd-m1", 1, 1, &to);
    let input = service.execute(CdEffect::Send(sent.clone())).await;
    let (answered, outcome) = outcome_of(input);
    assert_eq!(answered, sent);
    let frames = mock.requests("session.mutate");
    assert_eq!(frames.len(), 1);
    assert_eq!(
        frames[0].params,
        json!({
            "session_id": SID,
            "request_id": "testlaunch-cd-m1",
            "expected_revision": 1,
            "expected_generation": 1,
            "operation": "workspace",
            "payload": {"cwd": to},
        })
    );
    match outcome {
        MutationOutcome::Committed(result) => {
            assert_eq!(result["session_id"], SID);
            assert_eq!(result["revision"], 2);
            assert_eq!(result["execution_generation"], 2);
            assert_eq!(result["operation"], "workspace");
            assert_eq!(result["cwd"], to);
            assert_eq!(result["branch"], "main");
        }
        other => panic!("a move commits, got {other:?}"),
    }
    let info = next_event(&mut inbound).await;
    assert_eq!(info.raw["type"], "session.info");
    assert_eq!(info.raw["session_id"], SID);
    assert_eq!(info.raw["payload"]["cwd"], to);
    let updated = next_event(&mut inbound).await;
    assert_eq!(updated.raw["type"], "session.updated");
    assert_eq!(updated.raw["session_id"], SID);
    assert_eq!(updated.raw["payload"]["cwd"], to);
    assert!(
        mock.seen()
            .iter()
            .all(|frame| frame.method.as_deref() != Some("session.cwd.set")),
        "no raw move is ever sent"
    );

    // A conflict carries fresh fences; the retry uses them with a new id.
    let (mock, client, _inbound) = MockGateway::start().await;
    mock.set_mutate_folders(&[&from, &to]);
    mock.set_mutate_branch(None);
    mock.set_mutate_conflict_once(14, 5);
    let service = WorkspaceService::new(MutationClient::new(client));
    let sent = request("testlaunch-cd-m1", 1, 1, &to);
    let input = service.execute(CdEffect::Send(sent.clone())).await;
    let (_, outcome) = outcome_of(input);
    assert_eq!(
        outcome,
        MutationOutcome::FenceRefused {
            reason: "revision_conflict".to_owned(),
            revision: Some(14),
            execution_generation: Some(5),
        }
    );
    let retry = request("testlaunch-cd-m2", 14, 5, &to);
    let input = service.execute(CdEffect::Send(retry.clone())).await;
    let (_, outcome) = outcome_of(input);
    match outcome {
        MutationOutcome::Committed(result) => {
            assert_eq!(result["revision"], 15);
            assert_eq!(result["execution_generation"], 6);
            assert!(result["branch"].is_null());
        }
        other => panic!("the retry commits, got {other:?}"),
    }
    let frames = mock.requests("session.mutate");
    assert_eq!(frames.len(), 2);
    assert_eq!(frames[1].params["expected_revision"], 14);
    assert_eq!(frames[1].params["expected_generation"], 5);
    assert_eq!(frames[1].params["request_id"], "testlaunch-cd-m2");

    // A dropped connection loses the answer; the repeated move resends the
    // identical request and gets the stored receipt.
    let (mock, client, _inbound) = MockGateway::start().await;
    mock.set_mutate_folders(&[&from, &to]);
    let service = WorkspaceService::new(MutationClient::new(client.clone()));
    mock.set_behavior("session.mutate", Behavior::DropBeforeReply);
    let sent = request("testlaunch-cd-m1", 1, 1, &to);
    let input = service.execute(CdEffect::Send(sent.clone())).await;
    let (_, outcome) = outcome_of(input);
    assert_eq!(outcome, MutationOutcome::Unknown);
    mock.replace_connection(&client).await;
    mock.set_behavior("session.mutate", Behavior::Normal);
    let input = service.execute(CdEffect::Send(sent.clone())).await;
    let (_, outcome) = outcome_of(input);
    match outcome {
        MutationOutcome::Committed(result) => {
            assert_eq!(result["revision"], 2);
            assert_eq!(result["execution_generation"], 2);
            assert_eq!(result["cwd"], to);
        }
        other => panic!("the resend gets the receipt, got {other:?}"),
    }
    let frames = mock.requests("session.mutate");
    assert_eq!(frames.len(), 2);
    assert_eq!(frames[0].params, frames[1].params);

    // Busy, unknown and refused moves map to their refusals.
    for reason in [
        "session_busy",
        "unknown_execution",
        "runtime_coordination_required",
        "permission_denied",
    ] {
        let (mock, client, _inbound) = MockGateway::start().await;
        mock.set_mutate_folders(&[&from, &to]);
        let service = WorkspaceService::new(MutationClient::new(client));
        mock.queue_behavior("session.mutate", Behavior::Refuse(reason.to_owned()));
        let sent = request("testlaunch-cd-m1", 1, 1, &to);
        let input = service.execute(CdEffect::Send(sent.clone())).await;
        let (_, outcome) = outcome_of(input);
        assert_eq!(
            outcome,
            MutationOutcome::Refused {
                reason: reason.to_owned(),
                field: None,
                detail: None,
                rpc_code: 4001,
            },
            "reason {reason}"
        );
        mock.assert_no_violations();
    }

    // An unmarked folder is refused, and every sent cwd is absolute.
    let (mock, client, _inbound) = MockGateway::start().await;
    mock.set_mutate_folders(&[&from]);
    let service = WorkspaceService::new(MutationClient::new(client));
    let sent = request("testlaunch-cd-m1", 1, 1, &to);
    let input = service.execute(CdEffect::Send(sent)).await;
    let (_, outcome) = outcome_of(input);
    assert!(matches!(
        outcome,
        MutationOutcome::Refused { reason, .. } if reason == "invalid_params"
    ));
    for frame in mock.seen() {
        if frame.method.as_deref() == Some("session.mutate") {
            let cwd = frame.params["payload"]["cwd"]
                .as_str()
                .expect("a string cwd");
            assert!(cwd.starts_with('/'), "a relative cwd was sent: {cwd}");
        }
    }

    // Folder updates and notices need no call.
    let input = service
        .execute(CdEffect::UpdateFolder {
            session_id: SID.to_owned(),
            cwd: to.clone(),
            branch: Some("main".to_owned()),
        })
        .await;
    assert_eq!(input, None);
    let input = service
        .execute(CdEffect::Notice {
            kind: talaria_core::session::NoticeKind::System,
            text: "shown".to_owned(),
        })
        .await;
    assert_eq!(input, None);

    mock.assert_no_violations();
}
