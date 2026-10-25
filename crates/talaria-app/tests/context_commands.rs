//! The local commands: what the application does with a typed line.

#[path = "support/mock_gateway.rs"]
mod mock_gateway;

use hermes_contract::schemas::CanonicalSnapshot;
use talaria_app::app::{AppEffect, AppMessage, AppState, UserAction, update};
use talaria_app::sessions::OpenedSession;

/// A session with the mock's recorded snapshot, so a command has somewhere to read from.
fn opened() -> OpenedSession {
    let snapshot: CanonicalSnapshot =
        serde_json::from_value(mock_gateway::recorded_resume_snapshot())
            .expect("a contract-shaped snapshot");
    OpenedSession::from_snapshot(snapshot)
}

#[test]
fn context_and_usage_become_reads() {
    let mut state = AppState::new("launch-1");
    for line in ["/context", "/usage", "/cost"] {
        let effects = update(
            &mut state,
            AppMessage::User(UserAction::Submit(line.to_owned())),
        );
        assert!(
            effects
                .iter()
                .any(|effect| matches!(effect, AppEffect::ContextRead { .. })),
            "{line} is a read"
        );
    }
}

#[test]
fn a_read_without_a_session_says_so() {
    let mut state = AppState::new("launch-1");
    let effects = update(
        &mut state,
        AppMessage::User(UserAction::Submit("/context".to_owned())),
    );
    assert!(
        effects
            .iter()
            .any(|effect| matches!(effect, AppEffect::ContextRead { .. })),
        "the read is asked for"
    );
}

#[test]
fn compact_sends_a_request_with_the_focus() {
    let mut state = AppState::new("launch-1");
    let effects = update(
        &mut state,
        AppMessage::Opened {
            opened: opened(),
            notice: None,
        },
    );
    assert!(!effects.is_empty(), "the session opens");
    let effects = update(
        &mut state,
        AppMessage::User(UserAction::Submit("/compact here 2".to_owned())),
    );
    let request = effects
        .iter()
        .find_map(|effect| match effect {
            AppEffect::CompactRequest { request, .. } => Some(request),
            _ => None,
        })
        .expect("a compaction request");
    assert_eq!(request.operation, "compress");
    assert_eq!(
        request
            .payload
            .get("focus")
            .and_then(|value| value.as_str()),
        Some("here 2")
    );
    assert!(!request.request_id.is_empty(), "the request carries an id");
}

#[test]
fn an_unknown_slash_line_is_not_a_local_command() {
    let mut state = AppState::new("launch-1");
    let effects = update(
        &mut state,
        AppMessage::User(UserAction::Submit("/not-a-command".to_owned())),
    );
    assert!(
        !effects
            .iter()
            .any(|effect| matches!(effect, AppEffect::ContextRead { .. })),
        "an unknown slash line is left to the gateway"
    );
}

#[test]
fn a_command_with_an_argument_it_does_not_take_is_refused() {
    let mut state = AppState::new("launch-1");
    let effects = update(
        &mut state,
        AppMessage::User(UserAction::Submit("/context x".to_owned())),
    );
    assert!(
        !effects
            .iter()
            .any(|effect| matches!(effect, AppEffect::ContextRead { .. })),
        "the refusal is a notice"
    );
}
