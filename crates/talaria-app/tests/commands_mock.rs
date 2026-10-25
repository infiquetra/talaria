//! Slash commands against the in-process gateway. Issue #223.

#[path = "support/mock_gateway.rs"]
mod mock_gateway;

use std::time::Duration;

use mock_gateway::{MockGateway, SESSION_ID};
use talaria_app::commands::{AppAction, CommandService, outcome_actions};
use talaria_core::commands::{CommandOutcome, CommandsInput};
use talaria_core::session::{Event, NoticeKind, SessionId, SessionState};

const NO_SESSION: &str = "No session is attached, so /help cannot run. Start one with talaria, talaria -c or talaria --resume <id>.";
const NO_SLASH: &str = "This Hermes gateway does not run slash commands. Check that it is at the release Talaria is pinned to.";

/// The mock records a request when its socket task reads it, after the client's writer, so
/// wait for the first one and then a little longer for any that follow.
async fn wait_for_requests(gateway: &MockGateway, method: &str) {
    for _ in 0..200 {
        if !gateway.requests(method).is_empty() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    tokio::time::sleep(Duration::from_millis(50)).await;
}

fn no_nulls(value: &serde_json::Value) {
    let Some(object) = value.as_object() else {
        return;
    };
    for (key, child) in object {
        assert!(!child.is_null(), "{key} is null");
        no_nulls(child);
    }
}

#[tokio::test]
async fn outbound_params_never_contain_null() {
    let (gateway, client, _inbound) = MockGateway::start().await;
    let mut service = CommandService::new(client);
    let typed = "/goal line one\n  line two";
    let _ = service.execute(SESSION_ID, typed).await;
    let frames = gateway.requests("slash.exec");
    assert_eq!(frames.len(), 1);
    assert_eq!(frames[0].params["command"], "goal line one\n  line two");
    assert!(frames[0].params.get("profile").is_none());
    no_nulls(&frames[0].params);

    let _ = service.fetch_catalog(1, None).await;
    let catalog = gateway.requests("commands.catalog");
    assert!(
        catalog
            .iter()
            .any(|frame| frame.params.as_object().is_some_and(|map| map.is_empty()))
    );
    for frame in &catalog {
        no_nulls(&frame.params);
    }

    let _ = service
        .complete(2, "/he".into(), Some(SESSION_ID.into()))
        .await;
    let completion = gateway.requests("complete.slash");
    assert!(completion.iter().any(|frame| frame.params["text"] == "/he"));
    for frame in &completion {
        no_nulls(&frame.params);
        assert!(frame.params.get("profile").is_none());
    }
}

#[tokio::test]
async fn skill_directive_submits_message_and_shows_display() {
    let (gateway, client, _inbound) = MockGateway::start().await;
    let mut service = CommandService::new(client);
    let typed = "/fixture-skill-1 tidy";
    let outcome = service.execute(SESSION_ID, typed).await;
    let (text, display) = match &outcome {
        CommandOutcome::Submit { text, display, .. } => (text.clone(), display.clone()),
        other => panic!("{other:?}"),
    };
    assert_eq!(display, "/fixture-skill-1 tidy");
    assert!(!display.contains("Fixture skill body."));
    let actions = outcome_actions(outcome, typed);
    assert!(actions.iter().any(|action| matches!(
        action,
        AppAction::Event(Event::SubmitCommand { display, .. }) if display == "/fixture-skill-1 tidy"
    )));
    let submits = gateway.requests("prompt.submit");
    assert_eq!(submits.len(), 1);
    assert_eq!(submits[0].params["text"], text);
    assert!(
        submits[0].params["submission_id"]
            .as_str()
            .is_some_and(|id| !id.is_empty())
    );
    assert!(submits[0].params.get("unattended").is_none());

    let quiet = service.execute("", "/help").await;
    assert_eq!(
        quiet,
        CommandOutcome::Failed {
            message: NO_SESSION.into()
        }
    );
    assert!(
        gateway
            .requests("slash.exec")
            .iter()
            .all(|frame| frame.params["command"] != "help")
    );
}

#[tokio::test]
async fn method_not_found_falls_back_to_command_dispatch_once() {
    let (gateway, client, _inbound) = MockGateway::start().await;
    gateway.set_gateway_without_slash_exec(true);
    let mut service = CommandService::new(client);
    let _ = service.execute(SESSION_ID, "/version").await;
    let _ = service.execute(SESSION_ID, "/help").await;
    assert_eq!(gateway.requests("slash.exec").len(), 2);
    assert_eq!(gateway.requests("command.dispatch").len(), 2);

    gateway.set_gateway_without_command_dispatch(true);
    let missing = service.execute(SESSION_ID, "/plan x").await;
    assert_eq!(
        missing,
        CommandOutcome::Failed {
            message: NO_SLASH.into()
        }
    );
    assert_eq!(gateway.requests("command.dispatch").len(), 3);

    gateway.set_gateway_without_slash_exec(false);
    gateway.set_gateway_without_command_dispatch(false);
    let before = gateway.requests("command.dispatch").len();
    let refused = service.execute(SESSION_ID, "/worktree").await;
    match refused {
        CommandOutcome::Refused { message, .. } => {
            assert!(message.contains("not available on the shared Hermes gateway"));
            assert!(message.lines().nth(1).is_some());
        }
        other => panic!("{other:?}"),
    }
    assert_eq!(gateway.requests("command.dispatch").len(), before);
}

#[tokio::test]
async fn skills_changed_refetches_catalog_without_touching_session() {
    let (gateway, client, _inbound) = MockGateway::start().await;
    let mut service = CommandService::new(client);
    let session = SessionState::empty(SessionId::new(SESSION_ID));
    let cells = session.cells().to_vec();
    let cursor = session.cursor();
    let resume = session.needs_resume();
    gateway.skills_changed().await;
    let _ = service.handle_input(CommandsInput::SkillsChanged);
    wait_for_requests(&gateway, "commands.catalog").await;
    assert_eq!(gateway.requests("commands.catalog").len(), 1);
    assert_eq!(session.cells(), cells.as_slice());
    assert_eq!(session.cursor(), cursor);
    assert_eq!(session.needs_resume(), resume);
}

#[tokio::test]
async fn completion_debounce_resets_on_each_draft_change() {
    let (gateway, client, _inbound) = MockGateway::start().await;
    tokio::time::pause();
    let mut service = CommandService::new(client);
    service.draft_changed("/h".into(), 2);
    tokio::time::advance(Duration::from_millis(30)).await;
    service.draft_changed("/he".into(), 3);
    tokio::time::advance(Duration::from_millis(30)).await;
    service.draft_changed("/hel".into(), 4);
    tokio::time::advance(Duration::from_millis(60)).await;
    tokio::time::resume();
    wait_for_requests(&gateway, "complete.slash").await;
    let calls = gateway.requests("complete.slash");
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].params["text"], "/hel");
}

#[tokio::test]
async fn mid_turn_skill_queues_and_plan_is_refused() {
    let (gateway, client, _inbound) = MockGateway::start().await;
    gateway.set_turn_running(true);
    let mut service = CommandService::new(client);
    let skill = service.execute(SESSION_ID, "/fixture-skill-1 tidy").await;
    let actions = outcome_actions(skill, "/fixture-skill-1 tidy");
    assert!(
        actions
            .iter()
            .any(|action| matches!(action, AppAction::Event(Event::SubmitCommand { .. })))
    );
    assert_eq!(gateway.requests("prompt.submit").len(), 1);

    let before = gateway.requests("prompt.submit").len();
    let plan = service.execute(SESSION_ID, "/plan x").await;
    let actions = outcome_actions(plan, "/plan x");
    assert!(actions.iter().any(|action| matches!(
        action,
        AppAction::Event(Event::Notice { kind: NoticeKind::System, text })
            if text.contains("can't run mid-turn")
    )));
    assert_eq!(gateway.requests("prompt.submit").len(), before);
}
