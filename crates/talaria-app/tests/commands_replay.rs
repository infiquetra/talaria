//! Recorded slash replies. Issue #223, scenario "slash palette fixture".

#[path = "support/mock_gateway.rs"]
mod mock_gateway;

use std::path::PathBuf;

use hermes_client::ReplaySource;
use mock_gateway::{MockGateway, SESSION_ID};
use talaria_app::commands::{CommandService, outcome_actions};
use talaria_core::commands::CommandOutcome;
use talaria_core::session::{Event, NoticeKind};

fn fixture() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/frames/slash-palette.jsonl")
}

fn command_of(source: &ReplaySource, needle: &str) -> String {
    source
        .outbound()
        .find_map(|frame| {
            let method = frame.frame["method"].as_str()?;
            if method != "slash.exec" {
                return None;
            }
            let command = frame.frame["params"]["command"].as_str()?;
            command.contains(needle).then(|| command.to_owned())
        })
        .unwrap_or_else(|| panic!("no slash.exec containing {needle}"))
}

fn notice_text(outcome: CommandOutcome, typed: &str) -> String {
    outcome_actions(outcome, typed)
        .into_iter()
        .find_map(|action| match action {
            talaria_app::commands::AppAction::Event(Event::Notice { kind, text }) => {
                Some(format!("{kind:?}:{text}"))
            }
            _ => None,
        })
        .unwrap_or_default()
}

#[tokio::test]
async fn slash_fixture_replays_to_expected_cells() {
    let path = fixture();
    let source = ReplaySource::open(&path).expect("slash-palette fixture opens");
    assert_eq!(source.header().scenario.as_deref(), Some("slash-palette"));
    let version = command_of(&source, "version");
    let plan = command_of(&source, "plan");
    let skill = command_of(&source, "fixture-skill-");
    let worktree = command_of(&source, "worktree");
    let unknown = command_of(&source, "nonexistent-talaria-check");

    let (_gateway, client, _inbound) = MockGateway::start().await;
    let mut service = CommandService::new(client);

    let version_outcome = service.execute(SESSION_ID, &format!("/{version}")).await;
    let version_text = notice_text(version_outcome, &format!("/{version}"));
    assert!(
        version_text.starts_with(&format!("{:?}:/{version}\n", NoticeKind::System))
            || version_text.contains("/version")
    );
    assert!(
        version_text
            .lines()
            .nth(1)
            .is_some_and(|line| line.starts_with("Hermes Agent"))
            || version_text.contains("Hermes Agent")
    );

    let plan_outcome = service.execute(SESSION_ID, &format!("/{plan}")).await;
    let plan_text = notice_text(plan_outcome.clone(), &format!("/{plan}"));
    assert!(plan_text.contains("Planning: write a haiku about rain"));
    let user = outcome_actions(plan_outcome, &format!("/{plan}"));
    assert!(user.iter().any(|action| matches!(
        action,
        talaria_app::commands::AppAction::Event(Event::SubmitCommand { display, .. })
            if display == "/plan write a haiku about rain"
    )));

    let skill_outcome = service.execute(SESSION_ID, &format!("/{skill}")).await;
    let skill_actions = outcome_actions(skill_outcome, &format!("/{skill}"));
    assert!(skill_actions.iter().any(|action| matches!(
        action,
        talaria_app::commands::AppAction::Event(Event::SubmitCommand { display, .. })
            if regex_skill(display)
    )));

    let work = service.execute(SESSION_ID, &format!("/{worktree}")).await;
    match work {
        CommandOutcome::Refused { message, .. } => {
            assert!(message.contains("not available on the shared Hermes gateway"));
            assert!(message.lines().nth(1).is_some());
        }
        other => panic!("{other:?}"),
    }

    let missing = service.execute(SESSION_ID, &format!("/{unknown}")).await;
    match missing {
        CommandOutcome::Refused { message, .. } => {
            assert!(message.contains("does not know"));
        }
        other => panic!("{other:?}"),
    }
}

fn regex_skill(display: &str) -> bool {
    let Some(rest) = display.strip_prefix("/fixture-skill-") else {
        return false;
    };
    let Some((number, tail)) = rest.split_once(' ') else {
        return false;
    };
    !number.is_empty() && number.chars().all(|ch| ch.is_ascii_digit()) && tail == "tidy"
}
