//! Link wait: polls, finished states, refusals, deadline, and events.

#![allow(clippy::too_many_lines)]

#[path = "connectors_common/mod.rs"]
mod common;

use std::time::{Duration, Instant};

use talaria_core::connectors::{
    AccountAddress, ConnectorsEffect, ConnectorsFailure, ConnectorsInput, ConnectorsKey,
    ConnectorsState, ConnectorsTab, Operation, Screen, Target, update, view,
};

fn operation(state_name: &str, settled: bool, link: Option<&str>, deadline: Instant) -> Operation {
    Operation {
        op_id: "op-test-1".into(),
        deadline,
        settled,
        targets: vec![Target {
            name: "github".into(),
            state: state_name.into(),
            detail: None,
            connect_url: link.map(str::to_owned),
        }],
    }
}

fn connect(state: &mut ConnectorsState, clock: Instant) {
    let _ = update(
        state,
        common::open_consented(ConnectorsTab::Connectors),
        clock,
    );
    let _ = update(state, ConnectorsInput::Key(ConnectorsKey::Add), clock);
    let _ = update(state, ConnectorsInput::Paste("work".into()), clock);
    let _ = update(state, ConnectorsInput::Key(ConnectorsKey::Open), clock);
}

#[test]
fn connect_then_poll_until_connected() {
    common::scrub(common::fixture("operation-open"));
    let clock = Instant::now();
    let mut state = ConnectorsState::new();
    connect(&mut state, clock);
    let effects = update(
        &mut state,
        ConnectorsInput::ConnectReply(Ok(operation(
            "initiated",
            false,
            Some("https://example.invalid/connect/test"),
            clock + Duration::from_secs(300),
        ))),
        clock,
    );
    assert!(!effects.iter().any(|effect| matches!(effect, ConnectorsEffect::PollOperation { .. })));
    assert!(effects.iter().all(|effect| !matches!(effect, ConnectorsEffect::Connect { .. })));
    let effects = update(&mut state, ConnectorsInput::Tick, clock + Duration::from_secs(2));
    assert_eq!(effects, vec![ConnectorsEffect::PollOperation { op_id: "op-test-1".into() }]);
    let effects = update(&mut state, ConnectorsInput::Tick, clock + Duration::from_secs(4));
    assert!(effects.is_empty());
    let replied = clock + Duration::from_secs(5);
    let _ = update(
        &mut state,
        ConnectorsInput::OperationReply {
            op_id: "op-test-1".into(),
            reply: Ok(operation("initiated", false, None, clock + Duration::from_secs(300))),
        },
        replied,
    );
    let effects = update(&mut state, ConnectorsInput::Tick, replied + Duration::from_secs(2));
    assert_eq!(effects, vec![ConnectorsEffect::PollOperation { op_id: "op-test-1".into() }]);
    let _ = update(
        &mut state,
        ConnectorsInput::OperationReply {
            op_id: "op-test-1".into(),
            reply: Ok(operation("connected", false, None, clock + Duration::from_secs(300))),
        },
        replied,
    );
    let notice = view(&state, replied).notice.expect("connected").text;
    assert_eq!(notice, "GitHub account work is connected.");
    let effects = update(&mut state, ConnectorsInput::Tick, replied);
    assert!(effects.iter().any(|effect| matches!(effect, ConnectorsEffect::LoadAccounts)));
    assert!(effects.iter().any(|effect| matches!(effect, ConnectorsEffect::ListConnectors)));
}

#[test]
fn settled_at_once_and_failure_states() {
    let clock = Instant::now();
    let mut settled = ConnectorsState::new();
    connect(&mut settled, clock);
    let _ = update(
        &mut settled,
        ConnectorsInput::ConnectReply(Ok(operation("initiated", true, None, clock + Duration::from_secs(30)))),
        clock,
    );
    assert!(!matches!(view(&settled, clock).screen, Screen::LinkCard { .. }));
    assert_eq!(
        view(&settled, clock).notice.expect("settled").text,
        "GitHub did not connect. Try again."
    );

    let mut failed = ConnectorsState::new();
    connect(&mut failed, clock);
    let mut op = operation("failed", false, None, clock + Duration::from_secs(30));
    op.targets[0].detail = Some("the host refused".into());
    let _ = update(&mut failed, ConnectorsInput::ConnectReply(Ok(op)), clock);
    assert_eq!(view(&failed, clock).notice.expect("detail").text, "the host refused");

    for state_name in ["not_connected", "expired", "skipped"] {
        let mut state = ConnectorsState::new();
        connect(&mut state, clock);
        let _ = update(
            &mut state,
            ConnectorsInput::ConnectReply(Ok(operation(state_name, false, None, clock + Duration::from_secs(30)))),
            clock,
        );
        assert_eq!(
            view(&state, clock).notice.expect(state_name).text,
            "GitHub did not connect. Try again."
        );
    }

    let mut empty = ConnectorsState::new();
    connect(&mut empty, clock);
    let _ = update(
        &mut empty,
        ConnectorsInput::ConnectReply(Ok(Operation {
            op_id: "op-test-1".into(),
            deadline: clock + Duration::from_secs(30),
            settled: false,
            targets: Vec::new(),
        })),
        clock,
    );
    assert!(matches!(view(&empty, clock).screen, Screen::LinkCard { starting: true, .. }));

    let mut still = ConnectorsState::new();
    connect(&mut still, clock);
    let _ = update(
        &mut still,
        ConnectorsInput::ConnectReply(Err(ConnectorsFailure::Refused {
            code: 4002,
            reason: Some("LINK_STILL_VALID".into()),
            message: "Reopen the stored link.".into(),
        })),
        clock,
    );
    assert_eq!(
        view(&still, clock).notice.expect("4002").text,
        "The last link still works. Open it from the card, or wait until it expires."
    );
}

#[test]
fn poll_errors_retry_or_end() {
    let clock = Instant::now();
    let mut state = ConnectorsState::new();
    connect(&mut state, clock);
    let _ = update(
        &mut state,
        ConnectorsInput::ConnectReply(Ok(operation(
            "initiated",
            false,
            Some("https://example.invalid/connect/test"),
            clock + Duration::from_secs(300),
        ))),
        clock,
    );
    let _ = update(
        &mut state,
        ConnectorsInput::OperationReply {
            op_id: "op-test-1".into(),
            reply: Err(ConnectorsFailure::Unknown),
        },
        clock,
    );
    let effects = update(&mut state, ConnectorsInput::Tick, clock + Duration::from_secs(2));
    assert_eq!(effects, vec![ConnectorsEffect::PollOperation { op_id: "op-test-1".into() }]);

    let _ = update(
        &mut state,
        ConnectorsInput::OperationReply {
            op_id: "op-test-1".into(),
            reply: Err(ConnectorsFailure::Refused {
                code: 5034,
                reason: Some("CONNECTOR_REQUEST_FAILED".into()),
                message: "down".into(),
            }),
        },
        clock,
    );
    let effects = update(&mut state, ConnectorsInput::Tick, clock + Duration::from_secs(4));
    assert_eq!(effects, vec![ConnectorsEffect::PollOperation { op_id: "op-test-1".into() }]);

    let _ = update(
        &mut state,
        ConnectorsInput::OperationReply {
            op_id: "op-test-1".into(),
            reply: Err(ConnectorsFailure::Refused {
                code: 4004,
                reason: Some("UNKNOWN_OPERATION".into()),
                message: "No open operation with that op_id.".into(),
            }),
        },
        clock,
    );
    assert!(view(&state, clock).notice.is_none());

    let mut text = ConnectorsState::new();
    connect(&mut text, clock);
    let _ = update(
        &mut text,
        ConnectorsInput::ConnectReply(Ok(operation("initiated", false, None, clock + Duration::from_secs(30)))),
        clock,
    );
    let _ = update(
        &mut text,
        ConnectorsInput::OperationReply {
            op_id: "op-test-1".into(),
            reply: Err(ConnectorsFailure::Refused {
                code: 4032,
                reason: Some("NEEDS_NOUS_AUTH".into()),
                message: "Sign in to use connectors.".into(),
            }),
        },
        clock,
    );
    assert!(view(&text, clock).notice.expect("4032").text.contains("hermes -p fixture auth upgrade"));
}

#[test]
fn deadline_and_escape() {
    let clock = Instant::now();
    let mut state = ConnectorsState::new();
    connect(&mut state, clock);
    let _ = update(
        &mut state,
        ConnectorsInput::ConnectReply(Ok(operation("initiated", false, None, clock))),
        clock,
    );
    let _ = update(&mut state, ConnectorsInput::Tick, clock + Duration::from_secs(2));
    let effects = update(&mut state, ConnectorsInput::Tick, clock + Duration::from_secs(3));
    assert!(effects.iter().all(|effect| !matches!(effect, ConnectorsEffect::PollOperation { .. })));
    assert_eq!(
        view(&state, clock).notice.expect("deadline").text,
        "Stopped waiting: the link expired. The list shows where the account stands."
    );

    let mut escape = ConnectorsState::new();
    connect(&mut escape, clock);
    let _ = update(
        &mut escape,
        ConnectorsInput::ConnectReply(Ok(operation(
            "initiated",
            false,
            Some("https://example.invalid/connect/test"),
            clock + Duration::from_secs(300),
        ))),
        clock,
    );
    let effects = update(&mut escape, ConnectorsInput::Key(ConnectorsKey::Close), clock);
    assert!(effects.iter().all(|effect| !matches!(
        effect,
        ConnectorsEffect::PollOperation { .. } | ConnectorsEffect::WakeOperation { .. }
    )));
    assert_eq!(
        view(&escape, clock).notice.expect("esc").text,
        "Stopped waiting. The link still works until it expires."
    );
}

#[test]
fn open_copy_wake_and_events() {
    common::scrub(common::fixture("connection-update"));
    let raw: serde_json::Value = serde_json::from_str(common::fixture("connection-update")).expect("event");
    assert!(raw.get("connect_url").is_none());
    assert!(raw["targets"][0].get("connect_url").is_none());
    assert!(raw["targets"][0].get("connection_id").is_none());

    let clock = Instant::now();
    let mut state = ConnectorsState::new();
    connect(&mut state, clock);
    let _ = update(
        &mut state,
        ConnectorsInput::ConnectReply(Ok(operation(
            "initiated",
            false,
            Some("https://example.invalid/connect/test"),
            clock + Duration::from_secs(300),
        ))),
        clock,
    );
    let effects = update(&mut state, ConnectorsInput::Key(ConnectorsKey::OpenLink), clock);
    assert_eq!(effects, vec![ConnectorsEffect::OpenUrl("https://example.invalid/connect/test".into())]);
    let effects = update(&mut state, ConnectorsInput::Key(ConnectorsKey::ReconnectOrCopy), clock);
    assert_eq!(effects, vec![ConnectorsEffect::CopyText("https://example.invalid/connect/test".into())]);
    assert_eq!(format!("{:?}", effects[0]), "CopyText([link])");

    let mut plain = ConnectorsState::new();
    connect(&mut plain, clock);
    let _ = update(
        &mut plain,
        ConnectorsInput::ConnectReply(Ok(operation(
            "initiated",
            false,
            Some("http://example.invalid/connect/test"),
            clock + Duration::from_secs(300),
        ))),
        clock,
    );
    let effects = update(&mut plain, ConnectorsInput::Key(ConnectorsKey::OpenLink), clock);
    assert!(effects.is_empty());
    assert_eq!(
        view(&plain, clock).notice.expect("scheme").text,
        "Hermes sent a link Talaria will not open."
    );

    let effects = update(&mut state, ConnectorsInput::Key(ConnectorsKey::Wake), clock);
    assert_eq!(effects, vec![ConnectorsEffect::WakeOperation { op_id: "op-test-1".into() }]);
    let _ = update(&mut state, ConnectorsInput::WakeReply(Ok(())), clock);
    let effects = update(&mut state, ConnectorsInput::Tick, clock);
    assert!(effects.iter().any(|effect| matches!(effect, ConnectorsEffect::PollOperation { .. })));

    let effects = update(
        &mut state,
        ConnectorsInput::ConnectionUpdate { op_id: "op-test-1".into(), owner_is_account: true },
        clock,
    );
    assert!(
        effects
            .iter()
            .all(|effect| !matches!(effect, ConnectorsEffect::PollOperation { .. })),
        "a poll already in flight is not started again"
    );
    let _ = update(
        &mut state,
        ConnectorsInput::OperationReply {
            op_id: "op-test-1".into(),
            reply: Ok(operation(
                "initiated",
                false,
                Some("https://example.invalid/connect/test"),
                clock + Duration::from_secs(300),
            )),
        },
        clock,
    );
    let effects = update(
        &mut state,
        ConnectorsInput::ConnectionUpdate { op_id: "op-test-1".into(), owner_is_account: true },
        clock,
    );
    assert!(effects.iter().any(|effect| matches!(effect, ConnectorsEffect::PollOperation { .. })));
    let effects = update(
        &mut state,
        ConnectorsInput::ConnectionUpdate { op_id: "op-other".into(), owner_is_account: true },
        clock,
    );
    assert!(effects.is_empty());
    let effects = update(
        &mut state,
        ConnectorsInput::ConnectionUpdate { op_id: "op-test-1".into(), owner_is_account: false },
        clock,
    );
    assert!(effects.is_empty());

    let _ = AccountAddress::Alias("work".into());
}
