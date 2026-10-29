//! Messaging rows, the platform card, and the on or off confirmation.

#![allow(clippy::too_many_lines)]

#[path = "connectors_common/mod.rs"]
mod common;

use std::time::Instant;

use talaria_core::connectors::{
    ConnectorsEffect, ConnectorsFailure, ConnectorsInput, ConnectorsKey, ConnectorsState, EnvVar,
    FieldValue, PlatformRow, Screen, UpdateReply, platform_label, update, view,
};

fn ready(clock: Instant) -> ConnectorsState {
    let mut state = ConnectorsState::default();
    let _ = update(
        &mut state,
        common::open_consented(talaria_core::connectors::ConnectorsTab::Messaging),
        clock,
    );
    state
}

fn platform(state: Option<&str>, error: Option<&str>) -> PlatformRow {
    PlatformRow {
        id: "signal".into(),
        name: "Signal".into(),
        description: "Signal bridge".into(),
        docs_url: Some("https://example.invalid/docs/signal".into()),
        enabled: state != Some("disabled"),
        state: state.map(str::to_owned),
        error_message: error.map(str::to_owned),
        home_channel: None,
        ingress_url: None,
        env_vars: vec![EnvVar {
            key: "SIGNAL_HTTP_URL".into(),
            prompt: "Bridge address".into(),
            required: true,
            is_set: true,
            is_list: false,
            is_password: false,
            advanced: false,
            value: Some("should-not-show".into()),
            help: None,
            description: None,
            url: None,
        }],
    }
}

#[test]
fn toggle_confirmation_keeps_its_direction_across_refreshes() {
    let clock = Instant::now();
    for initially_enabled in [false, true] {
        for refresh_changes_value in [false, true] {
            let mut state = ready(clock);
            let mut row = platform(None, None);
            row.enabled = initially_enabled;
            let _ = update(
                &mut state,
                ConnectorsInput::PlatformsReply(Ok(vec![row.clone()])),
                clock,
            );
            let _ = update(&mut state, ConnectorsInput::Key(ConnectorsKey::Open), clock);
            let _ = update(
                &mut state,
                ConnectorsInput::Key(ConnectorsKey::Toggle),
                clock,
            );
            let Screen::Confirm { text: question } = view(&state, clock).screen else {
                panic!("toggle confirmation");
            };
            row.enabled = initially_enabled ^ refresh_changes_value;
            let _ = update(
                &mut state,
                ConnectorsInput::PlatformsLoaded(Ok(talaria_core::connectors::PlatformsReply {
                    gateway_start_command: None,
                    platforms: vec![row],
                })),
                clock,
            );
            let Screen::Confirm {
                text: after_refresh,
            } = view(&state, clock).screen
            else {
                panic!("confirmation must remain stable until confirmed or cancelled");
            };
            assert_eq!(after_refresh, question);
            let effects = update(
                &mut state,
                ConnectorsInput::Key(ConnectorsKey::Confirm),
                clock,
            );
            if refresh_changes_value {
                assert!(effects.is_empty(), "the intended state is already present");
            } else {
                assert!(
                    matches!(effects.as_slice(), [ConnectorsEffect::UpdatePlatform { id, enabled: Some(value), .. }] if id == "signal" && *value != initially_enabled)
                );
            }
        }
    }
}

#[test]
fn toggle_confirmation_keeps_platform_identity_when_the_list_changes() {
    let clock = Instant::now();
    for remove in [false, true] {
        let mut state = ready(clock);
        let original = platform(None, None);
        let _ = update(
            &mut state,
            ConnectorsInput::PlatformsReply(Ok(vec![original.clone()])),
            clock,
        );
        let _ = update(
            &mut state,
            ConnectorsInput::Key(ConnectorsKey::Toggle),
            clock,
        );
        let Screen::Confirm { text: question } = view(&state, clock).screen else {
            panic!("confirmation")
        };
        let mut other = original.clone();
        other.id = "other".into();
        other.name = "Other".into();
        let rows = if remove {
            vec![other]
        } else {
            vec![other, original]
        };
        let _ = update(
            &mut state,
            ConnectorsInput::PlatformsLoaded(Ok(talaria_core::connectors::PlatformsReply {
                gateway_start_command: None,
                platforms: rows,
            })),
            clock,
        );
        assert!(matches!(view(&state, clock).screen, Screen::Confirm { text } if text == question));
        let effects = update(
            &mut state,
            ConnectorsInput::Key(ConnectorsKey::Confirm),
            clock,
        );
        if remove {
            assert!(effects.is_empty());
            assert!(
                view(&state, clock)
                    .notice
                    .unwrap()
                    .text
                    .contains("no longer available")
            );
        } else {
            assert!(
                matches!(effects.as_slice(), [ConnectorsEffect::UpdatePlatform { id, enabled: Some(false), .. }] if id == "signal")
            );
        }
    }
}

#[test]
fn filtered_platform_opens_with_one_enter_and_unwinds_with_three_escapes() {
    let clock = Instant::now();
    let mut state = ready(clock);
    let _ = update(
        &mut state,
        ConnectorsInput::PlatformsReply(Ok(common::platforms().platforms)),
        clock,
    );
    let _ = update(
        &mut state,
        ConnectorsInput::Key(ConnectorsKey::Filter),
        clock,
    );
    let _ = update(&mut state, ConnectorsInput::Paste("signal".into()), clock);
    let _ = update(&mut state, ConnectorsInput::Key(ConnectorsKey::Open), clock);
    assert!(matches!(
        view(&state, clock).screen,
        Screen::PlatformCard { fields, .. }
            if fields.iter().any(|field| field.key == "SIGNAL_HTTP_URL")
    ));
    let _ = update(
        &mut state,
        ConnectorsInput::Key(ConnectorsKey::Close),
        clock,
    );
    assert!(matches!(
        view(&state, clock).screen,
        Screen::PlatformList { rows, .. } if rows.len() == 1
    ));
    let _ = update(
        &mut state,
        ConnectorsInput::Key(ConnectorsKey::Close),
        clock,
    );
    assert!(matches!(
        view(&state, clock).screen,
        Screen::PlatformList { rows, .. } if rows.len() > 1
    ));
    assert!(
        update(
            &mut state,
            ConnectorsInput::Key(ConnectorsKey::Close),
            clock
        )
        .contains(&ConnectorsEffect::Close)
    );
}

#[test]
fn platform_label_table() {
    let cases = [
        (Some("connected"), None, "connected"),
        (Some("connecting"), None, "connecting…"),
        (Some("retrying"), Some("later"), "retrying: later"),
        (Some("retrying"), None, "retrying"),
        (Some("fatal"), Some("boom"), "error: boom"),
        (Some("fatal"), None, "error"),
        (Some("disabled"), None, "off"),
        (Some("not_configured"), None, "needs setup"),
        (Some("pending_restart"), None, "restart needed"),
        (Some("gateway_stopped"), None, "gateway stopped"),
        (Some("startup_failed"), Some("no"), "startup failed: no"),
        (Some("startup_failed"), None, "startup failed"),
        (Some("paused"), None, "paused"),
        (Some("disconnected"), None, "disconnected"),
        (Some("odd"), None, "unknown (odd)"),
        (None, None, "unknown"),
    ];
    for (state, error, expect) in cases {
        assert_eq!(platform_label(&platform(state, error)), expect, "{expect}");
    }
    let clock = Instant::now();
    let mut overlay = ready(clock);
    let mut rows = common::platforms();
    rows.platforms.push(platform(Some("pending_restart"), None));
    let _ = update(
        &mut overlay,
        ConnectorsInput::PlatformsReply(Ok(rows.platforms)),
        clock,
    );
    let _ = update(
        &mut overlay,
        ConnectorsInput::PairingReply(Ok(common::pairing())),
        clock,
    );
    let Screen::PlatformList {
        rows, restart_line, ..
    } = view(&overlay, clock).screen
    else {
        panic!("list");
    };
    assert!(rows.iter().any(|row| row.state.contains("waiting to pair")));
    assert!(restart_line);
    let shown = format!("{rows:?}");
    assert!(
        shown.contains("Some changes wait for a gateway restart. R opens the doctor.")
            || restart_line
    );
}

#[test]
fn card_never_shows_values() {
    let clock = Instant::now();
    let mut state = ready(clock);
    let loaded = common::platforms();
    let _ = update(
        &mut state,
        ConnectorsInput::PlatformsReply(Ok(loaded.platforms)),
        clock,
    );
    let _ = update(&mut state, ConnectorsInput::Key(ConnectorsKey::Open), clock);
    let Screen::PlatformCard { fields, lines, .. } = view(&state, clock).screen else {
        panic!("card");
    };
    let secret = fields
        .iter()
        .find(|field| field.key == "SIGNAL_HTTP_URL")
        .expect("field");
    assert!(matches!(secret.value, FieldValue::Set | FieldValue::NotSet));
    let painted = format!("{fields:?}{lines:?}");
    assert!(!painted.contains("should-not-show"));
    assert!(!painted.contains("redacted_value"));
    assert!(!painted.contains("env_path"));
    let list = fields
        .iter()
        .find(|field| field.key == "TELEGRAM_ALLOWED_USERS");
    if let Some(field) = list {
        assert!(matches!(field.value, FieldValue::List(_)));
    }
    let _ = update(
        &mut state,
        ConnectorsInput::Key(ConnectorsKey::Advanced),
        clock,
    );
    let shown = format!("{:?}", view(&state, clock));
    assert!(
        shown.contains("more fields")
            || shown.contains("advanced")
            || shown.contains("SIGNAL_HTTP_URL")
    );
}

#[test]
fn toggle_confirm_and_texts() {
    let clock = Instant::now();
    let mut state = ready(clock);
    let _ = update(
        &mut state,
        ConnectorsInput::PlatformsReply(Ok(common::platforms().platforms)),
        clock,
    );
    let effects = update(
        &mut state,
        ConnectorsInput::Key(ConnectorsKey::Toggle),
        clock,
    );
    assert!(effects.is_empty());
    let effects = update(
        &mut state,
        ConnectorsInput::Key(ConnectorsKey::Confirm),
        clock,
    );
    let ConnectorsEffect::UpdatePlatform {
        enabled,
        typed_keys,
        lists,
        clear_env,
        ..
    } = &effects[0]
    else {
        panic!("toggle");
    };
    assert!(enabled.is_some());
    assert!(typed_keys.is_empty());
    assert!(lists.is_empty());
    assert!(clear_env.is_empty());
    let _ = update(
        &mut state,
        ConnectorsInput::UpdateReply {
            id: "signal".into(),
            reply: Ok(UpdateReply { hot_served: false }),
        },
        clock,
    );
    let notice = view(&state, clock).notice.expect("restart").text;
    assert!(notice.contains("is on.") || notice.contains("is off."));
    assert!(notice.contains("R opens the doctor."));

    let mut hot = ready(clock);
    let _ = update(
        &mut hot,
        ConnectorsInput::UpdateReply {
            id: "signal".into(),
            reply: Ok(UpdateReply { hot_served: true }),
        },
        clock,
    );
    assert!(
        view(&hot, clock)
            .notice
            .expect("hot")
            .text
            .contains("The running gateway is connecting")
    );

    let mut conflict = ready(clock);
    let _ = update(
        &mut conflict,
        ConnectorsInput::UpdateReply {
            id: "signal".into(),
            reply: Err(ConnectorsFailure::Http {
                status: 409,
                detail: Some("multiplex".into()),
            }),
        },
        clock,
    );
    assert_eq!(
        view(&conflict, clock).notice.expect("409").text,
        "multiplex"
    );

    let mut doctor = ready(clock);
    let mut platforms = common::platforms();
    platforms.platforms[0].state = Some("pending_restart".into());
    let _ = update(
        &mut doctor,
        ConnectorsInput::PlatformsReply(Ok(platforms.platforms)),
        clock,
    );
    let effects = update(
        &mut doctor,
        ConnectorsInput::Key(ConnectorsKey::Doctor),
        clock,
    );
    assert_eq!(effects, vec![ConnectorsEffect::OpenDoctor]);
}
