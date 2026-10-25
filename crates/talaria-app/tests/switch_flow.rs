//! Red-run tests 21 to 25b (#231): switching the foreground session.

#[path = "support/directory_drive.rs"]
mod drive;
#[path = "support/mock_gateway.rs"]
mod mock_gateway;

use drive::{
    committed_text, locked, resume_snapshot, settle_short, snapshot_of, state_with_opened,
};
use serde_json::json;
use talaria_app::app::{AppState, UserAction};
use talaria_app::launch::StartMode;
use talaria_app::sessions::OpenedSession;
use talaria_app::switch::{SwitchOutcome, switch_foreground};
use talaria_core::directory::rows::RowKey;
use talaria_core::directory::state::{PickerAction, PickerInput};

fn target(profile: &str, session_id: &str) -> RowKey {
    RowKey {
        profile: profile.to_owned(),
        session_id: session_id.to_owned(),
    }
}

fn child_snapshot(folder: &str, id: &str, title: &str, revision: i64) -> serde_json::Value {
    let mut snapshot = resume_snapshot(folder);
    snapshot["session_id"] = json!(id);
    snapshot["revision"] = json!(revision);
    snapshot["execution_generation"] = json!(3);
    snapshot["info"]["title"] = json!(title);
    snapshot
}

#[tokio::test]
async fn same_profile_switch_opens_first_then_lets_go() {
    let rig = drive::Rig::start();
    let folder = rig.folder.to_string_lossy().to_string();
    rig.gateway
        .queue_resume_snapshot(child_snapshot(&folder, "s-two", "Second work", 11));
    // Hermes holds the fences of the session it switches to.
    rig.gateway
        .set_mutation_state("s-two", 11, 3, "Second work", false, 2);
    rig.gateway.set_detached(Some(false));

    let gateway = &rig.gateway;
    let (_outcome, log) = rig
        .drive(StartMode::New, |driver| async move {
            driver.opened().await;
            driver.submit("/resume s-two");
            driver.line("Now showing").await;
            driver.submit("/rename Third title");
            driver.line("Renamed to").await;
            // An event for the old session afterwards paints nothing.
            let before = driver.counts();
            gateway
                .send_event_params(json!({
                    "type": "message.start",
                    "session_id": "local-session-1",
                    "execution_generation": 9,
                    "payload": {},
                }))
                .await;
            settle_short().await;
            assert_eq!(driver.counts(), before, "the old event paints nothing");
            driver.quit();
            driver.finished().await;
        })
        .await;

    let seen = rig.gateway.seen();
    let at = |method: &str, nth: usize| {
        seen.iter()
            .filter(|frame| frame.method.as_deref() == Some(method))
            .nth(nth)
            .unwrap_or_else(|| panic!("{method} frame {nth}"))
            .at
    };
    assert!(
        at("session.resume", 0) < at("session.detach", 0),
        "the target opens before the old session is let go"
    );
    let resumes = rig.gateway.requests("session.resume");
    assert_eq!(resumes.len(), 1);
    assert_eq!(resumes[0].params["session_id"], json!("s-two"));
    assert!(
        resumes[0].params.get("display_projection").is_some(),
        "the resume carries the projection"
    );
    // Quitting detaches the new session afterwards; the old one is let go once, by the switch.
    let detaches: Vec<_> = rig
        .gateway
        .requests("session.detach")
        .into_iter()
        .filter(|detach| detach.params["session_id"] == json!("local-session-1"))
        .collect();
    assert_eq!(detaches.len(), 1);
    assert!(detaches[0].params.get("end_reason").is_none());

    // The later rename sends the target's snapshot revision.
    let mutate = rig.gateway.requests("session.mutate");
    assert_eq!(mutate.len(), 1);
    assert_eq!(mutate[0].params["session_id"], json!("s-two"));
    assert_eq!(mutate[0].params["expected_revision"], json!(11));

    let lines = committed_text(&log);
    assert!(
        lines.iter().any(|line| line.contains("Now showing")),
        "the switch notice: {lines:?}"
    );
    assert!(
        lines
            .iter()
            .any(|line| line.contains("Resumed: Second work")),
        "the new separator: {lines:?}"
    );
}

#[tokio::test]
async fn other_profile_switch_opens_a_new_link_first() {
    let rig = drive::Rig::start();
    let folder = rig.folder.to_string_lossy().to_string();
    // The picker lists only the other profile's rows.
    rig.gateway.set_list_rows_for(
        "default",
        "live",
        vec![mock_gateway::list_row(
            "s-default",
            "Default work",
            Some(1_792_700_000.0),
            None,
            4,
            false,
        )],
    );
    rig.gateway
        .set_list_rows_for("default", "stored", Vec::new());
    rig.gateway.set_list_rows_for("fixture", "live", Vec::new());
    rig.gateway
        .set_list_rows_for("fixture", "stored", Vec::new());
    rig.gateway
        .queue_resume_snapshot(child_snapshot(&folder, "s-default", "Default work", 2));

    let (_outcome, log) = rig
        .drive(StartMode::New, |driver| async move {
            driver.opened().await;
            driver.submit("/resume");
            settle_short().await;
            driver.picker(PickerInput::Action(PickerAction::Open));
            driver.line("Now showing").await;
            driver.submit("after the switch");
            settle_short().await;
            driver.quit();
            driver.finished().await;
        })
        .await;

    let tickets = rig.gateway.ticket_records();
    assert_eq!(tickets.len(), 3, "launch, side, and the new profile link");
    assert_eq!(tickets[2].scope, "profile");
    // Quitting detaches the new session afterwards; the old one is let go once, by the switch.
    let detaches: Vec<_> = rig
        .gateway
        .requests("session.detach")
        .into_iter()
        .filter(|detach| detach.params["session_id"] == json!("local-session-1"))
        .collect();
    assert_eq!(detaches.len(), 1);
    assert!(
        tickets[2].at < detaches[0].at,
        "the new link's ticket comes before the detach"
    );

    // Prompts afterwards travel on the new link, not the old one.
    let creates = rig.gateway.requests("session.create");
    let resumes = rig.gateway.requests("session.resume");
    let submits = rig.gateway.requests("prompt.submit");
    assert_eq!(submits.len(), 1);
    assert_eq!(submits[0].connection, resumes[0].connection);
    assert_ne!(submits[0].connection, creates[0].connection);

    let lines = committed_text(&log);
    assert!(
        lines.iter().any(|line| line.contains("Now showing")),
        "the switch notice: {lines:?}"
    );
    let session = locked(&log).session.clone();
    assert_eq!(session.as_deref(), Some("s-default"));
}

#[tokio::test]
async fn failed_open_keeps_the_old_session() {
    let rig = drive::Rig::start();
    rig.gateway.queue_behavior(
        "session.resume",
        mock_gateway::Behavior::Refuse("not_found".to_owned()),
    );

    let gateway = &rig.gateway;
    let (_outcome, log) = rig
        .drive(StartMode::New, |driver| async move {
            driver.opened().await;
            driver.submit("/resume s-gone");
            driver.line("Could not open").await;
            // Quitting detaches the session in front, so the check comes first.
            assert!(
                gateway.requests("session.detach").is_empty(),
                "the failed switch detaches nothing"
            );
            driver.quit();
            driver.finished().await;
        })
        .await;

    let lines = committed_text(&log);
    assert!(
        lines.iter().any(|line| line
            == "Could not open \"s-gone\": Hermes refused the resume (not_found). Still in the old session."),
        "the failure text: {lines:?}"
    );
    let session = locked(&log).session.clone();
    assert_eq!(session.as_deref(), Some("local-session-1"));
}

#[tokio::test]
async fn open_prompt_is_mentioned_in_the_switch_notice() {
    let rig = drive::Rig::start();
    let folder = rig.folder.to_string_lossy().to_string();
    let mut with_prompt = mock_gateway::data_snapshot("open_approval.json");
    with_prompt["info"]["cwd"] = json!(folder);
    rig.gateway.set_resume_snapshot(with_prompt);
    rig.gateway
        .queue_resume_snapshot(child_snapshot(&folder, "s-two", "Second work", 11));

    let (_outcome, log) = rig
        .drive(StartMode::Resume(None), |driver| async move {
            driver.opened().await;
            driver.submit("/resume s-two");
            driver.line("Now showing").await;
            driver.quit();
            driver.finished().await;
        })
        .await;

    let lines = committed_text(&log);
    assert!(
        lines
            .iter()
            .any(|line| line.contains("Now showing")
                && line.contains("It is waiting for your answer.")),
        "the waiting sentence: {lines:?}"
    );
}

#[tokio::test]
async fn already_here_sends_nothing() {
    // The picker's Enter on the foreground row.
    let folder = tempfile::tempdir().expect("a folder");
    let opened = OpenedSession::from_snapshot(snapshot_of(&resume_snapshot(
        &folder.path().to_string_lossy(),
    )));
    let mut app = AppState::new("launch-1");
    state_with_opened(&mut app, opened);
    let outcome = switch_foreground(&mut app, target("fixture", "local-session-1")).await;
    assert_eq!(outcome, SwitchOutcome::AlreadyHere);

    // /resume with the current id, and with an unknown id.
    let rig = drive::Rig::start();
    let (_outcome, log) = rig
        .drive(StartMode::New, |driver| async move {
            driver.opened().await;
            driver.submit("/resume local-session-1");
            driver.line("already in this session").await;
            driver.submit("/resume s-unknown");
            driver.line("No session with the id").await;
            driver.quit();
            driver.finished().await;
        })
        .await;
    let lines = committed_text(&log);
    assert!(
        lines
            .iter()
            .any(|line| line == "You are already in this session."),
        "the already-here text: {lines:?}"
    );
    // Only the unknown id is asked of Hermes; its refusal gives the "no session" text.
    assert!(
        rig.gateway
            .requests("session.resume")
            .iter()
            .all(|resume| resume.params["session_id"] != json!("local-session-1")),
        "no resume is sent for the session in front"
    );

    // Held prompts refuse the switch.
    let mut held = AppState::new("launch-1");
    let opened = OpenedSession::from_snapshot(snapshot_of(&resume_snapshot("/work")));
    state_with_opened(&mut held, opened);
    held.held.push_back("unsent words".to_owned());
    let outcome = switch_foreground(&mut held, target("fixture", "s-two")).await;
    assert_eq!(
        outcome,
        SwitchOutcome::Failed {
            text: "Finish or discard the prompt you typed first: switches do not carry unsent prompts.".to_owned(),
        }
    );
}

#[tokio::test]
async fn prompt_during_switch_is_refused_and_kept() {
    let rig = drive::Rig::start();
    let folder = rig.folder.to_string_lossy().to_string();
    rig.gateway
        .queue_resume_snapshot(child_snapshot(&folder, "s-two", "Second work", 11));
    rig.gateway.set_delay(
        "session.resume",
        Some(std::time::Duration::from_millis(500)),
    );

    let (_outcome, log) = rig
        .drive(StartMode::New, |driver| async move {
            driver.opened().await;
            driver.submit("/resume s-two");
            settle_short().await;
            driver.picker(PickerInput::Escape);
            driver.send(UserAction::Submit("hello".to_owned()));
            driver.line("Switching sessions").await;
            driver.line("Now showing").await;
            driver.submit("second");
            settle_short().await;
            driver.quit();
            driver.finished().await;
        })
        .await;

    let submits = rig.gateway.requests("prompt.submit");
    assert_eq!(submits.len(), 1, "only the post-switch prompt is sent");
    assert_eq!(submits[0].params["session_id"], json!("s-two"));
    let lines = committed_text(&log);
    assert!(
        lines.iter().any(|line| line
            == "Switching sessions: your prompt was not sent. It is back in the composer; submit it again when the new session is ready."),
        "the refusal text: {lines:?}"
    );
    let drafts = locked(&log).drafts.clone();
    assert_eq!(drafts, ["hello".to_owned()]);
}
