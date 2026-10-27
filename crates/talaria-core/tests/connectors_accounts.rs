//! Account card, alias rules, rename, remove, and reconnect.

#![allow(clippy::too_many_lines)]

#[path = "connectors_common/mod.rs"]
mod common;

use std::time::Instant;

use talaria_core::connectors::{
    AccountAddress, AccountRow, AliasProblem, ConnectorsEffect, ConnectorsFailure, ConnectorsInput,
    ConnectorsKey, ConnectorsState, ConnectorsTab, Screen, account_view, alias_problem, update,
    view,
};

fn on_card(state: &mut ConnectorsState, rows: Vec<AccountRow>, clock: Instant) {
    let _ = update(
        state,
        common::open_consented(ConnectorsTab::Connectors),
        clock,
    );
    let _ = update(
        state,
        ConnectorsInput::ListReply(Ok(common::list_reply())),
        clock,
    );
    let _ = update(
        state,
        ConnectorsInput::CatalogReply(Ok(common::catalog_reply().connectors)),
        clock,
    );
    let _ = update(state, ConnectorsInput::AccountsReply(Ok(rows)), clock);
    let _ = update(state, ConnectorsInput::Key(ConnectorsKey::Open), clock);
}

fn account(alias: Option<&str>, status: &str, disabled: bool, id: &str) -> AccountRow {
    AccountRow {
        connection_id: id.into(),
        connector: "github".into(),
        status: status.into(),
        status_reason: None,
        label: "ada@example.invalid".into(),
        alias: alias.map(str::to_owned),
        active: true,
        disabled,
    }
}

#[test]
fn account_view_folds_retired() {
    let rows = vec![
        account(Some("work"), "active", false, "conn-b"),
        account(None, "failed", false, "conn-a"),
        account(Some("old"), "active", true, "conn-c"),
    ];
    let folded = account_view(&rows, false);
    assert_eq!(folded.retired, 1);
    assert_eq!(folded.rows.len(), 2);
    assert_eq!(folded.rows[0].name, "ada@example.invalid (unnamed)");
    assert_eq!(folded.rows[0].status, "failed");
    assert_eq!(folded.rows[1].name, "work");
    assert_eq!(folded.rows[1].status, "active");
    let shown = account_view(&rows, true);
    assert_eq!(shown.rows.len(), 3);
    assert_eq!(shown.rows[2].name, "old");
    assert_eq!(shown.rows[2].status, "retired");
    assert!(format!("{rows:?}").contains("label_len"));
    assert!(!format!("{rows:?}").contains("ada@example.invalid"));
}

#[test]
fn alias_rules() {
    let live = account(Some("work"), "active", false, "conn-b");
    let retired = account(Some("old"), "active", true, "conn-c");
    let accounts = [live, retired];
    assert_eq!(
        alias_problem("", "github", &accounts, None),
        Some(AliasProblem::Invalid)
    );
    assert_eq!(
        alias_problem(
            "abcdefghijklmnopqrstuvwxyz0123456",
            "github",
            &accounts,
            None
        ),
        Some(AliasProblem::Invalid)
    );
    assert_eq!(
        alias_problem("Work", "github", &accounts, None),
        Some(AliasProblem::Invalid)
    );
    assert_eq!(
        alias_problem("-work", "github", &accounts, None),
        Some(AliasProblem::Invalid)
    );
    assert_eq!(
        alias_problem("work", "github", &accounts, None),
        Some(AliasProblem::Taken)
    );
    assert_eq!(alias_problem("old", "github", &accounts, None), None);
    assert_eq!(
        alias_problem("work", "github", &accounts, Some("conn-b")),
        None
    );
    assert_eq!(alias_problem("work2", "github", &accounts, None), None);
}

#[test]
fn rename_remove_reconnect_effects() {
    let clock = Instant::now();
    let mut state = ConnectorsState::new();
    on_card(
        &mut state,
        vec![account(Some("work"), "failed", false, "conn-b")],
        clock,
    );
    let prompted = update(
        &mut state,
        ConnectorsInput::Key(ConnectorsKey::Rename),
        clock,
    );
    assert!(
        prompted
            .iter()
            .all(|effect| !matches!(effect, ConnectorsEffect::RenameAccount { .. })),
        "rename opens the prefilled prompt"
    );
    let Screen::Prompt { title, text, .. } = view(&state, clock).screen else {
        panic!("prefilled rename prompt");
    };
    assert!(title.contains("Rename"));
    assert_eq!(text, "work");
    let effects = update(&mut state, ConnectorsInput::Key(ConnectorsKey::Open), clock);
    let ConnectorsEffect::RenameAccount { alias, .. } = &effects[0] else {
        panic!("enter submits the prefilled alias");
    };
    assert_eq!(alias, "work");
    let effects = update(
        &mut state,
        ConnectorsInput::RenameReply(Ok(account(Some("work"), "failed", false, "conn-b"))),
        clock,
    );
    assert!(
        effects
            .iter()
            .any(|effect| matches!(effect, ConnectorsEffect::LoadAccounts)),
        "the reply reloads accounts"
    );
    let notice = view(&state, clock).notice.expect("rename notice").text;
    assert_eq!(notice, "Renamed to work (GitHub).");
    assert!(!notice.contains("ada@example.invalid"));
    let _ = update(
        &mut state,
        ConnectorsInput::AccountsReply(Ok(vec![account(Some("work"), "failed", false, "conn-b")])),
        clock,
    );

    let effects = update(
        &mut state,
        ConnectorsInput::Key(ConnectorsKey::Remove),
        clock,
    );
    assert!(effects.is_empty());
    let effects = update(
        &mut state,
        ConnectorsInput::Key(ConnectorsKey::Confirm),
        clock,
    );
    assert!(matches!(effects[0], ConnectorsEffect::RemoveAccount { .. }));
    let _ = update(
        &mut state,
        ConnectorsInput::RemoveReply(Ok(talaria_core::connectors::RemovedReply {
            connection_id: "conn-b".into(),
        })),
        clock,
    );
    assert_eq!(
        view(&state, clock).notice.expect("removed").text,
        "Removed GitHub account work."
    );

    let mut unnamed = ConnectorsState::new();
    on_card(
        &mut unnamed,
        vec![account(None, "failed", false, "conn-a")],
        clock,
    );
    let _ = update(
        &mut unnamed,
        ConnectorsInput::Key(ConnectorsKey::Remove),
        clock,
    );
    let _ = update(
        &mut unnamed,
        ConnectorsInput::Key(ConnectorsKey::Confirm),
        clock,
    );
    let _ = update(
        &mut unnamed,
        ConnectorsInput::RemoveReply(Ok(talaria_core::connectors::RemovedReply {
            connection_id: "conn-a".into(),
        })),
        clock,
    );
    assert_eq!(
        view(&unnamed, clock).notice.expect("unnamed removed").text,
        "Account removed from GitHub."
    );

    let mut repair = ConnectorsState::new();
    on_card(
        &mut repair,
        vec![account(Some("work"), "failed", false, "conn-b")],
        clock,
    );
    let effects = update(
        &mut repair,
        ConnectorsInput::Key(ConnectorsKey::ReconnectOrCopy),
        clock,
    );
    assert_eq!(
        effects,
        vec![ConnectorsEffect::Connect {
            slug: "github".into(),
            address: AccountAddress::Repair {
                connection_id: "conn-b".into()
            },
        }]
    );

    let mut active = ConnectorsState::new();
    on_card(
        &mut active,
        vec![account(Some("work"), "active", false, "conn-b")],
        clock,
    );
    let effects = update(
        &mut active,
        ConnectorsInput::Key(ConnectorsKey::ReconnectOrCopy),
        clock,
    );
    assert!(effects.is_empty());
    assert_eq!(
        view(&active, clock).notice.expect("already").text,
        "GitHub account work is connected."
    );

    let mut retired = ConnectorsState::new();
    on_card(
        &mut retired,
        vec![account(Some("old"), "active", true, "conn-c")],
        clock,
    );
    let _ = update(
        &mut retired,
        ConnectorsInput::Key(ConnectorsKey::Retired),
        clock,
    );
    let effects = update(
        &mut retired,
        ConnectorsInput::Key(ConnectorsKey::ReconnectOrCopy),
        clock,
    );
    assert!(effects.is_empty());
    assert_eq!(
        view(&retired, clock).notice.expect("retired").text,
        "A retired account cannot be reconnected. Add another account instead."
    );
}

#[test]
fn account_errors_map_to_text() {
    let clock = Instant::now();
    let cases = [
        (
            ConnectorsFailure::Refused {
                code: 4090,
                reason: Some("ALIAS_TAKEN".into()),
                message: "That name is already used.".into(),
            },
            "That name is already used.",
        ),
        (
            ConnectorsFailure::Refused {
                code: 4041,
                reason: Some("CONNECTION_NOT_FOUND".into()),
                message: "Connector account not found.".into(),
            },
            "That account no longer exists. Reloaded.",
        ),
        (
            ConnectorsFailure::Refused {
                code: 4000,
                reason: Some("INVALID_PARAMS".into()),
                message: "invalid params".into(),
            },
            "invalid params",
        ),
        (
            ConnectorsFailure::Refused {
                code: 5034,
                reason: Some("CONNECTOR_REQUEST_FAILED".into()),
                message: "Connector request failed. Try again explicitly.".into(),
            },
            "Hermes could not reach the connector service: Connector request failed. Try again explicitly.",
        ),
        (
            ConnectorsFailure::Refused {
                code: 4032,
                reason: Some("NEEDS_NOUS_AUTH".into()),
                message: "Sign in to use connectors.".into(),
            },
            "Sign in to use connectors: hermes -p fixture auth upgrade",
        ),
    ];
    for (failure, expect) in cases {
        let mut state = ConnectorsState::new();
        let _ = update(
            &mut state,
            common::open_consented(ConnectorsTab::Connectors),
            clock,
        );
        let _ = update(
            &mut state,
            ConnectorsInput::RenameReply(Err(failure)),
            clock,
        );
        let notice = view(&state, clock).notice.expect("error").text;
        assert_eq!(notice, expect);
    }
}
