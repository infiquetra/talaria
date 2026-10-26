//! Red-run tests 14 to 20 (#231): renames and branches.

#[path = "support/directory_drive.rs"]
mod drive;
#[path = "support/mock_gateway.rs"]
mod mock_gateway;

use drive::{
    committed_text, locked, resume_snapshot, settle_short, snapshot_of, sorted_keys,
    state_with_opened,
};
use hermes_client::liveness::LinkEvent;
use hermes_client::{Decoded, Generation, Inbound, decode_frame};
use serde_json::json;
use talaria_app::app::{AppMessage, AppState, update};
use talaria_app::directory::DirectoryService;
use talaria_app::launch::StartMode;
use talaria_app::sessions::OpenedSession;
use talaria_app::switch::{BranchOutcome, branch_foreground, branch_request};
use talaria_core::directory::rows::RowKey;
use talaria_core::mutation::MutationLedger;

fn picker_key(profile: &str, session_id: &str) -> RowKey {
    RowKey {
        profile: profile.to_owned(),
        session_id: session_id.to_owned(),
    }
}

fn session_ids(frames: &[mock_gateway::Seen]) -> Vec<String> {
    frames
        .iter()
        .filter_map(|frame| frame.params["session_id"].as_str().map(str::to_owned))
        .collect()
}

#[tokio::test]
async fn renames_send_revision_zero_and_adopt_the_receipt() {
    // A picker row over the side connection.
    let home = tempfile::tempdir().expect("a home");
    let gateway = mock_gateway::StandaloneGateway::start(home.path());
    gateway.set_list_rows_for(
        "default",
        "live",
        vec![mock_gateway::list_row(
            "s-row",
            "Old title",
            Some(1_792_700_000.0),
            None,
            4,
            false,
        )],
    );
    gateway.set_mutation_state("s-row", 5, 2, "Old title", false, 4);

    let mut service = DirectoryService::new();
    service
        .open(home.path(), "fixture")
        .await
        .expect("the side connection opens");
    let mut ledger = MutationLedger::new("launch-1");
    let renamed = service
        .rename(&mut ledger, &picker_key("default", "s-row"), "New title")
        .await;
    assert_eq!(
        renamed,
        talaria_app::directory::RenameResult::Renamed {
            title: "New title".to_owned(),
            revision: Some(6),
        }
    );
    let frames = gateway.requests("session.mutate");
    assert_eq!(
        frames.len(),
        2,
        "the seed conflicts once, then the retry commits"
    );
    assert_eq!(
        sorted_keys(&frames[0].params),
        [
            "expected_revision",
            "operation",
            "payload",
            "profile",
            "request_id",
            "session_id"
        ]
    );
    assert_eq!(frames[0].params["operation"], json!("rename"));
    assert_eq!(frames[0].params["payload"], json!({"title": "New title"}));
    assert_eq!(frames[0].params["expected_revision"], json!(0));
    assert!(frames[0].params.get("expected_generation").is_none());
    assert_eq!(frames[0].params["profile"], json!("default"));
    assert_eq!(frames[1].params["expected_revision"], json!(5));

    // The foreground session over the foreground link, with its tracked fences.
    let rig = drive::Rig::start();
    let folder = rig.folder.to_string_lossy().to_string();
    let mut snapshot = resume_snapshot(&folder);
    snapshot["info"]["title"] = json!("Launch title");
    snapshot["revision"] = json!(7);
    snapshot["execution_generation"] = json!(3);
    rig.gateway.set_resume_snapshot(snapshot);
    rig.gateway
        .set_mutation_state("local-session-1", 7, 3, "Launch title", false, 4);
    let (_outcome, log) = rig
        .drive(StartMode::New, |driver| async move {
            driver.opened().await;
            driver.submit("/rename Foreground title");
            settle_short().await;
            driver.submit("/rename");
            settle_short().await;
            driver.quit();
            driver.finished().await;
        })
        .await;
    let frames = rig.gateway.requests("session.mutate");
    assert_eq!(
        session_ids(&frames),
        ["local-session-1".to_owned()],
        "one rename on the foreground link"
    );
    assert_eq!(frames[0].params["expected_revision"], json!(7));
    assert!(frames[0].params.get("profile").is_none());
    assert!(frames[0].params.get("expected_generation").is_none());
    let lines = committed_text(&log);
    assert!(
        lines
            .iter()
            .any(|line| line.contains("Renamed to \"Foreground title\".")),
        "the rename notice commits: {lines:?}"
    );
    assert!(
        lines
            .iter()
            .any(|line| line.contains("This session is called \"Foreground title\".")),
        "the title field holds the receipt title: {lines:?}"
    );

    // /rename texts without a call.
    let rig = drive::Rig::start();
    let (_outcome, log) = rig
        .drive(StartMode::New, |driver| async move {
            driver.opened().await;
            driver.submit("/rename   ");
            settle_short().await;
            driver.submit("/rename");
            settle_short().await;
            driver.quit();
            driver.finished().await;
        })
        .await;
    let lines = committed_text(&log);
    assert!(
        lines
            .iter()
            .any(|line| line.contains("Give the session a new title:")),
        "the no-arg text: {lines:?}"
    );
    assert!(rig.gateway.requests("session.mutate").is_empty());
}

#[tokio::test]
async fn rename_conflict_retries_once_then_stops() {
    let home = tempfile::tempdir().expect("a home");
    let gateway = mock_gateway::StandaloneGateway::start(home.path());
    gateway.set_mutation_state("s-churn", 5, 2, "Old", false, 4);
    gateway.set_live_revision("s-churn", true);

    let mut service = DirectoryService::new();
    service
        .open(home.path(), "fixture")
        .await
        .expect("the side connection opens");
    let mut ledger = MutationLedger::new("launch-1");
    let renamed = service
        .rename(&mut ledger, &picker_key("fixture", "s-churn"), "New")
        .await;
    assert_eq!(
        renamed,
        talaria_app::directory::RenameResult::Failed {
            text: "Hermes refused the rename: the session changed while the rename was being prepared. Try again.".to_owned(),
        }
    );
    let frames = gateway.requests("session.mutate");
    assert_eq!(frames.len(), 2, "one retry, then it stops");
    assert_ne!(
        frames[0].params["request_id"], frames[1].params["request_id"],
        "the retry carries a new request id"
    );
    assert_eq!(frames[1].params["expected_revision"], json!(6));
}

#[tokio::test]
async fn unknown_rename_resends_on_a_new_connection() {
    let home = tempfile::tempdir().expect("a home");
    let gateway = mock_gateway::StandaloneGateway::start(home.path());
    // The row is untracked, so the rename is prepared against revision zero; Hermes holds
    // that revision, so the first frame commits before the connection drops.
    gateway.set_mutation_state("s-drop", 0, 2, "Old", false, 4);
    gateway.queue_behavior("session.mutate", mock_gateway::Behavior::DropBeforeReply);

    let mut service = DirectoryService::new();
    service
        .open(home.path(), "fixture")
        .await
        .expect("the side connection opens");
    let tickets_before = gateway.tickets_issued();
    let mut ledger = MutationLedger::new("launch-1");
    let key = picker_key("fixture", "s-drop");
    let first = service.rename(&mut ledger, &key, "New").await;
    assert!(
        matches!(first, talaria_app::directory::RenameResult::Failed { .. }),
        "the dropped call fails unknown"
    );
    let second = service.rename(&mut ledger, &key, "New").await;
    assert!(
        matches!(second, talaria_app::directory::RenameResult::Renamed { .. }),
        "the resend commits"
    );
    assert_eq!(
        gateway.tickets_issued(),
        tickets_before + 1,
        "the resend opens a new side connection"
    );
    let frames = gateway.requests("session.mutate");
    assert_eq!(frames.len(), 2);
    assert_eq!(frames[0].params, frames[1].params);

    let third = service.rename(&mut ledger, &key, "Other").await;
    assert!(
        matches!(third, talaria_app::directory::RenameResult::Renamed { .. }),
        "a new title commits"
    );
    let frames = gateway.requests("session.mutate");
    assert_eq!(frames.len(), 3);
    assert_ne!(
        frames[1].params["request_id"], frames[2].params["request_id"],
        "a new title is a new request"
    );
}

#[tokio::test]
async fn taken_title_is_a_gateway_error() {
    let home = tempfile::tempdir().expect("a home");
    let gateway = mock_gateway::StandaloneGateway::start(home.path());
    gateway.set_mutation_state("s-a", 1, 1, "Taken", false, 4);
    gateway.set_mutation_state("s-b", 1, 1, "Mine", false, 4);

    let mut service = DirectoryService::new();
    service
        .open(home.path(), "fixture")
        .await
        .expect("the side connection opens");
    let mut ledger = MutationLedger::new("launch-1");
    let renamed = service
        .rename(&mut ledger, &picker_key("fixture", "s-b"), "Taken")
        .await;
    assert_eq!(
        renamed,
        talaria_app::directory::RenameResult::Failed {
            text: "Hermes could not rename the session: another session already has this title."
                .to_owned(),
        }
    );
    assert_eq!(gateway.mutate_title("s-b").as_deref(), Some("Mine"));
}

#[tokio::test]
async fn branch_switches_into_the_child_and_adopts_its_fences() {
    let rig = drive::Rig::start();
    let folder = rig.folder.to_string_lossy().to_string();
    rig.gateway
        .set_mutation_state("local-session-1", 7, 3, "Launch title", false, 9);
    let mut child = resume_snapshot(&folder);
    child["session_id"] = json!("child-1");
    child["revision"] = json!(8);
    child["execution_generation"] = json!(3);
    child["info"]["title"] = json!("Launch title #2");
    rig.gateway.queue_resume_snapshot(child);

    let (_outcome, log) = rig
        .drive(StartMode::New, |driver| async move {
            driver.opened().await;
            driver.submit("/branch");
            driver.line("Branched into").await;
            driver.submit("/rename After the branch");
            driver.line("Renamed to").await;
            driver.quit();
            driver.finished().await;
        })
        .await;

    let mutate = rig.gateway.requests("session.mutate");
    assert_eq!(mutate.len(), 2);
    assert_eq!(mutate[0].params["operation"], json!("branch"));
    assert_eq!(mutate[0].params["expected_revision"], json!(7));
    assert_eq!(mutate[0].params["expected_generation"], json!(3));
    // The later rename sends the child's snapshot revision.
    assert_eq!(mutate[1].params["operation"], json!("rename"));
    assert_eq!(mutate[1].params["session_id"], json!("child-1"));
    assert_eq!(mutate[1].params["expected_revision"], json!(8));

    let seen = rig.gateway.seen();
    let at = |method: &str, nth: usize| {
        seen.iter()
            .filter(|frame| frame.method.as_deref() == Some(method))
            .nth(nth)
            .unwrap_or_else(|| panic!("{method} frame {nth}"))
            .at
    };
    assert!(
        at("session.mutate", 0) < at("session.resume", 0),
        "the branch receipt comes before the child resume"
    );
    assert!(
        at("session.resume", 0) < at("session.detach", 0),
        "the child opens before the parent is let go"
    );
    let resumes = rig.gateway.requests("session.resume");
    assert_eq!(resumes.len(), 1);
    assert_eq!(resumes[0].params["session_id"], json!("child-1"));
    assert!(
        resumes[0].params.get("display_projection").is_some(),
        "the child resume carries the projection"
    );
    let attached = rig.gateway.attaches();
    let child_attach = attached
        .iter()
        .find(|attach| attach.session_id == "child-1")
        .expect("the child attaches");
    assert_eq!(child_attach.subscription_id, "sub-branch-1");
    // Quitting detaches the child afterwards; the parent is let go once, by the branch.
    let detaches: Vec<_> = rig
        .gateway
        .requests("session.detach")
        .into_iter()
        .filter(|detach| detach.params["session_id"] == json!("local-session-1"))
        .collect();
    assert_eq!(detaches.len(), 1);
    assert!(detaches[0].params.get("end_reason").is_none());

    let lines = committed_text(&log);
    assert!(
        lines.iter().any(|line| line
            == "Branched into \"Launch title #2\" with 9 earlier messages. The child continues from here."),
        "the branch notice: {lines:?}"
    );
    assert!(
        lines
            .iter()
            .any(|line| line.contains("Resumed: Launch title #2")),
        "the child separator: {lines:?}"
    );
}

#[tokio::test]
async fn branch_refusals_keep_the_parent() {
    // A busy session, then a busy compressor, then an unknown turn.
    for (reason, field, text) in [
        (
            "session_busy",
            None,
            "Hermes cannot branch while the agent is working. Wait for the reply to finish, or press Esc to stop it, then run /branch again.",
        ),
        (
            "session_busy",
            Some("compression"),
            "Hermes is already compacting this conversation. Wait for it to finish, then run /branch again.",
        ),
        (
            "unknown_execution",
            None,
            "Hermes cannot branch while the outcome of the last turn is unknown. Nothing was changed.",
        ),
    ] {
        let rig = drive::Rig::start();
        rig.gateway
            .set_mutation_state("local-session-1", 7, 3, "Launch title", false, 9);
        let mut data = json!({});
        if let Some(field) = field {
            data["field"] = json!(field);
        }
        rig.gateway
            .queue_mutate_refusal("local-session-1", reason, data);
        let gateway = &rig.gateway;
        let (_outcome, log) = rig
            .drive(StartMode::New, |driver| async move {
                driver.opened().await;
                driver.submit("/branch");
                settle_short().await;
                // Quitting detaches the session in front, so the check comes first.
                assert!(
                    gateway.requests("session.detach").is_empty(),
                    "a refused branch detaches nothing"
                );
                driver.quit();
                driver.finished().await;
            })
            .await;
        let lines = committed_text(&log);
        assert!(
            lines.iter().any(|line| line == text),
            "the refusal text for {reason}: {lines:?}"
        );
        assert!(
            rig.gateway.requests("session.resume").len() <= 1,
            "no child is opened"
        );
    }

    // A conflict with a moved generation stops at once.
    let rig = drive::Rig::start();
    rig.gateway
        .set_mutation_state("local-session-1", 7, 3, "Launch title", false, 9);
    rig.gateway.queue_mutate_refusal(
        "local-session-1",
        "revision_conflict",
        json!({"revision": 8, "execution_generation": 4}),
    );
    let (_outcome, log) = rig
        .drive(StartMode::New, |driver| async move {
            driver.opened().await;
            driver.submit("/branch");
            settle_short().await;
            driver.quit();
            driver.finished().await;
        })
        .await;
    assert_eq!(rig.gateway.requests("session.mutate").len(), 1);
    let lines = committed_text(&log);
    assert!(
        lines.iter().any(|line| line
            == "The session changed while the branch was being prepared. Run /branch again."),
        "the conflict text: {lines:?}"
    );

    // The branch commits but the child will not open.
    let rig = drive::Rig::start();
    rig.gateway
        .set_mutation_state("local-session-1", 7, 3, "Launch title", false, 9);
    rig.gateway.queue_behavior(
        "session.resume",
        mock_gateway::Behavior::Refuse("not_found".to_owned()),
    );
    let gateway = &rig.gateway;
    let (_outcome, log) = rig
        .drive(StartMode::New, |driver| async move {
            driver.opened().await;
            driver.submit("/branch");
            settle_short().await;
            // Quitting detaches the session in front, so the check comes first.
            assert!(
                gateway.requests("session.detach").is_empty(),
                "the parent stays attached when the child does not open"
            );
            driver.quit();
            driver.finished().await;
        })
        .await;
    let lines = committed_text(&log);
    assert!(
        lines.iter().any(|line| line.contains("Branch made")
            && line.contains("child-1")
            && line.contains("Resume it from the picker when Hermes is reachable again.")),
        "the committed-child text: {lines:?}"
    );
    let session = locked(&log).session.clone();
    assert_eq!(session.as_deref(), Some("local-session-1"));
}

#[tokio::test]
async fn branch_refuses_a_running_turn() {
    let running = mock_gateway::data_snapshot("running_turn.json");
    let opened = OpenedSession::from_snapshot(snapshot_of(&running));
    let mut app = AppState::new("launch-1");
    state_with_opened(&mut app, opened);

    let outcome = branch_foreground(&mut app, None).await;
    assert_eq!(
        outcome,
        BranchOutcome::Refused {
            text: "Hermes cannot branch while the agent is working. Wait for the reply to finish, or press Esc to stop it, then run /branch again.".to_owned(),
        }
    );
}

/// A turn raises the session's revision and generation, and the gateway
/// reports the new fences on the session's own `session.info`. The next
/// mutation must present them, or the gateway refuses it as a fence
/// conflict: the branch after a finished turn is refused otherwise.
#[tokio::test]
async fn a_turns_new_fences_reach_the_next_mutation() {
    // The session as it opens: idle, at revision 1 and generation 1.
    let mut snapshot = mock_gateway::data_snapshot("running_turn.json");
    snapshot["running"] = json!(false);
    snapshot["revision"] = json!(1);
    snapshot["execution_generation"] = json!(1);
    let opened = OpenedSession::from_snapshot(snapshot_of(&snapshot));
    let mut app = AppState::new("launch-1");
    state_with_opened(&mut app, opened);
    let session_id = app
        .opened
        .as_ref()
        .expect("the session is open")
        .session_id
        .clone();

    // The session's revision and generation as the session opened with them.
    let before = branch_request(&mut app, None).expect("the branch is prepared");
    assert_eq!(before.expected_revision, 1);
    assert_eq!(before.expected_generation, Some(1));

    // The turn finished, and the gateway reported the fences it left behind.
    update(
        &mut app,
        AppMessage::Link(session_info_event(
            &session_id,
            json!({"revision": 2, "execution_generation": 2}),
        )),
    );

    let after = branch_request(&mut app, None).expect("the branch is prepared");
    assert_eq!(
        after.expected_revision, 2,
        "the branch presents the revision the turn left behind"
    );
    assert_eq!(
        after.expected_generation,
        Some(2),
        "the branch presents the generation the turn left behind"
    );
}

/// A `session.info` event as the link delivers it.
fn session_info_event(session_id: &str, payload: serde_json::Value) -> LinkEvent {
    let frame = json!({
        "jsonrpc": "2.0",
        "method": "event",
        "params": {
            "type": "session.info",
            "session_id": session_id,
            "replay_epoch": "id-0002",
            "seq": 9,
            "payload": payload,
        }
    });
    let bytes = serde_json::to_vec(&frame).expect("event frame");
    let decoded = decode_frame(Generation(1), &bytes, &mut |_id| unreachable!("event"));
    let Decoded::Inbound(item) = decoded else {
        panic!("session.info decodes as an event");
    };
    assert!(matches!(item, Inbound::Event(_)));
    LinkEvent::Inbound {
        item,
        replayed: false,
    }
}
