//! Issue 230, scenarios for the mutation client.

#[path = "support/mock_gateway.rs"]
mod mock_gateway;

use mock_gateway::MockGateway;
use serde_json::{Map, Value, json};
use talaria_app::mutation::MutationClient;
use talaria_core::mutation::{MutationOutcome, MutationRequest};

const SESSION: &str = "sess-model-1";

fn payload(value: Value) -> Map<String, Value> {
    value.as_object().expect("object").clone()
}

fn request(
    id: &str,
    revision: i64,
    generation: i64,
    operation: &str,
    body: Value,
) -> MutationRequest {
    MutationRequest {
        session_id: SESSION.to_string(),
        request_id: id.to_string(),
        expected_revision: revision,
        expected_generation: Some(generation),
        operation: operation.to_string(),
        payload: payload(body),
    }
}

fn model_body() -> Value {
    json!({"model": "glm-5", "provider": "ollama-cloud", "reasoning": "xhigh"})
}

/// Issue 230, scenario "typed result keeps status fields".
#[tokio::test]
async fn typed_result_keeps_status_fields() {
    let (gateway, client, _stream) = MockGateway::start().await;
    gateway.set_mutate_fences(12, 4);
    let mutation = MutationClient::new(client);

    gateway.set_mutate_mode("busy");
    let deferred = mutation
        .send(&request("req-busy", 12, 4, "model", model_body()))
        .await;
    match deferred {
        MutationOutcome::Deferred(value) => {
            assert_eq!(value["status"], json!("deferred"));
            assert_eq!(value["model"], json!("glm-5"));
            assert_eq!(value["revision"], json!(12));
            assert_eq!(value["execution_generation"], json!(4));
        }
        other => panic!("expected a deferred result, got {other:?}"),
    }

    gateway.set_mutate_mode("guarded");
    let confirmation = mutation
        .send(&request("req-guard", 12, 4, "model", model_body()))
        .await;
    match confirmation {
        MutationOutcome::NeedsConfirmation(card) => {
            assert_eq!(card.token, "tok-1");
            assert_eq!(card.message, "Confirm the switch.");
            assert_eq!(card.target_model, "glm-5");
            assert_eq!(card.target_provider.as_deref(), Some("ollama-cloud"));
        }
        other => panic!("expected a confirmation, got {other:?}"),
    }

    gateway.set_mutate_mode("normal");
    let committed = mutation
        .send(&request("req-commit", 12, 4, "model", model_body()))
        .await;
    match committed {
        MutationOutcome::Committed(value) => {
            assert_eq!(value["model"], json!("glm-5"));
            assert_eq!(value["provider"], json!("ollama-cloud"));
            assert_eq!(value["effort"], json!("xhigh"));
            assert_eq!(value["revision"], json!(13));
            assert_eq!(value["execution_generation"], json!(5));
        }
        other => panic!("expected a committed receipt, got {other:?}"),
    }
}

/// Issue 230, scenario "fence refusal then retry on the mock".
#[tokio::test]
async fn fence_refusal_then_retry_on_mock() {
    let (gateway, client, _stream) = MockGateway::start().await;
    gateway.set_mutate_fences(15, 6);
    let mutation = MutationClient::new(client);

    let refused = mutation
        .send(&request("req-stale", 12, 4, "model", model_body()))
        .await;
    assert_eq!(
        refused,
        MutationOutcome::FenceRefused {
            reason: "revision_conflict".to_string(),
            revision: Some(15),
            execution_generation: Some(6),
        }
    );

    let committed = mutation
        .send(&request("req-retry", 15, 6, "model", model_body()))
        .await;
    match committed {
        MutationOutcome::Committed(value) => {
            assert_eq!(value["model"], json!("glm-5"));
            assert_eq!(value["revision"], json!(16));
            assert_eq!(value["execution_generation"], json!(7));
        }
        other => panic!("expected the retry to commit, got {other:?}"),
    }
}

/// Issue 230, scenario "guarded and busy modes".
#[tokio::test]
async fn guarded_and_busy_modes() {
    let (gateway, client, _stream) = MockGateway::start().await;
    let mutation = MutationClient::new(client);

    gateway.set_mutate_mode("guarded");
    gateway.set_mutate_fences(12, 4);
    let asking = mutation
        .send(&request("req-ask", 12, 4, "model", model_body()))
        .await;
    match asking {
        MutationOutcome::NeedsConfirmation(card) => assert_eq!(card.token, "tok-1"),
        other => panic!("expected a confirmation, got {other:?}"),
    }
    let mut confirmed_body = model_body();
    confirmed_body["confirm"] = json!("tok-1");
    let confirmed = mutation
        .send(&request("req-confirm", 12, 4, "model", confirmed_body))
        .await;
    assert!(
        matches!(confirmed, MutationOutcome::Committed(_)),
        "a confirmed switch commits, got {confirmed:?}"
    );

    gateway.set_mutate_mode("guarded-twice");
    gateway.set_mutate_fences(12, 4);
    let mut again = model_body();
    again["confirm"] = json!("tok-1");
    let still = mutation
        .send(&request("req-twice", 12, 4, "model", again))
        .await;
    match still {
        MutationOutcome::NeedsConfirmation(card) => {
            assert_eq!(card.token, "tok-1");
            assert_eq!(card.message, "Confirm the switch.");
        }
        other => panic!("expected a second confirmation, got {other:?}"),
    }

    gateway.set_mutate_mode("busy");
    gateway.set_mutate_fences(12, 4);
    let deferred = mutation
        .send(&request("req-deferred", 12, 4, "model", model_body()))
        .await;
    assert!(
        matches!(deferred, MutationOutcome::Deferred(_)),
        "a busy model pick is deferred, got {deferred:?}"
    );
    let busy_effort = mutation
        .send(&request(
            "req-busy-effort",
            12,
            4,
            "reasoning",
            json!({"effort": "high"}),
        ))
        .await;
    match busy_effort {
        MutationOutcome::Refused { reason, .. } => assert_eq!(reason, "session_busy"),
        other => panic!("expected session_busy, got {other:?}"),
    }

    gateway.set_mutate_mode("unresolvable");
    gateway.set_mutate_fences(12, 4);
    let unresolved = mutation
        .send(&request("req-missing", 12, 4, "model", model_body()))
        .await;
    match unresolved {
        MutationOutcome::Refused { reason, .. } => assert_eq!(reason, "model_resolution_failed"),
        other => panic!("expected model_resolution_failed, got {other:?}"),
    }
}

/// Issue 230, scenario "dropped connection is unknown and the resend is exact".
#[tokio::test]
async fn dropped_connection_is_unknown_and_exact_resend() {
    let (gateway, client, _stream) = MockGateway::start().await;
    gateway.set_mutate_mode("drop");
    gateway.set_mutate_fences(12, 4);
    let mutation = MutationClient::new(client);
    let original = request("req-drop", 12, 4, "model", model_body());

    let lost = mutation.send(&original).await;
    assert_eq!(lost, MutationOutcome::Unknown);

    gateway.replace_connection(mutation.client()).await;
    let recovered = mutation.send(&original).await;
    match recovered {
        MutationOutcome::Committed(value) => {
            assert_eq!(value["model"], json!("glm-5"));
            assert_eq!(value["provider"], json!("ollama-cloud"));
            assert_eq!(value["effort"], json!("xhigh"));
        }
        other => panic!("expected the stored receipt, got {other:?}"),
    }

    let seen = gateway.requests("session.mutate");
    assert_eq!(seen.len(), 2);
    assert_eq!(seen[0].params, seen[1].params);
}
