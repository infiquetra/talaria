//! Drafting red tests for the review. Issue #245.
//!
//! The success reply comes from a frame recording, served by the mock gateway.
//! A missing recording fails `BLOCKED`, never silently.

#[path = "support/mock_gateway.rs"]
mod mock_gateway;

use std::collections::BTreeSet;
use std::path::PathBuf;

use mock_gateway::{Behavior, StandaloneGateway};
use serde_json::Value;
use talaria_app::bg::{LaunchConn, LaunchLink};
use talaria_app::review::draft::{DraftError, draft_message};
use talaria_core::directory::rows::RowKey;
use talaria_core::review::{ReviewInput, ReviewKey, ReviewState, ReviewTarget};

fn frames_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/frames")
}

fn row() -> RowKey {
    RowKey {
        profile: "fixture".to_owned(),
        session_id: "local-draft".to_owned(),
    }
}

/// The recorded draft text.
fn recorded_text() -> String {
    let path = frames_dir().join("review-oneshot.jsonl");
    let Ok(text) = std::fs::read_to_string(&path) else {
        panic!("BLOCKED: fixtures/frames/review-oneshot.jsonl not recorded");
    };
    // The log opens with a header, then holds every frame of the connection. The draft is
    // the reply that carries a `text`.
    text.lines()
        .filter_map(|line| serde_json::from_str::<Value>(line).ok())
        .find_map(|entry| entry["frame"]["result"]["text"].as_str().map(str::to_owned))
        .expect("recorded text")
}

/// Parameters, the 12,000-character cut, and every error mapping.
#[tokio::test]
async fn oneshot_params_and_error_mapping() {
    let text = recorded_text();
    let home = tempfile::tempdir().expect("home");
    let gateway = StandaloneGateway::start(home.path());
    let link = LaunchLink::open(home.path(), "fixture")
        .await
        .expect("link");
    let conn = LaunchConn::Link(link);
    gateway.set_oneshot_text(&text);

    let draft = draft_message(&conn, &row(), "diff --git a/a.txt", "seed", "")
        .await
        .expect("the draft arrives");
    assert_eq!(draft, text.trim(), "the recorded text arrives trimmed");

    let seen = gateway.requests("llm.oneshot");
    assert_eq!(seen.len(), 1, "one draft call");
    let params = &seen[0].params;
    let keys: BTreeSet<String> = params
        .as_object()
        .expect("object params")
        .keys()
        .cloned()
        .collect();
    assert!(
        keys == BTreeSet::from(["session_id", "template", "variables"].map(str::to_owned))
            || keys
                == BTreeSet::from(
                    ["session_id", "template", "temperature", "variables"].map(str::to_owned)
                )
            || keys
                == BTreeSet::from(
                    [
                        "session_id",
                        "template",
                        "temperature",
                        "variables",
                        "profile",
                    ]
                    .map(str::to_owned)
                ),
        "closed keys: {keys:?}"
    );
    assert_eq!(params["session_id"], "local-draft");
    assert_eq!(params["template"], "commit_message");
    assert_eq!(params["temperature"], 0.8);
    assert_eq!(params["variables"]["recent_commits"], "seed");
    assert_eq!(params["variables"]["avoid"], "");
    gateway.assert_no_violations();

    // A long diff is cut to 12,000 characters before sending.
    let long = "x".repeat(20_000);
    let _ = draft_message(&conn, &row(), &long, "seed", "")
        .await
        .expect("the long draft arrives");
    let seen = gateway.requests("llm.oneshot");
    let sent = seen[1].params["variables"]["diff"]
        .as_str()
        .expect("sent diff");
    assert!(
        sent.chars().count() <= 12_000,
        "the diff is cut: {} characters",
        sent.chars().count()
    );

    // Every error mapping.
    let errors: Vec<(Behavior, DraftError)> = vec![
        (
            Behavior::RefuseCode {
                reason: "not_found".to_owned(),
                code: 4001,
            },
            DraftError::SessionGone,
        ),
        (
            Behavior::RefuseCode {
                reason: "oneshot_busy".to_owned(),
                code: 4001,
            },
            DraftError::Busy,
        ),
        (
            Behavior::RefuseCode {
                reason: "runtime_draining".to_owned(),
                code: 4001,
            },
            DraftError::Draining,
        ),
        (
            Behavior::FailCode {
                code: -32601,
                message: "unknown method".to_owned(),
                reason: None,
            },
            DraftError::Unsupported,
        ),
        (
            Behavior::RefuseCode {
                reason: "permission_denied".to_owned(),
                code: 4001,
            },
            DraftError::Failed("permission_denied".to_owned()),
        ),
        (
            Behavior::RefuseCode {
                reason: "profile_mismatch".to_owned(),
                code: 4001,
            },
            DraftError::Failed("profile_mismatch".to_owned()),
        ),
        (
            Behavior::RefuseCode {
                reason: "invalid_params".to_owned(),
                code: 4001,
            },
            DraftError::Failed("invalid_params".to_owned()),
        ),
        (
            Behavior::FailCode {
                code: 5030,
                message: "overloaded".to_owned(),
                reason: None,
            },
            DraftError::Failed("overloaded".to_owned()),
        ),
        (
            Behavior::FailCode {
                code: -32000,
                message: "broke".to_owned(),
                reason: None,
            },
            DraftError::Failed("broke".to_owned()),
        ),
    ];
    for (behavior, error) in errors {
        gateway.queue_behavior("llm.oneshot", behavior);
        assert_eq!(
            draft_message(&conn, &row(), "diff", "seed", "").await,
            Err(error)
        );
    }

    // An empty text fails with its own message.
    gateway.set_oneshot_text("  \n ");
    assert_eq!(
        draft_message(&conn, &row(), "diff", "seed", "").await,
        Err(DraftError::Failed("the model returned nothing".to_owned()))
    );

    // A dropped connection gives no reply. It goes last: the mock closes the socket.
    gateway.queue_behavior("llm.oneshot", Behavior::DropBeforeReply);
    assert_eq!(
        draft_message(&conn, &row(), "diff", "seed", "").await,
        Err(DraftError::NoReply)
    );
}

/// No session or no own route sends no `llm.oneshot`.
#[tokio::test]
async fn no_session_or_no_route_sends_nothing() {
    let home = tempfile::tempdir().expect("home");
    let gateway = StandaloneGateway::start(home.path());
    let link = LaunchLink::open(home.path(), "fixture")
        .await
        .expect("link");
    let _conn = LaunchConn::Link(link);

    // No session on the target: drafting emits nothing.
    let (mut state, _) = ReviewState::open(
        ReviewTarget {
            folder: PathBuf::from("repo"),
            main_root: PathBuf::from("repo"),
            session: None,
            record: None,
            title: "repo".to_owned(),
        },
        1.0,
    );
    let _ = state.update(ReviewInput::Key(ReviewKey::Commit));
    let effects = state.update(ReviewInput::Key(ReviewKey::Draft));
    assert!(
        !effects
            .iter()
            .any(|effect| matches!(effect, talaria_core::review::ReviewEffect::Draft { .. })),
        "no session drafts nothing: {effects:?}"
    );

    // A stored session without a folder has no route of its own.
    let (mut state, _) = ReviewState::open(
        ReviewTarget {
            folder: PathBuf::from("repo"),
            main_root: PathBuf::from("repo"),
            session: Some(row()),
            record: None,
            title: "repo".to_owned(),
        },
        1.0,
    );
    let _ = state.update(ReviewInput::SessionInfo {
        model: Some("fixture-model".to_owned()),
        can_draft: false,
    });
    let _ = state.update(ReviewInput::Key(ReviewKey::Commit));
    let effects = state.update(ReviewInput::Key(ReviewKey::Draft));
    assert!(
        !effects
            .iter()
            .any(|effect| matches!(effect, talaria_core::review::ReviewEffect::Draft { .. })),
        "no route drafts nothing: {effects:?}"
    );

    // A null model also has no route of its own.
    let _ = state.update(ReviewInput::SessionInfo {
        model: None,
        can_draft: false,
    });
    let effects = state.update(ReviewInput::Key(ReviewKey::Draft));
    assert!(
        !effects
            .iter()
            .any(|effect| matches!(effect, talaria_core::review::ReviewEffect::Draft { .. })),
        "a null model drafts nothing: {effects:?}"
    );

    assert!(
        gateway.requests("llm.oneshot").is_empty(),
        "nothing was sent"
    );
    gateway.assert_no_violations();
}

/// The drafting notice names the session's model, or the fallback words.
#[tokio::test]
async fn model_name_from_session_info() {
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;
    use talaria_tui::review_view::draw_review;
    use talaria_tui::tool_cell::ToolTheme;

    let draw_notice = |model: Option<String>| {
        let (mut state, _) = ReviewState::open(
            ReviewTarget {
                folder: PathBuf::from("repo"),
                main_root: PathBuf::from("repo"),
                session: Some(row()),
                record: None,
                title: "repo".to_owned(),
            },
            1.0,
        );
        let _ = state.update(ReviewInput::SessionInfo {
            model,
            can_draft: true,
        });
        let _ = state.update(ReviewInput::Key(ReviewKey::Commit));
        let _ = state.update(ReviewInput::Key(ReviewKey::Draft));
        let view = state.view();
        let theme = ToolTheme::default();
        let mut terminal = Terminal::new(TestBackend::new(120, 40)).expect("terminal");
        terminal
            .draw(|frame| {
                draw_review(&view, &theme, frame.area(), frame.buffer_mut());
            })
            .expect("draw");
        let buffer = terminal.backend().buffer().clone();
        let mut text = String::new();
        for y in 0..buffer.area.height {
            for x in 0..buffer.area.width {
                text.push_str(buffer[(x, y)].symbol());
            }
            text.push('\n');
        }
        text
    };

    let text = draw_notice(Some("fixture-model".to_owned()));
    assert!(
        text.contains("Drafting a message with fixture-model…"),
        "the model is named: {text}"
    );
    let text = draw_notice(None);
    assert!(
        text.contains("Drafting a message with the session's model…"),
        "a null model falls back: {text}"
    );
}
