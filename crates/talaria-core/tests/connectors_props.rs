//! Secrecy and hostile text properties.

#![allow(clippy::too_many_lines)]

#[path = "connectors_common/mod.rs"]
mod common;

use std::time::{Duration, Instant};

use proptest::prelude::*;
use proptest::test_runner::FileFailurePersistence;
use talaria_core::connectors::{
    AccountRow, ApprovedPair, ConnectorsEffect, ConnectorsInput, ConnectorsState, ConnectorsTab,
    EnvVar, FieldValue, Operation, PendingPair, PlatformRow, Screen, Target, update, view,
};

fn few_cases() -> ProptestConfig {
    let mut config = ProptestConfig::with_cases(8);
    config.failure_persistence = Some(Box::new(FileFailurePersistence::Off));
    config
}

fn debug_blob(state: &ConnectorsState, effects: &[ConnectorsEffect]) -> String {
    let mut text = format!("{state:?}");
    for effect in effects {
        text.push_str(&format!("{effect:?}"));
        if let ConnectorsEffect::Notice(notice) = effect {
            text.push_str(notice);
        }
    }
    text
}

proptest! {
    #![proptest_config(few_cases())]
    #[test]
    fn secrets_never_leave_their_place(
        secret in "secret-sentinel-[A-Za-z0-9_-]{8,24}",
        entry in "allow-sentinel-[a-z0-9]{4,12}",
    ) {
        let link = format!("https://example.invalid/connect/{secret}");
        let label = format!("{entry}@example.invalid");
        let clock = Instant::now();
        let mut state = ConnectorsState::new();
        let _ = update(&mut state, common::open_consented(ConnectorsTab::Connectors), clock);
        let account = AccountRow {
            connection_id: "conn-test-1".into(),
            connector: "github".into(),
            status: "active".into(),
            status_reason: None,
            label: label.clone(),
            alias: None,
            active: true,
            disabled: false,
        };
        let platform = PlatformRow {
            id: "signal".into(),
            name: "Signal".into(),
            description: "bridge".into(),
            docs_url: None,
            enabled: true,
            state: Some("connected".into()),
            error_message: None,
            home_channel: None,
            ingress_url: None,
            env_vars: vec![EnvVar {
                key: "SIGNAL_HTTP_URL".into(),
                prompt: "Bridge address".into(),
                required: true,
                is_set: false,
                is_list: false,
                is_password: false,
                advanced: false,
                value: Some(secret.clone()),
                help: None,
                description: None,
                url: None,
            }, EnvVar {
                key: "SIGNAL_ALLOWED".into(),
                prompt: "Allowed".into(),
                required: false,
                is_set: true,
                is_list: true,
                is_password: false,
                advanced: false,
                value: Some(entry.clone()),
                help: None,
                description: None,
                url: None,
            }],
        };
        let pairing = talaria_core::connectors::Pairing {
            pending: vec![PendingPair {
                platform: "signal".into(),
                request_id: "req-test-1".into(),
                user_id: secret.clone(),
                user_name: Some(label.clone()),
                age_minutes: 1,
            }],
            approved: vec![ApprovedPair {
                platform: "signal".into(),
                user_id: entry.clone(),
                user_name: Some("pair-name".into()),
            }],
        };
        let _ = update(&mut state, ConnectorsInput::AccountsReply(Ok(vec![account])), clock);
        let _ = update(&mut state, ConnectorsInput::PlatformsReply(Ok(vec![platform])), clock);
        let _ = update(&mut state, ConnectorsInput::PairingReply(Ok(pairing)), clock);
        let _ = update(
            &mut state,
            ConnectorsInput::ConnectReply(Ok(Operation {
                op_id: "op-test-1".into(),
                deadline: clock + Duration::from_secs(30),
                settled: false,
                targets: vec![Target {
                    name: "github".into(),
                    state: "initiated".into(),
                    detail: None,
                    connect_url: Some(link.clone()),
                }],
            })),
            clock,
        );
        let effects = update(
            &mut state,
            ConnectorsInput::SecretEdited { key: "SIGNAL_HTTP_URL".into(), empty: false },
            clock,
        );
        let blob = debug_blob(&state, &effects);
        prop_assert!(!blob.contains(&secret));
        prop_assert!(!blob.contains(&label));
        prop_assert!(!blob.contains(&link));
        prop_assert!(!blob.contains(&entry));
        let shown = view(&state, clock);
        let link_places = match &shown.screen {
            Screen::LinkCard { link: shown_link, .. } => shown_link.clone(),
            other => {
                let text = format!("{other:?}");
                prop_assert!(!text.contains(&link));
                None
            }
        };
        if let Some(shown_link) = link_places {
            prop_assert_eq!(shown_link, link);
        }
        if let Screen::PlatformCard { fields, .. } = &shown.screen {
            for field in fields {
                if let FieldValue::List(values) = &field.value {
                    prop_assert!(values.iter().any(|value| value == &entry) || field.key != "SIGNAL_ALLOWED");
                }
            }
        }
    }
}

proptest! {
    #![proptest_config(few_cases())]
    #[test]
    fn hostile_text_is_sanitised(mark in proptest::sample::select(vec!['\u{1b}', '\u{7f}', '\u{202e}', '\u{202a}', '\u{2066}', '\u{200e}', '\u{061c}', '\u{2028}'])) {
        let clock = Instant::now();
        let mut state = ConnectorsState::new();
        let _ = update(&mut state, common::open_consented(ConnectorsTab::Connectors), clock);
        let mut catalog = common::catalog_reply();
        catalog.connectors[0].name = format!("Git{mark}Hub");
        catalog.connectors[0].description = Some(format!("desc{mark}"));
        let _ = update(&mut state, ConnectorsInput::CatalogReply(Ok(catalog.connectors)), clock);
        let shown = view(&state, clock);
        let text = format!("{shown:?}");
        prop_assert!(!text.chars().any(|item| item == mark));
    }
}
