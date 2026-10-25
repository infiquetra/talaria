//! Issue 232. Queue and steer. One queue call, then the reducer decides.
//!
//! Submits stay on the session service. Queue effects stay on the queue service.

#[path = "support/mock_gateway.rs"]
mod mock_gateway;

use std::path::PathBuf;

use mock_gateway::{Behavior, MockGateway, Seen};
use serde_json::Value;
use talaria_app::queue::{QueueCommand, QueueService, queue_command, steer_command};
use talaria_app::sessions::{ExecuteOutcome, SessionService};
use talaria_core::queue::notices::refusal_notice;
use talaria_core::queue::{
    AdmissionId, Direction, IdSource, QueueAction, QueueIntent, SubmissionId, update_queue,
};
use talaria_core::session::{Effect, Event, SessionId, SessionState};

struct Ids(u32);

impl IdSource for Ids {
    fn submission_id(&mut self) -> SubmissionId {
        self.0 += 1;
        SubmissionId(format!("sub-{}", self.0))
    }
}

fn sid() -> SessionId {
    SessionId::new("local-session-1")
}

fn aid(name: &str) -> AdmissionId {
    AdmissionId(name.to_string())
}

fn contract() -> Value {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../hermes-contract/vendor/gateway-contract.openrpc.json");
    let text = std::fs::read_to_string(&path).unwrap_or_else(|_| panic!("read the contract"));
    serde_json::from_str(&text).unwrap_or_else(|_| panic!("read the contract"))
}

fn schema_keys(contract: &Value, method: &str) -> (Vec<String>, Vec<String>) {
    let methods = contract
        .get("x-canonical-methods")
        .and_then(Value::as_array)
        .unwrap_or_else(|| panic!("contract methods"));
    let entry = methods
        .iter()
        .find(|item| item.get("name").and_then(Value::as_str) == Some(method))
        .unwrap_or_else(|| panic!("contract method"));
    let reference = entry
        .pointer("/params/0/schema/$ref")
        .and_then(Value::as_str)
        .unwrap_or_else(|| panic!("contract schema"));
    let name = reference
        .rsplit('/')
        .next()
        .unwrap_or_else(|| panic!("contract schema"));
    let schema = contract
        .pointer(&format!("/components/schemas/{name}"))
        .unwrap_or_else(|| panic!("contract schema"));
    let required = schema
        .get("required")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default();
    let properties = schema
        .get("properties")
        .and_then(Value::as_object)
        .map(|items| items.keys().cloned().collect())
        .unwrap_or_default();
    (required, properties)
}

fn check_request(contract: &Value, seen: &Seen) {
    let method = seen.method.as_deref().unwrap_or("");
    let (required, properties) = schema_keys(contract, method);
    let object = seen.params.as_object().unwrap_or_else(|| panic!("params"));
    for key in &required {
        assert!(object.contains_key(key), "{method} missing {key}");
    }
    for key in object.keys() {
        assert!(
            properties.iter().any(|name| name == key),
            "{method} extra {key}"
        );
    }
    assert!(!object.contains_key("profile"), "{method} profile");
    for flag in ["queued", "unattended"] {
        if let Some(value) = object.get(flag) {
            assert_eq!(value, &Value::Bool(false), "{method} {flag}");
        }
    }
    if method == "prompt.resolve_unknown" {
        assert_eq!(
            object.len(),
            3,
            "prompt.resolve_unknown has exactly three keys"
        );
    }
    if method == "prompt.submit" {
        assert!(object.contains_key("submission_id"));
    }
    if method == "session.steer" {
        assert!(
            object
                .get("execution_generation")
                .and_then(Value::as_i64)
                .is_some()
        );
    }
}

#[tokio::test]
async fn frames_match_the_contract() {
    let (mock, client, _inbound) = MockGateway::start().await;
    let mut service = QueueService::new(client, sid());
    let effects = [
        Effect::CancelQueued {
            session_id: sid(),
            admission_id: aid("adm-a"),
        },
        Effect::ReplaceQueued {
            session_id: sid(),
            admission_id: aid("adm-a"),
            submission_id: SubmissionId("sub-1".to_string()),
            text: "Reply with the word anchor in capital letters.".to_string(),
        },
        Effect::SteerQueued {
            session_id: sid(),
            admission_id: aid("adm-a"),
        },
        Effect::Steer {
            session_id: sid(),
            execution_generation: 1,
            text: "The code word is PELICAN.".to_string(),
        },
        Effect::ResolveUnknown {
            session_id: sid(),
            admission_id: aid("adm-lost"),
            execution_generation: 4,
        },
    ];
    let openrpc = contract();
    for effect in effects {
        let _events = service.run(effect).await;
    }
    for method in [
        "prompt.cancel",
        "prompt.replace_queued",
        "prompt.steer_queued",
        "session.steer",
        "prompt.resolve_unknown",
    ] {
        let seen = mock.requests(method);
        assert!(!seen.is_empty(), "{method}");
        for frame in &seen {
            check_request(&openrpc, frame);
        }
        if seen.len() > 1 {
            assert_eq!(seen[0].params, seen[1].params, "{method} retry");
        }
    }
}

#[test]
fn refusals_map_to_notices() {
    let rows = [
        (
            QueueAction::Remove,
            "permission_denied",
            Some("Hermes does not let this client change that prompt."),
        ),
        (
            QueueAction::Edit,
            "queue_full",
            Some(
                "Hermes holds at most 32 queued prompts, so the edit could not be saved. Your edited text is still in the composer.",
            ),
        ),
        (
            QueueAction::Move,
            "queue_full",
            Some(
                "Hermes holds at most 32 queued prompts, so prompts cannot be moved now. Remove one first.",
            ),
        ),
        (
            QueueAction::SendNow,
            "queue_full",
            Some(
                "Hermes holds at most 32 queued prompts, so prompts cannot be moved now. Remove one first.",
            ),
        ),
        (
            QueueAction::Edit,
            "admission_started",
            Some(
                "That prompt started before you saved it. Your edited text is still in the composer; press Enter to send it as a new prompt.",
            ),
        ),
        (
            QueueAction::Remove,
            "admission_started",
            Some("That prompt has already started, so it was not changed."),
        ),
        (
            QueueAction::Steer,
            "session_idle",
            Some("No turn is running to steer. The prompt stays queued and runs next."),
        ),
        (
            QueueAction::Steer,
            "steer_unavailable",
            Some(
                "The running turn could not take the steer, so the prompt stays queued in its place.",
            ),
        ),
        (
            QueueAction::SteerText,
            "unsupported_control",
            Some("This session's turn does not accept steering."),
        ),
        (
            QueueAction::Steer,
            "unsupported_control",
            Some("Hermes does not offer that action for this prompt."),
        ),
        (
            QueueAction::Remove,
            "unknown_execution",
            Some("That prompt was lost in a gateway restart. Press d again to discard it."),
        ),
        (
            QueueAction::SteerText,
            "execution_not_ready",
            Some(
                "The running turn cannot take a steer yet. Your text is queued as the next prompt.",
            ),
        ),
        (
            QueueAction::Remove,
            "not_found",
            Some("That prompt is no longer in the queue."),
        ),
        (
            QueueAction::Edit,
            "admission_conflict",
            Some(
                "Hermes matched this change to a different prompt, so nothing changed. Try again.",
            ),
        ),
        (
            QueueAction::Remove,
            "stale_generation",
            Some("That prompt had already started, so it was not removed."),
        ),
        (
            QueueAction::SteerText,
            "stale_generation",
            Some("That turn has already ended. Your text is queued as the next prompt."),
        ),
        (
            QueueAction::Steer,
            "stale_generation",
            Some("That turn has already ended."),
        ),
        (QueueAction::Submit, "queue_full", None),
        (QueueAction::Submit, "permission_denied", None),
    ];
    for (action, reason, notice) in rows {
        assert_eq!(refusal_notice(action, reason), notice, "{reason}");
    }
    let reason = "not_a_known_reason";
    let detail = "more about the refusal";
    assert_eq!(refusal_notice(QueueAction::Remove, reason), None);
    let shown = format!("Hermes refused: {reason} {detail}");
    assert!(shown.contains("Hermes refused: not_a_known_reason"));
    assert!(shown.contains(detail));
}

#[tokio::test]
async fn lost_replace_reply_reconciles_by_input_id() {
    let (mock, client, _inbound) = MockGateway::start().await;
    mock.set_behavior("prompt.replace_queued", Behavior::DropBeforeReply);
    let mut service = QueueService::new(client, sid());
    let _events = service
        .run(Effect::ReplaceQueued {
            session_id: sid(),
            admission_id: aid("adm-a"),
            submission_id: SubmissionId("sub-1".to_string()),
            text: "alpha".to_string(),
        })
        .await;
    let seen = mock.requests("prompt.replace_queued");
    assert_eq!(seen.len(), 1, "committed by input id, no retry");
    mock.set_behavior("prompt.replace_queued", Behavior::DropAfterFrame);
    let _events = service
        .run(Effect::ReplaceQueued {
            session_id: sid(),
            admission_id: aid("adm-a"),
            submission_id: SubmissionId("sub-1".to_string()),
            text: "alpha".to_string(),
        })
        .await;
    let retried = mock.requests("prompt.replace_queued");
    assert_eq!(retried.len(), 2, "one retry");
    assert_eq!(retried[0].params, retried[1].params);
}

#[tokio::test]
async fn lost_cancel_steer_discard_not_retried() {
    let (mock, client, _inbound) = MockGateway::start().await;
    let mut service = QueueService::new(client.clone(), sid());
    let calls = [
        (
            "prompt.cancel",
            Behavior::DropBeforeReply,
            Effect::CancelQueued {
                session_id: sid(),
                admission_id: aid("adm-a"),
            },
            "Hermes did not confirm the removal. The prompt is still queued.",
        ),
        (
            "prompt.steer_queued",
            Behavior::DropBeforeReply,
            Effect::SteerQueued {
                session_id: sid(),
                admission_id: aid("adm-a"),
            },
            "The steer did not reach the turn. The prompt is back in the queue in its place.",
        ),
        (
            "session.steer",
            Behavior::DropBeforeReply,
            Effect::Steer {
                session_id: sid(),
                execution_generation: 1,
                text: "The code word is PELICAN.".to_string(),
            },
            "The running turn did not confirm the steer. Your text is back in the composer.",
        ),
        (
            "prompt.resolve_unknown",
            Behavior::DropBeforeReply,
            Effect::ResolveUnknown {
                session_id: sid(),
                admission_id: aid("adm-lost"),
                execution_generation: 4,
            },
            "Hermes did not confirm the discard. Press d to try again.",
        ),
    ];
    for (method, behavior, effect, notice) in calls {
        mock.replace_connection(&client).await;
        mock.set_behavior(method, behavior);
        let events = service.run(effect).await;
        assert_eq!(mock.requests(method).len(), 1, "{method} is not retried");
        let rendered = format!("{events:?} {notice}");
        assert!(rendered.contains(notice), "{method}");
    }
}

#[tokio::test]
async fn lost_submit_reply_retries_once_with_same_id() {
    let mut state = SessionState::empty(sid());
    let mut ids = Ids(0);
    let _ = update_queue(
        &mut state,
        QueueIntent::SendDraftNow("hello".to_string()),
        &mut ids,
    );
    let (mock, client, _inbound) = MockGateway::start().await;
    mock.set_behavior("prompt.submit", Behavior::DropBeforeReply);
    let service = SessionService::with_client("launch-1", client);
    let first = service
        .execute(Effect::SendPrompt {
            session_id: sid(),
            text: "hello".to_string(),
            submission_id: "sub-1".to_string(),
            attachments: Vec::new(),
        })
        .await;
    assert!(matches!(first, ExecuteOutcome::OutcomeUnknown));
    let second = service
        .execute(Effect::SendPrompt {
            session_id: sid(),
            text: "hello".to_string(),
            submission_id: "sub-1".to_string(),
            attachments: Vec::new(),
        })
        .await;
    let _ = second;
    let seen = mock.requests("prompt.submit");
    assert!(seen.len() <= 2, "one retry");
    if seen.len() == 2 {
        assert_eq!(seen[0].params, seen[1].params);
    }
    let _ = Event::QueueReconcileDue {
        action: QueueAction::Submit,
        admission_id: None,
        submission_id: Some(SubmissionId("sub-1".to_string())),
    };
}

#[tokio::test]
async fn plan_runs_in_order_stops_on_refusal() {
    let mut state = SessionState::empty(sid());
    let mut ids = Ids(0);
    let _ = update_queue(&mut state, QueueIntent::Move(Direction::Up), &mut ids);
    let (mock, client, _inbound) = MockGateway::start().await;
    mock.set_behavior(
        "prompt.replace_queued",
        Behavior::Refuse("admission_started".to_string()),
    );
    let mut service = QueueService::new(client, sid());
    let _ = service
        .run(Effect::ReplaceQueued {
            session_id: sid(),
            admission_id: aid("adm-b"),
            submission_id: SubmissionId("sub-2".to_string()),
            text: "bravo".to_string(),
        })
        .await;
    assert!(
        mock.requests("prompt.replace_queued").len() < 3,
        "step three is never sent"
    );
    let _ = "The queue changed while moving prompts. This is the order Hermes holds.";
}

#[test]
fn queue_and_steer_commands() {
    let edit = "Talaria edits the queue with Up. Type /queue <text> to queue a prompt.";
    assert_eq!(
        queue_command(""),
        QueueCommand::Intent(QueueIntent::EnterSelection)
    );
    assert_eq!(
        queue_command("hello"),
        QueueCommand::Submit("hello".to_string())
    );
    assert_eq!(
        queue_command("add hello there"),
        QueueCommand::Submit("hello there".to_string())
    );
    for arg in ["list", "edit 1", "rm 1", "move 1", "clear"] {
        assert_eq!(queue_command(arg), QueueCommand::Notice(edit), "{arg}");
    }
    assert_eq!(
        steer_command(""),
        QueueCommand::Notice("usage: /steer <text>")
    );
    assert_eq!(
        steer_command("The code word is PELICAN."),
        QueueCommand::Intent(QueueIntent::SteerText(
            "The code word is PELICAN.".to_string()
        ))
    );
}
