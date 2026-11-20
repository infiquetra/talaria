//! Live `approval.cancelled` broadcasts: empty envelope session id, omitted request ids,
//! and a later answer that must not call the gateway.

use crate::fake_fleet::FakeFleetSink;
use crate::mock_gateway::MockGateway;

use std::path::PathBuf;
use std::sync::Arc;

use hermes_client::liveness::LinkEvent;
use hermes_client::{Decoded, Generation, Inbound, InboundEvent, decode_frame};
use serde_json::{Value, json};
use talaria_app::app::{AppEffect, AppMessage, AppState, UserAction, update};
use talaria_app::launch::{Launch, StartMode};
use talaria_app::peek::{PeekService, PeekTimeouts};
use talaria_app::prompts::PromptService;
use talaria_app::sessions::OpenedSession;
use talaria_core::directory::rows::RowKey;
use talaria_core::session::{AnswerRefusal, Effect, PromptKey, PromptKind, SessionId};

fn decode_event(params: Value) -> InboundEvent {
    let bytes = json!({"jsonrpc":"2.0","method":"event","params":params}).to_string();
    let decoded = decode_frame(Generation(1), bytes.as_bytes(), &mut |_| {
        panic!("approval.cancelled is not a server request")
    });
    match decoded {
        Decoded::Inbound(Inbound::Event(event)) => event,
        Decoded::Inbound(Inbound::Malformed(_)) => panic!("the broadcast was rejected"),
        _ => panic!("the broadcast was not an event"),
    }
}

fn deliver(state: &mut AppState, event: InboundEvent) {
    update(
        state,
        AppMessage::Link(LinkEvent::Inbound {
            item: Inbound::Event(event),
            replayed: false,
        }),
    );
}

fn approval_open(state: &talaria_core::session::SessionState, prompt_id: &str) -> bool {
    state
        .open_prompts()
        .any(|prompt| prompt.kind == PromptKind::Approval && prompt.key.prompt_id == prompt_id)
}

fn answer_effects(state: &mut AppState, prompt_id: &str) -> Vec<AppEffect> {
    update(
        state,
        AppMessage::User(UserAction::Approve {
            key: PromptKey {
                session_id: SessionId::new("local-session-1"),
                prompt_id: prompt_id.into(),
                execution_generation: 1,
            },
            choice: "once".into(),
            note: None,
        }),
    )
}

#[tokio::test]
async fn approval_cancelled_empty_envelope_withdraws_and_sends_no_rpc() {
    let mut state = AppState::new("approval-broadcast");
    let snapshot = serde_json::from_str(include_str!("../data/open_approval.json")).unwrap();
    state.opened = Some(OpenedSession::from_snapshot(snapshot));

    let event = decode_event(json!({
        "session_id": "",
        "replay_epoch": "epoch-1",
        "seq": 5,
        "type": "approval.cancelled",
        "payload": {
            "session_id": "local-session-1",
            "stored_session_id": "stored-1",
            "reason": "interrupt",
            "cancelled_count": 1,
            "request_ids": ["prompt-1"]
        }
    }));
    deliver(&mut state, event);
    assert!(
        !approval_open(&state.opened.as_ref().unwrap().state, "prompt-1"),
        "the named session's approval stayed answerable"
    );

    let effects = answer_effects(&mut state, "prompt-1");
    let effect = effects.into_iter().find_map(|effect| match effect {
        AppEffect::Respond { effect, .. } => Some(effect),
        _ => None,
    });
    assert!(
        matches!(
            &effect,
            Some(Effect::RefuseAnswer {
                reason: AnswerRefusal::AlreadyAnswered,
                ..
            })
        ),
        "the withdrawn card was still answerable"
    );
    let (mock, client, _inbound) = MockGateway::start().await;
    let mut prompts = PromptService::new(Box::new(|| "talaria-approval-1".into()));
    let _ = prompts.execute(&client, effect.unwrap()).await;
    assert!(
        mock.requests("approval.respond").is_empty(),
        "a withdrawn approval sent approval.respond"
    );
}

#[tokio::test]
async fn approval_cancelled_omitted_request_ids_withdraws_only_the_named_session() {
    let null_ids = decode_event(json!({
        "session_id": "",
        "type": "approval.cancelled",
        "payload": {
            "session_id": "local-session-1",
            "stored_session_id": "stored-1",
            "reason": "interrupt",
            "cancelled_count": 1,
            "request_ids": null
        }
    }));
    match null_ids.frame.notification {
        hermes_contract::events::Notification::ApprovalCancelled(payload) => {
            assert!(payload.request_ids.is_empty());
        }
        _ => panic!("a null request_ids list did not decode"),
    }

    let mut state = AppState::new("approval-omitted");
    let snapshot = serde_json::from_str(include_str!("../data/open_approval.json")).unwrap();
    state.opened = Some(OpenedSession::from_snapshot(snapshot));

    let event = decode_event(json!({
        "session_id": "",
        "replay_epoch": "epoch-1",
        "seq": 5,
        "type": "approval.cancelled",
        "payload": {
            "session_id": "local-session-1",
            "stored_session_id": "stored-1",
            "reason": "reap",
            "cancelled_count": 1
        }
    }));
    match &event.frame.notification {
        hermes_contract::events::Notification::ApprovalCancelled(payload) => {
            assert!(payload.request_ids.is_empty());
        }
        _ => panic!("an omitted request_ids list did not decode"),
    }
    deliver(&mut state, event);
    assert!(!approval_open(
        &state.opened.as_ref().unwrap().state,
        "prompt-1"
    ));

    let (mock, client, _inbound) = MockGateway::start().await;
    let effects = answer_effects(&mut state, "prompt-1");
    let effect = effects
        .into_iter()
        .find_map(|effect| match effect {
            AppEffect::Respond { effect, .. } => Some(effect),
            _ => None,
        })
        .expect("the answer produced an effect");
    let mut prompts = PromptService::new(Box::new(|| "talaria-approval-2".into()));
    let _ = prompts.execute(&client, effect).await;
    assert!(mock.requests("approval.respond").is_empty());
}

#[tokio::test]
async fn peek_approval_cancelled_empty_envelope_withdraws_the_card() {
    let home = tempfile::tempdir().expect("home");
    let mut peek = PeekService::new(
        Launch {
            mode: StartMode::New,
            model: None,
            provider: None,
            effort: None,
            profile: "fixture".into(),
            folder: PathBuf::from("."),
            record: None,
            bench_replay: None,
            no_ensure: true,
        },
        home.path().to_path_buf(),
        Arc::new(FakeFleetSink::new()),
        Box::new(|| "talaria-peek-1".into()),
        PeekTimeouts::scaled_for_tests(),
    );
    let snapshot = json!({
        "session_id": "peek-session",
        "stored_session_id": "stored-peek",
        "messages": [],
        "message_count": 0,
        "running": false,
        "authority_epoch": 1,
        "replay_epoch": "epoch-1",
        "last_sequence": 2,
        "subscription_id": "sub-peek",
        "revision": 1,
        "execution_generation": 1,
        "pending": [],
        "prompts": [{
            "kind": "approval",
            "prompt_id": "peek-prompt",
            "execution_generation": 1,
            "choices": ["once", "deny"],
            "command": "ls"
        }],
        "info": {"desktop_protocol": "1"},
    });
    peek.replay_snapshot(
        RowKey {
            profile: "other".into(),
            session_id: "peek-session".into(),
        },
        snapshot,
    )
    .expect("snapshot");
    let event = decode_event(json!({
        "session_id": "",
        "replay_epoch": "epoch-1",
        "seq": 3,
        "type": "approval.cancelled",
        "payload": {
            "session_id": "peek-session",
            "stored_session_id": "stored-peek",
            "reason": "teardown",
            "cancelled_count": 1,
            "request_ids": ["peek-prompt"]
        }
    }));
    peek.on_link_event(LinkEvent::Inbound {
        item: Inbound::Event(event),
        replayed: false,
    });
    let (_, session) = peek.state().expect("the peek stays open");
    assert!(
        !approval_open(session, "peek-prompt"),
        "the peek kept the withdrawn approval"
    );
}
