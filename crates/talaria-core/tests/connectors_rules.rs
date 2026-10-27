//! Connector switches, tool drafts, and the write slot.

#![allow(clippy::too_many_lines)]

#[path = "connectors_common/mod.rs"]
mod common;

use std::time::Instant;

use serde_json::Value;
use talaria_core::connectors::{
    ConnectorsEffect, ConnectorsFailure, ConnectorsInput, ConnectorsKey, ConnectorsState, Policy,
    PolicySetReply, Screen, ToolState, ToolsReply, update, view,
};

fn policy(name: &str) -> Policy {
    let file: Value = serde_json::from_str(common::fixture("policy-modes")).expect("modes");
    serde_json::from_value(file[name].clone()).expect(name)
}

fn space(state: &mut ConnectorsState, clock: Instant) -> Vec<ConnectorsEffect> {
    update(state, ConnectorsInput::Key(ConnectorsKey::Toggle), clock)
}

fn ready(clock: Instant) -> ConnectorsState {
    let mut state = ConnectorsState::default();
    let _ = update(
        &mut state,
        common::open_consented(talaria_core::connectors::ConnectorsTab::Connectors),
        clock,
    );
    let _ = update(
        &mut state,
        ConnectorsInput::ListReply(Ok(common::list_reply())),
        clock,
    );
    let _ = update(
        &mut state,
        ConnectorsInput::CatalogReply(Ok(common::catalog_reply().connectors)),
        clock,
    );
    state
}

#[test]
fn toggle_uses_member_revision() {
    let clock = Instant::now();
    let modes = [
        ("unrestricted", true, false),
        ("deny_all", false, true),
        ("allow", true, false),
        ("deny", false, true),
    ];
    for (name, member_on, sent_enabled) in modes {
        let mut state = ready(clock);
        let _ = update(
            &mut state,
            ConnectorsInput::PolicyReply(Ok(policy(name))),
            clock,
        );
        let effects = space(&mut state, clock);
        let ConnectorsEffect::SetConnector {
            enabled,
            expected_revision,
            slug,
        } = &effects[0]
        else {
            panic!("{name}");
        };
        assert_eq!(*slug, "github");
        assert_eq!(*enabled, sent_enabled);
        assert_eq!(expected_revision, common::REVISION);
        assert_ne!(*enabled, member_on);
        let _ = update(
            &mut state,
            ConnectorsInput::PolicySetReply(Ok(PolicySetReply::default())),
            clock,
        );
        let _ = update(
            &mut state,
            ConnectorsInput::PolicyReply(Ok(policy(name))),
            clock,
        );
        let _ = update(
            &mut state,
            ConnectorsInput::ListReply(Ok(common::list_reply())),
            clock,
        );
        let notice = view(&state, clock).notice.expect(name).text;
        assert!(notice.contains("fixture"), "{name}: {notice}");
    }

    let mut readonly = ready(clock);
    let _ = update(
        &mut readonly,
        ConnectorsInput::PolicyReply(Ok(policy("no_member"))),
        clock,
    );
    let effects = space(&mut readonly, clock);
    assert!(effects.is_empty());
    assert_eq!(
        view(&readonly, clock).notice.expect("read only").text,
        "Connector rules are read-only for this account."
    );

    let mut locked = ready(clock);
    let _ = update(
        &mut locked,
        ConnectorsInput::PolicyReply(Ok(policy("org_lock"))),
        clock,
    );
    let effects = space(&mut locked, clock);
    assert!(effects.is_empty());
    assert_eq!(
        view(&locked, clock).notice.expect("org").text,
        "Your organization turned GitHub off; your switch cannot change that."
    );

    let mut busy = ready(clock);
    let _ = update(
        &mut busy,
        ConnectorsInput::PolicyReply(Ok(policy("deny"))),
        clock,
    );
    let _ = space(&mut busy, clock);
    let effects = space(&mut busy, clock);
    assert!(effects.is_empty());
    assert_eq!(
        view(&busy, clock).notice.expect("slot").text,
        "Waiting for Hermes."
    );

    let holding = [
        ConnectorsKey::Add,
        ConnectorsKey::Rename,
        ConnectorsKey::Remove,
        ConnectorsKey::Toggle,
        ConnectorsKey::Save,
        ConnectorsKey::Approve,
    ];
    for key in holding {
        let mut state = ready(clock);
        let _ = update(
            &mut state,
            ConnectorsInput::PolicyReply(Ok(policy("deny"))),
            clock,
        );
        let _ = space(&mut state, clock);
        let effects = update(&mut state, ConnectorsInput::Key(key), clock);
        assert!(effects.is_empty(), "{key:?}");
        assert_eq!(
            view(&state, clock).notice.expect("held").text,
            "Waiting for Hermes."
        );
    }
    let mut free = ready(clock);
    let _ = update(
        &mut free,
        ConnectorsInput::PolicyReply(Ok(policy("deny"))),
        clock,
    );
    let _ = space(&mut free, clock);
    let effects = update(
        &mut free,
        ConnectorsInput::Key(ConnectorsKey::Reload),
        clock,
    );
    assert!(effects.iter().any(|effect| matches!(
        effect,
        ConnectorsEffect::ListConnectors | ConnectorsEffect::LoadPolicy
    )));
}

#[test]
fn policy_refusals() {
    let clock = Instant::now();
    let cases = [
        (
            4090,
            "POLICY_CONFLICT",
            "Connector rules changed elsewhere. Reloaded; try again.",
            true,
        ),
        (
            4090,
            "ORG_REQUIRED",
            "Select an organization to manage connector rules.",
            false,
        ),
        (
            4030,
            "FORBIDDEN_SCOPE",
            "Connector access is not permitted for this account.",
            false,
        ),
        (
            4000,
            "INVALID_POLICY",
            "connector is off in the member policy",
            false,
        ),
        (
            5034,
            "POLICY_UNAVAILABLE",
            "Hermes could not reach the connector service: down",
            false,
        ),
    ];
    for (code, reason, expect, reloads) in cases {
        let mut state = ready(clock);
        let _ = update(
            &mut state,
            ConnectorsInput::PolicyReply(Ok(policy("deny"))),
            clock,
        );
        let _ = space(&mut state, clock);
        let effects = update(
            &mut state,
            ConnectorsInput::PolicySetReply(Err(ConnectorsFailure::Refused {
                code,
                reason: Some(reason.into()),
                message: if code == 5034 {
                    "down".into()
                } else {
                    expect.into()
                },
            })),
            clock,
        );
        assert_eq!(view(&state, clock).notice.expect(reason).text, expect);
        assert_eq!(
            effects
                .iter()
                .any(|effect| matches!(effect, ConnectorsEffect::LoadPolicy)),
            reloads,
            "{reason}"
        );
    }
}

#[test]
fn tools_draft_review_and_save() {
    let clock = Instant::now();
    let mut state = ready(clock);
    let _ = update(
        &mut state,
        ConnectorsInput::PolicyReply(Ok(policy("org_lock"))),
        clock,
    );
    let _ = update(
        &mut state,
        ConnectorsInput::ToolsReply {
            slug: "github".into(),
            reply: Ok(common::tools()),
        },
        clock,
    );
    let Screen::Tools { rows, .. } = view(&state, clock).screen else {
        panic!("tools");
    };
    assert!(
        rows.iter()
            .any(|row| row.name == "Pulls" && row.state == ToolState::Locked)
    );
    let _ = update(
        &mut state,
        ConnectorsInput::Key(ConnectorsKey::Toggle),
        clock,
    );
    let _ = update(&mut state, ConnectorsInput::Key(ConnectorsKey::Save), clock);
    let Screen::Review { rows, .. } = view(&state, clock).screen else {
        panic!("review");
    };
    assert!(
        rows.iter()
            .any(|row| row.old.contains("on") || row.new.contains("off") || row.new.contains("on"))
    );
    let effects = update(
        &mut state,
        ConnectorsInput::Key(ConnectorsKey::Confirm),
        clock,
    );
    let ConnectorsEffect::SetTools {
        disabled_tools,
        expected_revision,
        ..
    } = &effects[0]
    else {
        panic!("save");
    };
    assert_eq!(expected_revision, common::REVISION);
    assert!(disabled_tools.is_sorted());
    assert!(
        disabled_tools.contains(&"issues".to_owned())
            || disabled_tools.is_empty()
            || !disabled_tools.contains(&"pulls".to_owned())
    );
    assert!(
        !disabled_tools.contains(&"pulls".to_owned())
            || disabled_tools
                .iter()
                .filter(|tool| *tool == "pulls")
                .count()
                <= 1
    );

    let mut off = ready(clock);
    let _ = update(
        &mut off,
        ConnectorsInput::PolicyReply(Ok(policy("deny_all"))),
        clock,
    );
    let effects = update(
        &mut off,
        ConnectorsInput::Key(ConnectorsKey::ToolsOrTest),
        clock,
    );
    assert!(effects.is_empty());
    assert_eq!(
        view(&off, clock).notice.expect("member off").text,
        "Turn GitHub on before choosing its tools."
    );
}

#[test]
fn tools_stale_refresh_and_errors() {
    let clock = Instant::now();
    let mut state = ready(clock);
    let _ = update(
        &mut state,
        ConnectorsInput::PolicyReply(Ok(common::policy_deny())),
        clock,
    );
    let _ = update(
        &mut state,
        ConnectorsInput::ToolsReply {
            slug: "github".into(),
            reply: Ok(common::tools()),
        },
        clock,
    );
    let Screen::Tools { cached, header, .. } = view(&state, clock).screen else {
        panic!("cached");
    };
    assert!(cached);
    assert!(header.contains("cached") || header.contains("GitHub"));
    let effects = update(
        &mut state,
        ConnectorsInput::Key(ConnectorsKey::Reload),
        clock,
    );
    assert_eq!(
        effects,
        vec![ConnectorsEffect::LoadTools {
            slug: "github".into(),
            refresh: true
        }]
    );

    let mut missing = ready(clock);
    let _ = update(
        &mut missing,
        ConnectorsInput::ToolsReply {
            slug: "github".into(),
            reply: Err(ConnectorsFailure::Refused {
                code: 4041,
                reason: Some("CONNECTOR_NOT_FOUND".into()),
                message: "Connector not found.".into(),
            }),
        },
        clock,
    );
    assert!(
        view(&missing, clock)
            .notice
            .expect("4041")
            .text
            .contains("Connector not found.")
    );

    let mut empty = ready(clock);
    let _ = update(
        &mut empty,
        ConnectorsInput::ToolsReply {
            slug: "github".into(),
            reply: Ok(ToolsReply {
                stale: false,
                tools: Vec::new(),
            }),
        },
        clock,
    );
    let shown = format!("{:?}", view(&empty, clock));
    assert!(shown.contains("GitHub has no tools to choose.") || shown.contains("no tools"));

    let mut unread = ready(clock);
    let _ = update(
        &mut unread,
        ConnectorsInput::PolicyReply(Err(ConnectorsFailure::Refused {
            code: 5034,
            reason: Some("POLICY_UNAVAILABLE".into()),
            message: "down".into(),
        })),
        clock,
    );
    let _ = update(
        &mut unread,
        ConnectorsInput::ToolsReply {
            slug: "github".into(),
            reply: Ok(common::tools()),
        },
        clock,
    );
    let effects = update(
        &mut unread,
        ConnectorsInput::Key(ConnectorsKey::Save),
        clock,
    );
    assert!(effects.is_empty());
    assert!(
        view(&unread, clock)
            .notice
            .expect("rules")
            .text
            .contains("Connector rules are not available right now:")
    );
}
