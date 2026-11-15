use hermes_client::replay::Cursor;
use hermes_contract::events::EventFrame;
use serde_json::json;
use talaria_app::app::{AppEffect, AppMessage, AppState, UserAction, update};
use talaria_app::sessions::{OpenedSession, SessionInfoSummary};
use talaria_core::session::{
    AssistantCell, Cell, CellId, Event, SessionId, SessionState, update as session_update,
};

fn opened_state(session_id: &str) -> AppState {
    let mut state = AppState::new("launch");
    state.opened = Some(OpenedSession {
        state: SessionState::empty(SessionId::new(session_id)),
        session_id: session_id.into(),
        stored_session_id: String::new(),
        subscription_id: String::new(),
        info: SessionInfoSummary {
            model: None,
            provider: None,
            cwd: None,
            branch: None,
        },
        title: None,
        cursor: Cursor {
            replay_epoch: String::new(),
            last_sequence: 0,
        },
        revision: 0,
        execution_generation: 0,
        launch_notices: Vec::new(),
        effort: None,
    });
    state
}

fn has_notice(effects: &[AppEffect], expected: &str) -> bool {
    effects.iter().any(|effect| {
        matches!(
            effect,
            AppEffect::CommitLines(lines) if lines.iter().any(|line| line.contains(expected))
        )
    })
}

fn has_copy_reply(effects: &[AppEffect], expected: Option<&str>) -> bool {
    effects.iter().any(|effect| match effect {
        AppEffect::CopyReply { text } => expected.is_none_or(|want| text == want),
        _ => false,
    })
}

#[test]
fn copy_emits_clipboard_text_and_a_notice() {
    let mut state = opened_state("s");
    state
        .opened
        .as_mut()
        .expect("opened")
        .state
        .insert_final(Cell::Assistant(AssistantCell {
            id: CellId::new(1),
            text: "hello".into(),
            sealed: true,
            interim: false,
            transformed: false,
        }));

    let effects = update(
        &mut state,
        AppMessage::User(UserAction::Submit("/copy".into())),
    );
    assert!(
        has_notice(
            &effects,
            "Copy requested: reply 1, 5 characters. Your terminal must allow clipboard access."
        ),
        "expected copy success notice"
    );
    assert!(
        has_copy_reply(&effects, Some("hello")),
        "expected CopyReply with reply text"
    );
    assert!(
        !effects
            .iter()
            .any(|effect| matches!(effect, AppEffect::RunCommand { .. })),
        "/copy must not emit RunCommand"
    );

    let miss = update(
        &mut state,
        AppMessage::User(UserAction::Submit("/copy 2".into())),
    );
    assert!(
        has_notice(&miss, "No reply 2 to copy."),
        "expected missing reply 2 notice"
    );
    assert!(
        !has_copy_reply(&miss, None),
        "/copy 2 must not emit CopyReply"
    );

    let mut closed = AppState::new("launch");
    let no_session = update(
        &mut closed,
        AppMessage::User(UserAction::Submit("/copy".into())),
    );
    assert!(
        has_notice(&no_session, "No session is open."),
        "expected no session notice"
    );
    assert!(
        !has_copy_reply(&no_session, None),
        "closed session must not emit CopyReply"
    );

    let bad = update(
        &mut state,
        AppMessage::User(UserAction::Submit("/copy no".into())),
    );
    assert!(
        has_notice(&bad, "Usage: /copy [N]"),
        "expected usage notice for /copy no"
    );
    assert!(
        !has_copy_reply(&bad, None),
        "/copy no must not emit CopyReply"
    );

    let huge = update(
        &mut state,
        AppMessage::User(UserAction::Submit("/copy 4294967296".into())),
    );
    assert!(
        has_notice(&huge, "No reply 4294967296 to copy."),
        "expected missing reply notice for u32 overflow"
    );
    assert!(
        !has_copy_reply(&huge, None),
        "/copy 4294967296 must not emit CopyReply"
    );

    let huge_closed = update(
        &mut closed,
        AppMessage::User(UserAction::Submit("/copy 4294967296".into())),
    );
    assert!(
        has_notice(&huge_closed, "No session is open."),
        "expected no session notice before u32 check"
    );
    assert!(
        !has_copy_reply(&huge_closed, None),
        "closed session /copy 4294967296 must not emit CopyReply"
    );

    let mut exact = opened_state("s");
    let max_text = "a".repeat(100_000);
    exact
        .opened
        .as_mut()
        .expect("opened")
        .state
        .insert_final(Cell::Assistant(AssistantCell {
            id: CellId::new(1),
            text: max_text.clone(),
            sealed: true,
            interim: false,
            transformed: false,
        }));
    let ok_exact = update(
        &mut exact,
        AppMessage::User(UserAction::Submit("/copy".into())),
    );
    assert!(
        has_notice(
            &ok_exact,
            "Copy requested: reply 1, 100000 characters. Your terminal must allow clipboard access."
        ),
        "expected copy success notice for 100000 chars"
    );
    assert!(
        has_copy_reply(&ok_exact, Some(&max_text)),
        "expected CopyReply for 100000 chars"
    );

    let mut over = opened_state("s");
    over.opened
        .as_mut()
        .expect("opened")
        .state
        .insert_final(Cell::Assistant(AssistantCell {
            id: CellId::new(1),
            text: "a".repeat(100_001),
            sealed: true,
            interim: false,
            transformed: false,
        }));
    let too_long = update(
        &mut over,
        AppMessage::User(UserAction::Submit("/copy".into())),
    );
    assert!(
        has_notice(
            &too_long,
            "Reply 1 is too long to copy (100001 characters). Use /export."
        ),
        "expected too-long notice for 100001 chars"
    );
    assert!(
        !has_copy_reply(&too_long, None),
        "100001 chars must not emit CopyReply"
    );
}

fn show_event(session_id: &str, seq: u64, key: &str, text: &str) -> Event {
    let raw = json!({
        "type": "notification.show",
        "session_id": session_id,
        "seq": seq,
        "replay_epoch": "epoch-1",
        "authority_epoch": 1,
        "execution_generation": 1,
        "payload": {
            "text": text,
            "level": "info",
            "kind": "sticky",
            "key": key,
        }
    });
    let frame = EventFrame::from_params(&raw).expect("valid notification.show frame");
    Event::Session {
        session_id: SessionId::new(session_id),
        frame,
        raw,
    }
}

#[test]
fn copy_link_requests_the_sign_in_text() {
    let mut state = opened_state("s");
    {
        let session = &mut state.opened.as_mut().expect("opened").state;
        let _ = session_update(
            session,
            show_event("s", 1, "login:aaaa0001", "https://example.test/device"),
        );
        let _ = session_update(session, show_event("s", 2, "login:aaaa0002", "ABCD-EFGH"));
        let _ = session_update(
            session,
            show_event(
                "s",
                3,
                "login:aaaa0003",
                "Do not share this code. Waiting for sign-in, up to 15.",
            ),
        );
    }

    let pair = update(
        &mut state,
        AppMessage::User(UserAction::Submit("/copy link".into())),
    );
    assert!(
        has_copy_reply(&pair, Some("https://example.test/device\nABCD-EFGH")),
        "expected CopyReply with link and code"
    );
    assert!(
        has_notice(
            &pair,
            "Copy requested: sign-in link and code. Your terminal must allow clipboard access."
        ),
        "expected link+code notice"
    );
    assert!(
        !pair
            .iter()
            .any(|effect| matches!(effect, AppEffect::RunCommand { .. })),
        "/copy link must not emit RunCommand"
    );

    let mut link_only_state = opened_state("s");
    let _ = session_update(
        &mut link_only_state.opened.as_mut().expect("opened").state,
        show_event("s", 1, "login:aaaa0001", "https://example.test/device"),
    );
    let link_only = update(
        &mut link_only_state,
        AppMessage::User(UserAction::Submit("/copy link".into())),
    );
    assert!(
        has_copy_reply(&link_only, Some("https://example.test/device")),
        "expected CopyReply with link alone"
    );
    assert!(
        has_notice(
            &link_only,
            "Copy requested: sign-in link. Your terminal must allow clipboard access."
        ),
        "expected link-only notice"
    );

    let mut empty_state = opened_state("s");
    let empty = update(
        &mut empty_state,
        AppMessage::User(UserAction::Submit("/copy link".into())),
    );
    assert!(
        has_notice(&empty, "No sign-in link to copy."),
        "expected missing link notice"
    );
    assert!(
        !has_copy_reply(&empty, None),
        "empty state must not emit CopyReply"
    );

    let mut other_notice_state = opened_state("s");
    let _ = session_update(
        &mut other_notice_state.opened.as_mut().expect("opened").state,
        show_event("s", 1, "sessions.auto_prune", "disk"),
    );
    let other = update(
        &mut other_notice_state,
        AppMessage::User(UserAction::Submit("/copy link".into())),
    );
    assert!(
        has_notice(&other, "No sign-in link to copy."),
        "expected missing link notice for non-login notice"
    );
    assert!(
        !has_copy_reply(&other, None),
        "non-login notice must not emit CopyReply"
    );
    assert!(
        !has_notice(&other, "disk"),
        "non-login notice must not print disk"
    );
}
