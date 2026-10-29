//! Messaging drafts, list parsing, save failures, and test replies.

#![allow(clippy::too_many_lines)]

#[path = "connectors_common/mod.rs"]
mod common;

use std::time::Instant;

use talaria_core::connectors::{
    ConnectorsEffect, ConnectorsFailure, ConnectorsInput, ConnectorsKey, ConnectorsState, Drafts,
    PlatformRow, Screen, parse_list, review_rows, update, view,
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

fn signal() -> PlatformRow {
    common::platforms()
        .platforms
        .into_iter()
        .find(|row| row.id == "signal")
        .expect("signal")
}

#[test]
fn drafts_review_and_save() {
    let clock = Instant::now();
    let mut state = ready(clock);
    let _ = update(
        &mut state,
        ConnectorsInput::PlatformsReply(Ok(common::platforms().platforms)),
        clock,
    );
    let _ = update(&mut state, ConnectorsInput::Key(ConnectorsKey::Open), clock);
    let _ = update(
        &mut state,
        ConnectorsInput::SecretEdited {
            key: "SIGNAL_HTTP_URL".into(),
            empty: false,
        },
        clock,
    );
    let _ = update(&mut state, ConnectorsInput::Key(ConnectorsKey::Save), clock);
    let Screen::Review { title, rows } = view(&state, clock).screen else {
        panic!("review");
    };
    assert!(title.contains("fixture"));
    assert!(
        rows.iter()
            .any(|row| row.new.contains("new value (hidden)"))
    );
    let _ = update(&mut state, ConnectorsInput::Key(ConnectorsKey::Down), clock);
    assert_eq!(view(&state, clock).review_scroll, 1);
    let _ = update(&mut state, ConnectorsInput::Key(ConnectorsKey::Up), clock);
    assert_eq!(view(&state, clock).review_scroll, 0);
    let effects = update(
        &mut state,
        ConnectorsInput::Key(ConnectorsKey::Cancel),
        clock,
    );
    assert!(effects.is_empty());
    let effects = update(&mut state, ConnectorsInput::Key(ConnectorsKey::Save), clock);
    assert!(matches!(view(&state, clock).screen, Screen::Review { .. }));
    let _ = effects;
    let effects = update(
        &mut state,
        ConnectorsInput::Key(ConnectorsKey::Confirm),
        clock,
    );
    let ConnectorsEffect::UpdatePlatform { typed_keys, .. } = &effects[0] else {
        panic!("save");
    };
    assert_eq!(typed_keys, &vec!["SIGNAL_HTTP_URL".to_owned()]);
    assert!(!format!("{:?}", effects[0]).contains("sv-sentinel-7c1e"));

    let mut refused = ready(clock);
    let _ = update(
        &mut refused,
        ConnectorsInput::PlatformsReply(Ok(vec![signal()])),
        clock,
    );
    let _ = update(
        &mut refused,
        ConnectorsInput::Key(ConnectorsKey::Open),
        clock,
    );
    let _ = update(
        &mut refused,
        ConnectorsInput::SecretEdited {
            key: "SIGNAL_HTTP_URL".into(),
            empty: false,
        },
        clock,
    );
    let _ = update(
        &mut refused,
        ConnectorsInput::Key(ConnectorsKey::Save),
        clock,
    );
    let effects = update(
        &mut refused,
        ConnectorsInput::Key(ConnectorsKey::Confirm),
        clock,
    );
    assert!(matches!(
        &effects[..],
        [ConnectorsEffect::UpdatePlatform { id, typed_keys, .. }]
            if id == "signal" && typed_keys == &["SIGNAL_HTTP_URL"]
    ));
    let _ = update(
        &mut refused,
        ConnectorsInput::UpdateReply {
            id: "signal".into(),
            reply: Err(ConnectorsFailure::NotSent("consent not given".into())),
        },
        clock,
    );
    assert!(matches!(
        view(&refused, clock).screen,
        Screen::Review { .. } | Screen::PlatformCard { .. }
    ));
    let _ = update(
        &mut refused,
        ConnectorsInput::Key(ConnectorsKey::Save),
        clock,
    );
    let effects = update(
        &mut refused,
        ConnectorsInput::Key(ConnectorsKey::Confirm),
        clock,
    );
    assert!(matches!(
        &effects[..],
        [ConnectorsEffect::UpdatePlatform { id, typed_keys, .. }]
            if id == "signal" && typed_keys == &["SIGNAL_HTTP_URL"]
    ));
    let _ = update(
        &mut refused,
        ConnectorsInput::UpdateReply {
            id: "signal".into(),
            reply: Err(ConnectorsFailure::Unknown),
        },
        clock,
    );
    assert_eq!(
        view(&refused, clock).notice.expect("unknown").text,
        "No answer from Hermes; the change may or may not have been made. Reloading."
    );

    let mut escape = ready(clock);
    let _ = update(
        &mut escape,
        ConnectorsInput::PlatformsReply(Ok(vec![signal()])),
        clock,
    );
    let _ = update(
        &mut escape,
        ConnectorsInput::Key(ConnectorsKey::Open),
        clock,
    );
    let _ = update(
        &mut escape,
        ConnectorsInput::SecretEdited {
            key: "SIGNAL_HTTP_URL".into(),
            empty: false,
        },
        clock,
    );
    let effects = update(
        &mut escape,
        ConnectorsInput::Key(ConnectorsKey::Close),
        clock,
    );
    assert!(effects.is_empty() || matches!(view(&escape, clock).screen, Screen::Confirm { .. }));
}

#[test]
fn parse_list_rules() {
    assert_eq!(
        parse_list("alpha, beta"),
        vec!["alpha".to_owned(), "beta".to_owned()]
    );
    assert_eq!(
        parse_list("alpha\nbeta"),
        vec!["alpha".to_owned(), "beta".to_owned()]
    );
    assert_eq!(
        parse_list("  alpha , , beta "),
        vec!["alpha".to_owned(), "beta".to_owned()]
    );
    assert_eq!(
        parse_list("alpha, alpha, beta"),
        vec!["alpha".to_owned(), "beta".to_owned()]
    );
    assert!(parse_list("  , \n ").is_empty());

    let platform = common::platforms()
        .platforms
        .into_iter()
        .find(|row| row.id == "telegram")
        .expect("telegram");
    let mut drafts = Drafts::default();
    drafts
        .lists
        .insert("TELEGRAM_ALLOWED_USERS".into(), Vec::new());
    drafts.clear.insert("TELEGRAM_ALLOWED_USERS".into());
    let rows = review_rows(&platform, &drafts);
    assert!(
        rows.iter()
            .any(|row| row.new.contains("cleared") || row.key == "TELEGRAM_ALLOWED_USERS")
    );
}

#[test]
fn save_errors_map_to_text() {
    let clock = Instant::now();
    let cases = [
        (ConnectorsFailure::Http { status: 400, detail: Some("Refusing to save a redacted credential preview; re-enter the full secret to replace it.".into()) }, "Refusing to save a redacted credential preview; re-enter the full secret to replace it."),
        (ConnectorsFailure::Http { status: 400, detail: Some("SIGNAL_HTTP_URL is not configurable for Signal".into()) }, "SIGNAL_HTTP_URL is not configurable for Signal"),
        (ConnectorsFailure::Http { status: 401, detail: None }, "Hermes refused Talaria's ticket. Run /doctor."),
        (ConnectorsFailure::Http { status: 403, detail: Some("profile_scope_mismatch".into()) }, "Hermes refused: the request named another profile."),
        (ConnectorsFailure::Http { status: 404, detail: None }, "Hermes does not know signal. ctrl+r reloads."),
        (ConnectorsFailure::Http { status: 500, detail: None }, "Hermes reported an error (HTTP 500). Reloading to show what was saved."),
        (ConnectorsFailure::Unknown, "No answer from Hermes; the change may or may not have been made. Reloading."),
    ];
    for (failure, expect) in cases {
        let mut state = ready(clock);
        let _ = update(
            &mut state,
            ConnectorsInput::UpdateReply {
                id: "signal".into(),
                reply: Err(failure),
            },
            clock,
        );
        assert_eq!(view(&state, clock).notice.expect("save").text, expect);
    }
}

#[test]
fn test_reply_texts() {
    let clock = Instant::now();
    let cases = [
        (true, Some("allowlist"), "Test: hello", false),
        (true, Some("pairing"), "Test: hello", false),
        (true, Some("deny_all"), "Test: hello", true),
        (
            true,
            Some("open"),
            "Test: hello (anyone can message the bot)",
            false,
        ),
        (true, None, "Test: hello", false),
        (false, None, "Test: no", true),
    ];
    for (ok, posture, expect, _) in cases {
        let mut state = ready(clock);
        let _ = update(
            &mut state,
            ConnectorsInput::TestReply {
                id: "signal".into(),
                reply: Ok(talaria_core::connectors::TestReply {
                    ok,
                    state: "connected".into(),
                    message: if ok { "hello".into() } else { "no".into() },
                    auth_posture: posture.map(str::to_owned),
                }),
            },
            clock,
        );
        let notice = view(&state, clock).notice.expect("test");
        assert_eq!(notice.text, expect);
    }
}
