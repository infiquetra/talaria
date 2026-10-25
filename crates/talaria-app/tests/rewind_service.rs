//! Gateway calls and application wiring for checkpoint rewind. Issue 233.
//!
//! Tests 45 to 49 hit a stub. Tests 50 to 53 fail because the application does not yet send
//! the call or show the text. They drive the rewind calls through the service against the mock.

#[path = "support/mock_gateway.rs"]
mod mock_gateway;

use std::time::Duration;

use hermes_client::liveness::LinkEvent;
use hermes_client::replay::{GapReason, ReplayGap};
use hermes_client::{
    CallError, CallOptions, Client, Decoded, Generation, Inbound, RpcError, decode_frame,
};
use mock_gateway::{Behavior, MockGateway};
use serde_json::{Value, json};
use talaria_app::app::{
    AppEffect, AppMessage, AppState, RebuildMode, UserAction, rebuild_foreground, update,
};
use talaria_app::launch::SessionRequest;
use talaria_app::rewind::{RewindService, failure_from};
use talaria_app::sessions::{ExecuteOutcome, SessionService};
use talaria_core::commands::LOCAL_COMMANDS;
use talaria_core::rewind::{Call, CallFailure, CallKind};
use talaria_core::session::{Cell, Effect, SessionState};
use talaria_tui::composer::Composer;
use talaria_tui::keys::KeyMap;

const LAUNCH_ID: &str = "rewind-service";

fn keys_of(value: &Value) -> Vec<String> {
    let mut keys: Vec<String> = value
        .as_object()
        .map(|object| object.keys().cloned().collect())
        .unwrap_or_default();
    keys.sort();
    keys
}

fn committed_lines(effects: &[AppEffect]) -> Vec<String> {
    effects
        .iter()
        .filter_map(|effect| match effect {
            AppEffect::CommitLines(lines) => Some(lines.clone()),
            _ => None,
        })
        .flatten()
        .collect()
}

fn cell_text(cell: &Cell) -> String {
    match cell {
        Cell::User(cell) => cell.text.clone(),
        Cell::Assistant(cell) => cell.text.clone(),
        Cell::Reasoning(cell) => cell.text.clone(),
        Cell::Tool(cell) => cell.name.clone(),
        Cell::Notice(cell) => cell.text.clone(),
        Cell::Prompt(cell) => cell.command.clone().unwrap_or_default(),
        Cell::Shell(cell) => cell.command.clone(),
        Cell::Side(cell) => cell.question.clone(),
    }
}

async fn opened_app(snapshot: Value) -> (MockGateway, AppState) {
    let (mock, client, _inbound) = MockGateway::start().await;
    let session_id = snapshot["session_id"]
        .as_str()
        .unwrap_or_default()
        .to_string();
    mock.set_resume_snapshot(snapshot);
    let opened = SessionService::with_client(LAUNCH_ID, client)
        .open(SessionRequest::ById { session_id })
        .await
        .expect("the snapshot opens");
    let mut app = AppState::new(LAUNCH_ID);
    update(
        &mut app,
        AppMessage::Opened {
            opened,
            notice: None,
        },
    );
    (mock, app)
}

fn resume_snapshot() -> Value {
    let mut snapshot = mock_gateway::recorded_resume_snapshot();
    snapshot["prompts"] = json!([]);
    snapshot["execution_generation"] = json!(4);
    snapshot
}

/// Two eligible prompts and their replies, at revision 7 and execution generation 4. Rows
/// carry only the keys a rewind reads.
fn prompts_snapshot() -> Value {
    let mut snapshot = resume_snapshot();
    snapshot["revision"] = json!(7);
    let rows = json!([
        {"role": "user", "row_id": 11, "content": "first prompt", "timestamp": 1_792_674_000.0},
        {"role": "assistant", "row_id": 12, "content": "first reply", "timestamp": 1_792_674_005.0},
        {"role": "user", "row_id": 13, "content": "second\nline two", "timestamp": 1_792_674_100.0},
        {"role": "assistant", "row_id": 14, "content": "second reply", "timestamp": 1_792_674_105.0}
    ]);
    snapshot["messages"] = rows.clone();
    snapshot["display_messages"] = rows;
    snapshot["message_count"] = json!(4);
    snapshot
}

/// [`opened_app`] plus a rewind service and the client of the same connection.
async fn opened_app_with_service(
    snapshot: Value,
) -> (MockGateway, AppState, RewindService, Client) {
    let (mock, client, _inbound) = MockGateway::start().await;
    let session_id = snapshot["session_id"]
        .as_str()
        .unwrap_or_default()
        .to_string();
    mock.set_resume_snapshot(snapshot);
    let service = RewindService::new(client.clone(), session_id.clone());
    let opened = SessionService::with_client(LAUNCH_ID, client.clone())
        .open(SessionRequest::ById { session_id })
        .await
        .expect("the snapshot opens");
    let mut app = AppState::new(LAUNCH_ID);
    update(
        &mut app,
        AppMessage::Opened {
            opened,
            notice: None,
        },
    );
    (mock, app, service, client)
}

/// Run each rewind call the app asks for through the service against the mock, feeding every
/// reply back into `update`, until no call is left. Returns every effect, in order.
async fn drive(
    app: &mut AppState,
    service: &RewindService,
    first: Vec<AppEffect>,
) -> Vec<AppEffect> {
    let mut seen = Vec::new();
    let mut pending = first;
    loop {
        let next = pending.iter().find_map(|effect| match effect {
            AppEffect::Rewind { ticket, call } => Some((*ticket, call.clone())),
            _ => None,
        });
        seen.append(&mut pending);
        let Some((ticket, call)) = next else {
            return seen;
        };
        let reply = service.execute(ticket, call).await;
        pending = update(app, AppMessage::Rewind(reply));
    }
}

fn rewind_event(session_id: &str, epoch: &str, seq: u64, operation: &str, count: u64) -> LinkEvent {
    let frame = json!({
        "jsonrpc": "2.0",
        "method": "event",
        "params": {
            "type": "session.updated",
            "session_id": session_id,
            "replay_epoch": epoch,
            "seq": seq,
            "payload": {
                "operation": operation,
                "revision": 9,
                "rewound_count": count,
                "session_id": session_id
            }
        }
    });
    let bytes = serde_json::to_vec(&frame).expect("event frame");
    let decoded = decode_frame(Generation(1), &bytes, &mut |_id| unreachable!("event"));
    let Decoded::Inbound(item) = decoded else {
        panic!("session.updated decodes as an event");
    };
    assert!(matches!(item, Inbound::Event(_)));
    LinkEvent::Inbound {
        item,
        replayed: false,
    }
}

#[tokio::test]
async fn read_session_and_list_send_exact_params() {
    let (mock, client, _inbound) = MockGateway::start().await;
    let service = RewindService::new(client, "local-session");
    let _read = service.execute(1, Call::ReadSession).await;
    let resume = mock.requests("session.resume");
    assert_eq!(resume.len(), 1);
    assert_eq!(
        keys_of(&resume[0].params),
        ["display_projection", "session_id"]
    );
    assert_eq!(resume[0].params["display_projection"], true);
    assert!(resume[0].params.get("profile").is_none());

    let _list = service.execute(2, Call::ListCheckpoints).await;
    let listed = mock.requests("rollback.list");
    assert_eq!(listed.len(), 1);
    assert_eq!(keys_of(&listed[0].params), ["session_id"]);
    let _ = CallOptions {
        timeout: Some(Duration::from_secs(120)),
    };
}

#[tokio::test]
async fn mutate_sends_exact_params() {
    let (mock, client, _inbound) = MockGateway::start().await;
    let service = RewindService::new(client, "local-session");
    let _first = service
        .execute(
            1,
            Call::Mutate {
                row_id: 11,
                expected_revision: 7,
                expected_generation: 4,
            },
        )
        .await;
    let _second = service
        .execute(
            2,
            Call::Mutate {
                row_id: 11,
                expected_revision: 7,
                expected_generation: 4,
            },
        )
        .await;
    let frames = mock.requests("session.mutate");
    assert_eq!(frames.len(), 2);
    for frame in &frames {
        assert_eq!(
            keys_of(&frame.params),
            [
                "expected_generation",
                "expected_revision",
                "operation",
                "payload",
                "request_id",
                "session_id"
            ]
        );
        assert_eq!(frame.params["operation"], "rewind");
        assert_eq!(keys_of(&frame.params["payload"]), ["target_message_id"]);
        assert_eq!(frame.params["payload"]["target_message_id"], 11);
        let request_id = frame.params["request_id"].as_str().unwrap_or_default();
        assert_eq!(request_id.len(), 32);
        assert!(
            request_id
                .chars()
                .all(|ch| ch.is_ascii_hexdigit() && !ch.is_ascii_uppercase())
        );
        assert!(frame.params.get("profile").is_none());
        assert!(frame.params.get("unattended").is_none());
    }
    assert_ne!(
        frames[0].params["request_id"],
        frames[1].params["request_id"]
    );
}

#[tokio::test]
async fn rollback_calls_send_exact_params() {
    let (mock, client, _inbound) = MockGateway::start().await;
    let service = RewindService::new(client, "local-session");
    let _diff = service
        .execute(
            1,
            Call::CheckpointDiff {
                hash: "abcdef01".to_string(),
            },
        )
        .await;
    let diff = mock.requests("rollback.diff");
    assert_eq!(keys_of(&diff[0].params), ["hash", "session_id"]);
    assert!(diff[0].params.get("rendered").is_none());

    let _whole = service
        .execute(
            2,
            Call::Restore {
                hash: "abcdef0123456789".to_string(),
                file_path: None,
                all: true,
            },
        )
        .await;
    let whole = &mock.requests("rollback.restore")[0];
    assert_eq!(keys_of(&whole.params), ["all", "hash", "session_id"]);
    assert_eq!(whole.params["all"], true);

    let _file = service
        .execute(
            3,
            Call::Restore {
                hash: "abcdef0123456789".to_string(),
                file_path: Some("src/a  b.rs".to_string()),
                all: false,
            },
        )
        .await;
    let file = &mock.requests("rollback.restore")[1];
    assert_eq!(keys_of(&file.params), ["file_path", "hash", "session_id"]);
    assert_eq!(file.params["file_path"], "src/a  b.rs");
    assert!(file.params.get("all").is_none());

    let _session = service.execute(4, Call::SessionDiff).await;
    let session = mock.requests("rollback.session_diff");
    assert_eq!(keys_of(&session[0].params), ["session_id"]);
    let _ = CallKind::SessionDiff;
}

#[test]
fn wire_refusals_map_to_reasons() {
    let refused = CallError::Rpc(RpcError {
        code: 4001,
        message: "refused".to_string(),
        reason: Some("revision_conflict".to_string()),
        data: Some(json!({
            "reason": "revision_conflict",
            "field": "expected_revision",
            "detail": "stale",
            "revision": 9,
            "execution_generation": 4
        })),
    });
    let failure = failure_from(&refused);
    match failure {
        CallFailure::Refused {
            reason,
            field,
            detail,
            revision,
            execution_generation,
        } => {
            assert_eq!(reason, "revision_conflict");
            assert_eq!(field.as_deref(), Some("expected_revision"));
            assert_eq!(detail.as_deref(), Some("stale"));
            assert_eq!(revision, Some(9));
            assert_eq!(execution_generation, Some(4));
        }
        other => panic!("4001 with a reason is Refused, got {other:?}"),
    }

    let fields = failure_from(&CallError::Rpc(RpcError {
        code: 4001,
        message: "refused".to_string(),
        reason: Some("invalid_params".to_string()),
        data: Some(json!({
            "reason": "invalid_params",
            "fields": ["expected_revision", "operation"]
        })),
    }));
    match fields {
        CallFailure::Refused { detail, .. } => {
            let detail = detail.unwrap_or_default();
            assert!(detail.contains("expected_revision"));
            assert!(detail.contains("operation"));
        }
        other => panic!("a fields list joins into detail, got {other:?}"),
    }

    let unknown_method = failure_from(&CallError::Rpc(RpcError {
        code: -32601,
        message: "method not found".to_string(),
        reason: None,
        data: None,
    }));
    assert!(matches!(
        unknown_method,
        CallFailure::Refused { reason, .. } if reason == "unknown_method"
    ));

    let storage = failure_from(&CallError::Rpc(RpcError {
        code: 5001,
        message: "storage".to_string(),
        reason: None,
        data: None,
    }));
    assert!(matches!(
        storage,
        CallFailure::Refused { reason, .. } if reason == "storage_unavailable"
    ));

    for error in [
        CallError::NotConnected,
        CallError::Encode,
        CallError::InvalidTimeout,
    ] {
        assert!(matches!(failure_from(&error), CallFailure::NotSent(_)));
    }
    assert!(matches!(
        failure_from(&CallError::TimedOut {
            method: "session.resume".to_string(),
            after: Duration::from_secs(120),
        }),
        CallFailure::Unknown
    ));
    assert!(matches!(
        failure_from(&CallError::Unknown {
            method: "session.mutate".to_string(),
        }),
        CallFailure::Unknown
    ));
    assert!(matches!(
        failure_from(&CallError::BadResult {
            method: "rollback.diff".to_string(),
            reason: "result did not decode",
        }),
        CallFailure::Unreadable
    ));
}

#[tokio::test]
async fn rewind_rebuild_preserves_pending_side_work_and_request_ids() {
    let snapshot = resume_snapshot();
    let (_mock, mut app) = opened_app(snapshot.clone()).await;
    update(
        &mut app,
        AppMessage::User(UserAction::Submit("!echo retained".to_owned())),
    );
    let before = app.opened.as_ref().expect("open").state.side().clone();
    assert_eq!(before.pending.len(), 1);
    rebuild_foreground(
        &mut app,
        snapshot,
        RebuildMode::AfterRewind { notice: Vec::new() },
    );
    assert_eq!(app.opened.as_ref().expect("open").state.side(), &before);
}

#[tokio::test]
async fn rewind_rebuild_marks_cells_without_reprint() {
    let snapshot = resume_snapshot();
    let (_mock, mut app) = opened_app(snapshot.clone()).await;
    let before: Vec<String> = app
        .opened
        .as_ref()
        .expect("open")
        .state
        .cells()
        .iter()
        .map(cell_text)
        .collect();
    let subscription = app.opened.as_ref().expect("open").subscription_id.clone();
    let notice = vec![
        "── Rewound to before prompt #2 (\"second\"). Hermes removed 4 messages. The lines above from that prompt on are no longer part of the conversation. Files were not changed. ──"
            .to_string(),
    ];
    let effects = rebuild_foreground(
        &mut app,
        snapshot.clone(),
        RebuildMode::AfterRewind {
            notice: notice.clone(),
        },
    );
    let lines = committed_lines(&effects);
    assert!(
        lines
            .iter()
            .any(|line| line.contains("Rewound to before prompt #2")),
        "{lines:?}"
    );
    let reprinted = effects.iter().fold(Vec::new(), |mut found, effect| {
        if let AppEffect::CommitCells(range) = effect
            && let Some(opened) = app.opened.as_ref()
        {
            found.extend(opened.state.cells()[range.clone()].iter().map(cell_text));
        }
        found
    });
    for earlier in &before {
        assert_eq!(
            reprinted.iter().filter(|text| *text == earlier).count(),
            0,
            "{earlier} was printed again"
        );
    }
    let state: &SessionState = &app.opened.as_ref().expect("rebuilt").state;
    assert_eq!(state.execution_generation(), Some(4));
    assert_eq!(
        app.opened.as_ref().expect("rebuilt").subscription_id,
        subscription
    );
    assert_eq!(
        effects
            .iter()
            .filter(|effect| matches!(effect, AppEffect::Resync { .. }))
            .count(),
        1
    );

    let session_id = app.opened.as_ref().expect("rebuilt").session_id.clone();
    let gap = update(
        &mut app,
        AppMessage::Link(LinkEvent::Gap(ReplayGap {
            session_id,
            reason: GapReason::SeqJump,
            latest_seq: Some(1),
            replay_epoch: None,
        })),
    );
    assert!(committed_lines(&gap).is_empty());
}

#[tokio::test]
async fn session_updated_is_observed_and_still_reduced() {
    // The other window's rewind is already in the snapshot the re-read returns.
    let mut snapshot = prompts_snapshot();
    snapshot["revision"] = json!(9);
    let (_mock, mut app, service, _client) = opened_app_with_service(snapshot).await;
    let opened = app.opened.as_ref().expect("open");
    let session_id = opened.session_id.clone();
    let epoch = opened
        .state
        .cursor()
        .replay_epoch
        .clone()
        .unwrap_or_else(|| opened.cursor.replay_epoch.clone());
    let seq = opened
        .state
        .cursor()
        .last_seq
        .max(opened.cursor.last_sequence)
        + 1;
    let effects = update(
        &mut app,
        AppMessage::Link(rewind_event(&session_id, &epoch, seq, "rewind", 4)),
    );
    assert_eq!(
        app.opened.as_ref().expect("open").state.cursor().last_seq,
        seq,
        "the frame still reaches the session reducer"
    );
    let effects = drive(&mut app, &service, effects).await;
    let lines = committed_lines(&effects);
    assert_eq!(
        lines
            .iter()
            .filter(|line| line.contains("rewound in another window"))
            .count(),
        1,
        "no such text: {lines:?}"
    );

    let again = update(
        &mut app,
        AppMessage::Link(rewind_event(&session_id, &epoch, seq, "rewind", 4)),
    );
    let again = drive(&mut app, &service, again).await;
    assert!(
        !committed_lines(&again)
            .iter()
            .any(|line| line.contains("rewound in another window")),
        "the same revision rebuilds once"
    );
    let rename = update(
        &mut app,
        AppMessage::Link(rewind_event(&session_id, &epoch, seq + 1, "rename", 4)),
    );
    let rename = drive(&mut app, &service, rename).await;
    assert!(
        !committed_lines(&rename)
            .iter()
            .any(|line| line.contains("rewound")),
        "a rename does not rebuild"
    );
}

#[tokio::test]
async fn dropped_connection_is_unknown_and_never_resent() {
    let (mock, mut app, service, client) = opened_app_with_service(prompts_snapshot()).await;
    // Hermes does the rewind, then the socket closes before the reply.
    mock.set_behavior("session.mutate", Behavior::DropBeforeReply);
    let mut pending = update(
        &mut app,
        AppMessage::User(UserAction::Submit("/undo".to_string())),
    );
    let mut seen = Vec::new();
    while let Some((ticket, call)) = pending.iter().find_map(|effect| match effect {
        AppEffect::Rewind { ticket, call } => Some((*ticket, call.clone())),
        _ => None,
    }) {
        seen.append(&mut pending);
        let mutate = matches!(call, Call::Mutate { .. });
        let reply = service.execute(ticket, call).await;
        if mutate {
            // The link reconnects before the verifying read, as the application's does.
            mock.set_behavior("session.mutate", Behavior::Normal);
            mock.replace_connection(&client).await;
        }
        pending = update(&mut app, AppMessage::Rewind(reply));
    }
    seen.append(&mut pending);
    assert_eq!(mock.requests("session.mutate").len(), 1, "no such request");
    // The opening read, the read before the rewind, then one verifying read.
    assert_eq!(mock.requests("session.resume").len(), 3, "no such request");
    let lines = committed_lines(&seen);
    assert!(
        lines
            .iter()
            .any(|line| line.contains("Talaria lost Hermes's reply, but the rewind went through.")),
        "no such text: {lines:?}"
    );
}

#[tokio::test]
async fn local_commands_registered_and_shadow_hermes() {
    let (mock, mut app) = opened_app(resume_snapshot()).await;
    for command in ["/undo", "/retry", "/rollback", "/diff", "/diff staged"] {
        let effects = update(
            &mut app,
            AppMessage::User(UserAction::Submit(command.to_string())),
        );
        let lines = committed_lines(&effects);
        assert!(
            !lines.iter().any(|line| line.contains("Only /quit works")),
            "no such text"
        );
        if command == "/diff staged" {
            assert!(
                lines
                    .iter()
                    .any(|line| line.contains("oldest kept checkpoint")),
                "no such text"
            );
        }
    }
    assert!(mock.requests("slash.exec").is_empty(), "no such request");
    let names: Vec<&str> = LOCAL_COMMANDS.iter().map(|command| command.name).collect();
    assert_eq!(names, ["rewind", "undo", "retry", "rollback", "diff"]);
}

#[tokio::test]
async fn rewound_prompt_loads_only_into_empty_composer() {
    let loaded = |effects: &[AppEffect]| {
        effects.iter().any(
            |effect| matches!(effect, AppEffect::LoadDraft(text) if text == "second\nline two"),
        )
    };

    // An empty composer gets the rewound prompt back, exactly.
    let (_mock, mut app, service, _client) = opened_app_with_service(prompts_snapshot()).await;
    let first = update(
        &mut app,
        AppMessage::User(UserAction::Submit("/undo".to_string())),
    );
    let effects = drive(&mut app, &service, first).await;
    assert!(loaded(&effects), "no such text");

    // A draft typed while the rewind ran is kept, and the prompt is shown instead.
    let (_mock, mut app, service, _client) = opened_app_with_service(prompts_snapshot()).await;
    let first = update(
        &mut app,
        AppMessage::User(UserAction::Submit("/undo".to_string())),
    );
    app.draft = "keep me".to_string();
    let effects = drive(&mut app, &service, first).await;
    assert!(!loaded(&effects), "a draft is never overwritten");
    assert!(
        committed_lines(&effects)
            .iter()
            .any(|line| line.contains("Your draft was kept")),
        "no such text"
    );

    // /retry sends the prompt again; a refused send says so and returns the prompt.
    let (_mock, mut app, service, _client) = opened_app_with_service(prompts_snapshot()).await;
    let first = update(
        &mut app,
        AppMessage::User(UserAction::Submit("/retry".to_string())),
    );
    let effects = drive(&mut app, &service, first).await;
    let submission_id = effects
        .iter()
        .find_map(|effect| match effect {
            AppEffect::Send(Effect::SendPrompt {
                submission_id,
                text,
                ..
            }) if text == "second\nline two" => Some(submission_id.clone()),
            _ => None,
        })
        .expect("no such request");
    let refused = update(
        &mut app,
        AppMessage::SubmitDone {
            submission_id,
            outcome: ExecuteOutcome::Refused {
                reason: "session_busy".to_string(),
            },
        },
    );
    assert!(
        committed_lines(&refused)
            .iter()
            .any(|line| line.contains("sending it again failed")),
        "no such text"
    );
    assert!(loaded(&refused), "no such text");

    // A send whose outcome is unknown may have gone through: no T35, but the text comes back.
    let (_mock, mut app, service, _client) = opened_app_with_service(prompts_snapshot()).await;
    let first = update(
        &mut app,
        AppMessage::User(UserAction::Submit("/retry".to_string())),
    );
    let effects = drive(&mut app, &service, first).await;
    let sends: Vec<String> = effects
        .iter()
        .filter_map(|effect| match effect {
            AppEffect::Send(Effect::SendPrompt { submission_id, .. }) => {
                Some(submission_id.clone())
            }
            _ => None,
        })
        .collect();
    assert_eq!(sends.len(), 1, "no such request");
    let unknown = update(
        &mut app,
        AppMessage::SubmitDone {
            submission_id: sends[0].clone(),
            outcome: ExecuteOutcome::OutcomeUnknown,
        },
    );
    assert!(
        !committed_lines(&unknown)
            .iter()
            .any(|line| line.contains("sending it again failed")),
        "an unknown send is not called a failure"
    );
    assert!(loaded(&unknown), "no such text");

    // An image prompt returns its text only, and says the attachment was not restored.
    let mut snapshot = prompts_snapshot();
    let image = json!([
        {"type": "input_text", "text": "second\nline two"},
        {"type": "image_url", "image_url": {"url": "data:image/png;base64,AAAA"}}
    ]);
    snapshot["messages"][2]["content"] = image.clone();
    snapshot["display_messages"][2]["content"] = image;
    let (_mock, mut app, service, _client) = opened_app_with_service(snapshot).await;
    let first = update(
        &mut app,
        AppMessage::User(UserAction::Submit("/undo".to_string())),
    );
    let effects = drive(&mut app, &service, first).await;
    assert!(loaded(&effects), "no such text");
    assert!(
        committed_lines(&effects)
            .iter()
            .any(|line| line.contains("attachments were not restored")),
        "no such text"
    );

    let mut composer = Composer::new(KeyMap::defaults());
    composer.load("a\u{1b}]0;x\u{7}b\nline two".to_string());
    assert_eq!(composer.text(), "a]0;xb\nline two");
    assert!(!composer.text().contains('\u{1b}'));
}
