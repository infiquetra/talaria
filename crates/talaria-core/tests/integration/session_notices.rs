//! Session-scoped store for Hermes `notification.show` and `notification.clear`.

use crate::drive;
use crate::events;
use crate::snapshot_json;

use serde_json::json;
use talaria_core::session::{SessionId, SessionState, update};

use events::{SESSION, ev, ev_session};

#[test]
fn same_key_replaces_the_earlier_text() {
    let (state, _effects) = drive::reduce(
        SESSION,
        vec![
            ev(
                1,
                "notification.show",
                json!({
                    "text": "first",
                    "level": "warn",
                    "kind": "sticky",
                    "key": "credits.usage",
                    "detail": "old",
                }),
            ),
            ev(
                2,
                "notification.show",
                json!({
                    "text": "second",
                    "level": "info",
                    "kind": "sticky",
                    "key": "credits.usage",
                    "detail": "new",
                }),
            ),
        ],
    );
    let row = state
        .notices()
        .by_key("credits.usage")
        .expect("credits.usage row must be stored");
    assert_eq!(row.text, "second");
    assert_eq!(row.level, "info");
    assert_eq!(row.detail.as_deref(), Some("new"));
    assert!(state.cells().is_empty());
}

#[test]
fn clear_removes_only_that_key() {
    let (mut state, _effects) = drive::reduce(
        SESSION,
        vec![
            ev(
                1,
                "notification.show",
                json!({
                    "text": "usage",
                    "level": "warn",
                    "kind": "sticky",
                    "key": "credits.usage",
                }),
            ),
            ev(
                2,
                "notification.show",
                json!({
                    "text": "sign in",
                    "level": "info",
                    "kind": "sticky",
                    "key": "login:abc",
                }),
            ),
            ev(
                3,
                "notification.show",
                json!({
                    "text": "loose",
                    "level": "info",
                    "kind": "sticky",
                }),
            ),
            ev(
                4,
                "notification.clear",
                json!({
                    "key": "credits.usage",
                }),
            ),
        ],
    );
    assert!(state.notices().by_key("credits.usage").is_none());
    assert!(state.notices().by_key("login:abc").is_some());
    assert_eq!(
        state.notices().keyless().map(|row| row.text.as_str()),
        Some("loose")
    );

    let _ = update(
        &mut state,
        ev(
            5,
            "notification.clear",
            json!({
                "key": "credits.absent",
            }),
        ),
    );
    assert!(state.notices().by_key("credits.usage").is_none());
    assert!(state.notices().by_key("login:abc").is_some());
    assert_eq!(
        state.notices().keyless().map(|row| row.text.as_str()),
        Some("loose")
    );
}

#[test]
fn timed_row_expires_when_the_caller_advances_time() {
    let (mut state, _effects) = drive::reduce(
        SESSION,
        vec![
            ev(
                1,
                "notification.show",
                json!({
                    "text": "back",
                    "level": "success",
                    "kind": "ttl",
                    "key": "credits.restored",
                    "ttl_ms": 8000,
                }),
            ),
            ev(
                2,
                "notification.show",
                json!({
                    "text": "stay",
                    "level": "warn",
                    "kind": "sticky",
                    "key": "credits.usage",
                }),
            ),
            ev(
                3,
                "notification.show",
                json!({
                    "text": "held",
                    "level": "info",
                    "kind": "ttl",
                    "key": "credits.other",
                }),
            ),
        ],
    );

    state.expire_notices(0);
    assert!(state.notices().by_key("credits.restored").is_some());

    state.expire_notices(7999);
    assert!(state.notices().by_key("credits.restored").is_some());

    state.expire_notices(8000);
    assert!(state.notices().by_key("credits.restored").is_none());
    assert!(state.notices().by_key("credits.usage").is_some());
    assert!(state.notices().by_key("credits.other").is_some());
}

#[test]
fn show_and_clear_do_not_add_a_transcript_cell() {
    let mut state = SessionState::empty(SessionId::new(SESSION));
    let show_effects = update(
        &mut state,
        ev(
            1,
            "notification.show",
            json!({
                "text": "notice text",
                "level": "info",
                "kind": "sticky",
                "key": "k1",
                "detail": "notice detail",
            }),
        ),
    );
    assert!(state.cells().is_empty());
    assert!(show_effects.is_empty());
    assert!(state.notices().by_key("k1").is_some());

    let clear_effects = update(
        &mut state,
        ev(
            2,
            "notification.clear",
            json!({
                "key": "k1",
            }),
        ),
    );
    assert!(state.cells().is_empty());
    assert!(clear_effects.is_empty());
    assert!(state.notices().by_key("k1").is_none());
}

#[test]
fn keyless_notice_replaces_the_one_slot() {
    let mut state = SessionState::empty(SessionId::new(SESSION));
    let _ = update(
        &mut state,
        ev(
            1,
            "notification.show",
            json!({
                "text": "one",
                "level": "info",
                "kind": "sticky",
            }),
        ),
    );
    let _ = update(
        &mut state,
        ev(
            2,
            "notification.show",
            json!({
                "text": "two",
                "level": "info",
                "kind": "sticky",
            }),
        ),
    );
    assert_eq!(
        state.notices().keyless().map(|row| row.text.as_str()),
        Some("two")
    );

    let _ = update(
        &mut state,
        ev_session(
            "",
            3,
            "notification.show",
            json!({
                "text": "other",
                "level": "info",
                "kind": "sticky",
            }),
        ),
    );
    assert_eq!(
        state.notices().keyless().map(|row| row.text.as_str()),
        Some("two")
    );

    let other_session = SessionState::empty(SessionId::new("other-session"));
    assert!(other_session.notices().keyless().is_none());

    let resumed =
        SessionState::from_snapshot(snapshot_json::snapshot(snapshot_json::base(SESSION)));
    assert!(resumed.notices().keyless().is_none());
}

#[test]
fn most_recent_is_the_latest_show_still_stored() {
    let mut state = SessionState::empty(SessionId::new(SESSION));
    let _ = update(
        &mut state,
        ev(
            1,
            "notification.show",
            json!({
                "text": "first a",
                "level": "info",
                "kind": "sticky",
                "key": "a",
            }),
        ),
    );
    let _ = update(
        &mut state,
        ev(
            2,
            "notification.show",
            json!({
                "text": "first b",
                "level": "info",
                "kind": "sticky",
                "key": "b",
            }),
        ),
    );
    assert_eq!(
        state
            .notices()
            .most_recent()
            .and_then(|row| row.key.as_deref()),
        Some("b")
    );

    let _ = update(
        &mut state,
        ev(
            3,
            "notification.show",
            json!({
                "text": "second a",
                "level": "info",
                "kind": "sticky",
                "key": "a",
            }),
        ),
    );
    assert_eq!(
        state
            .notices()
            .most_recent()
            .and_then(|row| row.key.as_deref()),
        Some("a")
    );

    let _ = update(
        &mut state,
        ev(
            4,
            "notification.show",
            json!({
                "text": "loose",
                "level": "info",
                "kind": "sticky",
            }),
        ),
    );
    assert_eq!(
        state.notices().most_recent().map(|row| row.text.as_str()),
        Some("loose")
    );

    let _ = update(
        &mut state,
        ev(
            5,
            "notification.clear",
            json!({
                "key": "b",
            }),
        ),
    );
    assert_eq!(
        state.notices().most_recent().map(|row| row.text.as_str()),
        Some("loose")
    );
}
