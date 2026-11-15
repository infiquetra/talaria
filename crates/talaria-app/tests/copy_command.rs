use hermes_client::replay::Cursor;
use talaria_app::app::{AppEffect, AppMessage, AppState, UserAction, update};
use talaria_app::sessions::{OpenedSession, SessionInfoSummary};
use talaria_core::session::{AssistantCell, Cell, CellId, SessionId, SessionState};

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

    let link = update(
        &mut state,
        AppMessage::User(UserAction::Submit("/copy link".into())),
    );
    assert!(
        has_notice(&link, "Usage: /copy [N]"),
        "expected usage notice for /copy link"
    );
    assert!(
        !has_copy_reply(&link, None),
        "/copy link must not emit CopyReply"
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
