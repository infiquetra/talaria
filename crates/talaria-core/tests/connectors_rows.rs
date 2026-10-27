//! Connector rows: join, labels, refusals, and the consent gate.

#![allow(clippy::too_many_lines)]

#[path = "connectors_common/mod.rs"]
mod common;

use std::time::Instant;

use talaria_core::connectors::{
    AccountRow, CatalogRow, ConnectorsEffect, ConnectorsFailure, ConnectorsInput, ConnectorsKey,
    ConnectorsOpen, ConnectorsState, ConnectorsTab, ConnectorRow, ListReply, Screen, connector_label,
    update, view,
};

fn now() -> Instant {
    Instant::now()
}

fn open(consented: bool, tab: ConnectorsTab) -> ConnectorsInput {
    ConnectorsInput::Open {
        open: ConnectorsOpen::Tab(tab),
        profile: common::PROFILE.into(),
        consented,
    }
}

fn row(slug: &str) -> ConnectorRow {
    ConnectorRow {
        connector: slug.into(),
        enabled: true,
        connected: false,
        connection_status: None,
        status_reason: None,
        gateway_disabled_tools: Vec::new(),
        org_locked: false,
        listed: true,
    }
}

fn account(status: &str, disabled: bool) -> AccountRow {
    AccountRow {
        connection_id: "conn-test-1".into(),
        connector: "github".into(),
        status: status.into(),
        status_reason: None,
        label: "test-account".into(),
        alias: Some("work".into()),
        active: false,
        disabled,
    }
}

#[test]
fn rows_join_list_catalog_and_accounts() {
    common::scrub_fixtures();
    let list = common::list_reply();
    let catalog = common::catalog_reply();
    let accounts = common::accounts();
    assert!(list.available);
    assert_eq!(list.connectors[0].connector, "github");
    assert_eq!(catalog.connectors.len(), 2);
    assert_eq!(accounts[0].alias.as_deref(), Some("work"));

    let mut state = ConnectorsState::new();
    let clock = now();
    let orders = [
        vec![
            ConnectorsInput::ListReply(Ok(list.clone())),
            ConnectorsInput::CatalogReply(Ok(catalog.connectors.clone())),
            ConnectorsInput::AccountsReply(Ok(accounts.clone())),
        ],
        vec![
            ConnectorsInput::CatalogReply(Ok(catalog.connectors.clone())),
            ConnectorsInput::AccountsReply(Ok(accounts.clone())),
            ConnectorsInput::ListReply(Ok(list.clone())),
        ],
        vec![
            ConnectorsInput::AccountsReply(Ok(accounts.clone())),
            ConnectorsInput::ListReply(Ok(list.clone())),
            ConnectorsInput::CatalogReply(Ok(catalog.connectors.clone())),
        ],
    ];
    for inputs in orders {
        let _ = update(&mut state, open(true, ConnectorsTab::Connectors), clock);
        for input in inputs {
            let _ = update(&mut state, input, clock);
        }
        let shown = view(&state, clock);
        let Screen::ConnectorList { rows, .. } = shown.screen else {
            panic!("joined rows");
        };
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].label, "GitHub");
        assert_eq!(rows[1].label, "Linear");
    }

    let mut listed = ConnectorsState::new();
    let _ = update(&mut listed, open(true, ConnectorsTab::Connectors), clock);
    let _ = update(&mut listed, ConnectorsInput::ListReply(Ok(list)), clock);
    let Screen::ConnectorList { rows, .. } = view(&listed, clock).screen else {
        panic!("list only");
    };
    assert_eq!(rows[0].label, "github");

    let mut catalogue = ConnectorsState::new();
    let _ = update(&mut catalogue, open(true, ConnectorsTab::Connectors), clock);
    let _ = update(
        &mut catalogue,
        ConnectorsInput::CatalogReply(Ok(vec![CatalogRow {
            slug: "linear".into(),
            name: "Linear".into(),
            description: None,
            category: None,
        }])),
        clock,
    );
    let Screen::ConnectorList { rows, .. } = view(&catalogue, clock).screen else {
        panic!("catalog only");
    };
    assert_eq!(rows[0].label, "Linear");
    assert_eq!(rows[0].state, "not offered");
}

#[test]
fn connector_label_precedence() {
    let mut locked = row("github");
    locked.org_locked = true;
    assert_eq!(connector_label(&locked, &[]), "off by your organization");

    let mut off = row("github");
    off.enabled = false;
    assert_eq!(connector_label(&off, &[]), "off");

    let one = account("active", false);
    assert_eq!(connector_label(&row("github"), &[one.clone()]), "connected · 1 account");
    let mut second = one.clone();
    second.connection_id = "conn-test-2".into();
    assert_eq!(
        connector_label(&row("github"), &[one.clone(), second]),
        "connected · 2 accounts"
    );

    let mut connected = row("github");
    connected.connected = true;
    assert_eq!(connector_label(&connected, &[]), "connected");

    let mut pending = row("github");
    pending.connection_status = Some("pending".into());
    assert_eq!(connector_label(&pending, &[]), "connecting…");

    for status in ["failed", "expired", "revoked", "inactive"] {
        let mut repair = row("github");
        repair.connection_status = Some(status.into());
        assert_eq!(connector_label(&repair, &[]), format!("needs repair ({status})"));
    }

    let mut offered = row("github");
    offered.listed = false;
    assert_eq!(connector_label(&offered, &[]), "not offered");
    assert_eq!(connector_label(&row("github"), &[]), "not connected");

    let mut ignored = account("failed", false);
    ignored.active = true;
    assert_ne!(connector_label(&row("github"), &[ignored]), "connected · 1 account");
}

#[test]
fn unavailable_and_gate_refusals() {
    let mut state = ConnectorsState::new();
    let clock = now();
    let _ = update(
        &mut state,
        open(true, ConnectorsTab::Connectors),
        clock,
    );
    let _ = update(
        &mut state,
        ConnectorsInput::ListReply(Ok(ListReply {
            available: false,
            connectors: Vec::new(),
        })),
        clock,
    );
    let shown = view(&state, clock);
    let Screen::Unavailable { text, command } = shown.screen else {
        panic!("unavailable");
    };
    assert!(text.contains("hermes -p fixture auth upgrade"));
    assert!(text.contains("Connectors are not available for profile fixture."));
    assert_eq!(command.as_deref(), Some("hermes -p fixture auth upgrade"));

    let cases = [
        (4031, Some("CONNECTORS_UNAVAILABLE"), "Connectors are not available for profile fixture."),
        (4032, Some("NEEDS_NOUS_AUTH"), "Sign in to use connectors: hermes -p fixture auth upgrade"),
        (4030, Some("ORG_ACCESS_DENIED"), "This account cannot manage connectors for this organization."),
        (5034, Some("CONNECTORS_UNAVAILABLE"), "Hermes could not reach the connector service:"),
        (-32601, None, "This Hermes does not offer connectors.list. Check the version with /doctor."),
    ];
    for (code, reason, expect) in cases {
        let mut refused = ConnectorsState::new();
        let _ = update(&mut refused, open(true, ConnectorsTab::Connectors), clock);
        let _ = update(
            &mut refused,
            ConnectorsInput::ListReply(Err(ConnectorsFailure::Refused {
                code,
                reason: reason.map(str::to_owned),
                message: expect.into(),
            })),
            clock,
        );
        let text = format!("{:?}", view(&refused, clock));
        assert!(text.contains(expect), "{code}");
    }
}

#[test]
fn consent_gates_every_call() {
    let clock = now();
    let mut state = ConnectorsState::new();
    let effects = update(&mut state, open(false, ConnectorsTab::Connectors), clock);
    assert!(effects.is_empty());
    assert!(matches!(view(&state, clock).screen, Screen::Consent { .. }));
    let consent = view(&state, clock);
    let Screen::Consent { text } = consent.screen else {
        panic!("consent card");
    };
    assert!(text.contains("Hermes creates a free guest identity for it when you continue."));
    assert!(text.contains("fixture"));
    assert!(consent.footer.iter().any(|(key, label)| key == "enter" && label.contains("continue")));

    let effects = update(&mut state, ConnectorsInput::Key(ConnectorsKey::Close), clock);
    assert_eq!(effects, vec![ConnectorsEffect::Close]);

    let mut again = ConnectorsState::new();
    let _ = update(&mut again, open(false, ConnectorsTab::Connectors), clock);
    let effects = update(&mut again, ConnectorsInput::Key(ConnectorsKey::Open), clock);
    assert_eq!(
        effects,
        vec![ConnectorsEffect::GrantConsent, ConnectorsEffect::ListConnectors]
    );

    let mut consented = ConnectorsState::new();
    let effects = update(&mut consented, open(true, ConnectorsTab::Connectors), clock);
    assert_eq!(effects, vec![ConnectorsEffect::ListConnectors]);
    assert!(!matches!(view(&consented, clock).screen, Screen::Consent { .. }));

    let mut messaging = ConnectorsState::new();
    let effects = update(&mut messaging, open(true, ConnectorsTab::Messaging), clock);
    assert_eq!(
        effects,
        vec![ConnectorsEffect::LoadPlatforms, ConnectorsEffect::LoadPairing]
    );
    let effects = update(&mut messaging, ConnectorsInput::Key(ConnectorsKey::NextTab), clock);
    assert_eq!(effects, vec![ConnectorsEffect::ListConnectors]);
    let _ = update(
        &mut messaging,
        ConnectorsInput::ListReply(Ok(common::list_reply())),
        clock,
    );
    let effects = update(&mut messaging, ConnectorsInput::Key(ConnectorsKey::PrevTab), clock);
    assert_eq!(
        effects,
        vec![ConnectorsEffect::LoadPlatforms, ConnectorsEffect::LoadPairing]
    );
    let effects = update(&mut messaging, ConnectorsInput::Key(ConnectorsKey::NextTab), clock);
    assert!(effects.is_empty(), "connectors already loaded");
}
