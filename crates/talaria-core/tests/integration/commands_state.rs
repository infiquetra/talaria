//! When the catalogue is fetched. Issue #223.

use std::time::Duration;

use serde_json::json;
use talaria_core::commands::{
    CatalogResult, CatalogStatus, CommandsEffect, CommandsInput, CommandsState,
    state_update as update,
};

fn arrived(request_id: u64) -> CommandsInput {
    let result: CatalogResult = serde_json::from_value(json!({
        "pairs": [["/help", "Show available commands"]],
        "categories": [{"name": "Info", "pairs": [["/help", "Show available commands"]]}]
    }))
    .expect("catalogue");
    CommandsInput::CatalogArrived { request_id, result }
}

fn fetch(effects: &[CommandsEffect]) -> Option<(u64, Duration)> {
    effects.iter().find_map(|effect| match effect {
        CommandsEffect::FetchCatalog {
            request_id, after, ..
        } => Some((*request_id, *after)),
        CommandsEffect::CatalogUpdated => None,
    })
}

#[test]
fn catalog_fetch_lifecycle() {
    let mut state = CommandsState::default();
    assert_eq!(state.next_id, 1);

    let first = update(&mut state, CommandsInput::Connected);
    assert_eq!(fetch(&first), Some((1, Duration::ZERO)));
    assert!(matches!(state.status, CatalogStatus::Loading));

    let second = update(
        &mut state,
        CommandsInput::ForegroundSession(Some("local-session-2".into())),
    );
    assert_eq!(fetch(&second), Some((2, Duration::ZERO)));
    assert_eq!(state.bound_session.as_deref(), Some("local-session-2"));

    let stale = update(&mut state, arrived(1));
    assert!(stale.is_empty(), "an older catalogue reply is ignored");
    assert!(state.catalog.is_none());

    assert!(update(&mut state, CommandsInput::SkillsChanged).is_empty());
    assert!(update(&mut state, CommandsInput::SkillsChanged).is_empty());
    assert!(state.dirty);

    let third = update(&mut state, arrived(2));
    assert_eq!(fetch(&third), Some((3, Duration::ZERO)));
    assert!(!state.dirty);

    let retry = update(
        &mut state,
        CommandsInput::CatalogFailed {
            request_id: 3,
            reason: "down".into(),
        },
    );
    assert_eq!(fetch(&retry), Some((4, Duration::from_secs(2))));
    assert!(matches!(state.status, CatalogStatus::Failed(_)));

    let stopped = update(
        &mut state,
        CommandsInput::CatalogFailed {
            request_id: 4,
            reason: "down".into(),
        },
    );
    assert!(fetch(&stopped).is_none());

    let mut again = CommandsState::default();
    let _ = update(&mut again, CommandsInput::Connected);
    let _ = update(
        &mut again,
        CommandsInput::CatalogFailed {
            request_id: 1,
            reason: "down".into(),
        },
    );
    let superseded = update(&mut again, CommandsInput::Connected);
    assert_eq!(
        fetch(&superseded).map(|(_, after)| after),
        Some(Duration::ZERO)
    );
    assert!(again.refused.is_empty());
    assert!(!again.retried);
}

#[test]
fn refused_names_cleared_on_reconnect() {
    let mut state = CommandsState::default();
    let _ = update(
        &mut state,
        CommandsInput::Refused {
            name: "version".into(),
        },
    );
    assert!(state.refused.contains("version"));
    assert!(!state.refused.contains("/version"));

    let _ = update(&mut state, CommandsInput::Connected);
    assert!(state.refused.is_empty());
    assert!(
        !state.refused.contains("usage"),
        "a refusal of /usage reset does not dim /usage"
    );
    assert!(
        !state.refused.contains("nope"),
        "an unknown name is never added"
    );
}
