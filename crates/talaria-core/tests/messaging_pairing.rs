//! Pairing approval, revoke, and change signals.

#![allow(clippy::too_many_lines)]

#[path = "connectors_common/mod.rs"]
mod common;

use std::time::{Duration, Instant};

use talaria_core::connectors::{
    ConnectorsEffect, ConnectorsFailure, ConnectorsInput, ConnectorsKey, ConnectorsState,
    ConnectorsTab, Drafts, update, view,
};

fn messaging(state: &mut ConnectorsState, clock: Instant) {
    let _ = update(
        state,
        ConnectorsInput::Open {
            open: talaria_core::connectors::ConnectorsOpen::Tab(ConnectorsTab::Messaging),
            profile: common::PROFILE.into(),
            consented: true,
        },
        clock,
    );
    let _ = update(
        state,
        ConnectorsInput::PlatformsReply(Ok(common::platforms().platforms)),
        clock,
    );
    let _ = update(
        state,
        ConnectorsInput::PairingReply(Ok(common::pairing())),
        clock,
    );
}

/// Platforms are Signal, then Telegram. Telegram's card lists its field, then the waiting row.
fn select_pending(state: &mut ConnectorsState, clock: Instant) {
    let _ = update(state, ConnectorsInput::Key(ConnectorsKey::Down), clock);
    let _ = update(state, ConnectorsInput::Key(ConnectorsKey::Open), clock);
    let _ = update(state, ConnectorsInput::Key(ConnectorsKey::Down), clock);
}

fn select_paired(state: &mut ConnectorsState, clock: Instant) {
    select_pending(state, clock);
    let _ = update(state, ConnectorsInput::Key(ConnectorsKey::Down), clock);
}

#[test]
fn approve_and_revoke() {
    let clock = Instant::now();
    let mut state = ConnectorsState::new();
    messaging(&mut state, clock);
    select_pending(&mut state, clock);
    let effects = update(
        &mut state,
        ConnectorsInput::Key(ConnectorsKey::Approve),
        clock,
    );
    assert!(effects.is_empty());
    let effects = update(
        &mut state,
        ConnectorsInput::Key(ConnectorsKey::Confirm),
        clock,
    );
    assert!(matches!(
        &effects[0],
        ConnectorsEffect::ApprovePairing { platform, request_id } if platform == "telegram" && request_id == "req-test-1"
    ));
    let _ = update(&mut state, ConnectorsInput::ApproveReply(Ok(())), clock);
    let notice = view(&state, clock).notice.expect("approved").text;
    assert_eq!(notice, "Pairing approved on Telegram.");
    assert!(!notice.contains("pair-pending"));
    assert!(!notice.contains("user-test-1"));

    let mut empty = ConnectorsState::new();
    messaging(&mut empty, clock);
    let mut pairing = common::pairing();
    pairing.pending[0].request_id.clear();
    let _ = update(
        &mut empty,
        ConnectorsInput::PairingReply(Ok(pairing)),
        clock,
    );
    select_pending(&mut empty, clock);
    let effects = update(
        &mut empty,
        ConnectorsInput::Key(ConnectorsKey::Approve),
        clock,
    );
    assert!(effects.is_empty() || matches!(effects[0], ConnectorsEffect::CopyText(_)));
    let shown = format!("{:?}", view(&empty, clock));
    assert!(shown.contains("hermes -p fixture pairing approve telegram <code>"));

    let mut revoke = ConnectorsState::new();
    messaging(&mut revoke, clock);
    select_paired(&mut revoke, clock);
    let _ = update(
        &mut revoke,
        ConnectorsInput::Key(ConnectorsKey::Remove),
        clock,
    );
    let effects = update(
        &mut revoke,
        ConnectorsInput::Key(ConnectorsKey::Confirm),
        clock,
    );
    assert!(matches!(
        effects.first(),
        Some(ConnectorsEffect::RevokePairing { .. })
    ));
    let _ = update(&mut revoke, ConnectorsInput::RevokeReply(Ok(())), clock);
    let notice = view(&revoke, clock).notice.expect("revoked").text;
    assert!(notice.contains("Other access rules may still"));
    assert!(!notice.contains("pair-approved"));
    assert!(!notice.contains("user-test-2"));

    let mut gone = ConnectorsState::new();
    messaging(&mut gone, clock);
    let _ = update(
        &mut gone,
        ConnectorsInput::ApproveReply(Err(ConnectorsFailure::Http {
            status: 404,
            detail: None,
        })),
        clock,
    );
    assert_eq!(
        view(&gone, clock).notice.expect("404").text,
        "That pairing request is gone or expired. Reloaded."
    );
    let mut limited = ConnectorsState::new();
    messaging(&mut limited, clock);
    let _ = update(
        &mut limited,
        ConnectorsInput::ApproveReply(Err(ConnectorsFailure::Http {
            status: 429,
            detail: Some("slow down".into()),
        })),
        clock,
    );
    assert_eq!(view(&limited, clock).notice.expect("429").text, "slow down");
    let mut bad = ConnectorsState::new();
    messaging(&mut bad, clock);
    let _ = update(
        &mut bad,
        ConnectorsInput::RevokeReply(Err(ConnectorsFailure::Http {
            status: 400,
            detail: Some("missing user".into()),
        })),
        clock,
    );
    assert_eq!(view(&bad, clock).notice.expect("400").text, "missing user");
    let _ = Drafts::default();
}

#[test]
fn change_events_refetch_coalesced() {
    let clock = Instant::now();
    let mut state = ConnectorsState::new();
    messaging(&mut state, clock);
    let _ = update(
        &mut state,
        ConnectorsInput::SecretEdited {
            key: "SIGNAL_HTTP_URL".into(),
            empty: false,
        },
        clock,
    );
    let effects = update(&mut state, ConnectorsInput::PairingChanged, clock);
    assert_eq!(
        effects,
        vec![
            ConnectorsEffect::LoadPairing,
            ConnectorsEffect::LoadPlatforms
        ]
    );
    let effects = update(&mut state, ConnectorsInput::PlatformsChanged, clock);
    assert!(effects.is_empty(), "inside the two second window");
    let effects = update(
        &mut state,
        ConnectorsInput::PlatformsChanged,
        clock + Duration::from_secs(2),
    );
    assert_eq!(effects, vec![ConnectorsEffect::LoadPlatforms]);
    assert!(matches!(
        view(&state, clock).screen,
        talaria_core::connectors::Screen::PlatformCard { .. }
            | talaria_core::connectors::Screen::PlatformList { .. }
            | talaria_core::connectors::Screen::Review { .. }
    ));

    let mut connectors = ConnectorsState::new();
    let _ = update(
        &mut connectors,
        ConnectorsInput::Open {
            open: talaria_core::connectors::ConnectorsOpen::Tab(ConnectorsTab::Connectors),
            profile: common::PROFILE.into(),
            consented: true,
        },
        clock,
    );
    assert!(update(&mut connectors, ConnectorsInput::PairingChanged, clock).is_empty());
    assert!(update(&mut connectors, ConnectorsInput::PlatformsChanged, clock).is_empty());

    let mut writing = ConnectorsState::new();
    messaging(&mut writing, clock);
    let _ = update(
        &mut writing,
        ConnectorsInput::Key(ConnectorsKey::Toggle),
        clock,
    );
    let _ = update(
        &mut writing,
        ConnectorsInput::Key(ConnectorsKey::Confirm),
        clock,
    );
    assert!(update(&mut writing, ConnectorsInput::PairingChanged, clock).is_empty());
}
