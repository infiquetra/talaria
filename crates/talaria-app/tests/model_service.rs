//! Issue 230, scenarios for the model service calls.

#[path = "support/mock_gateway.rs"]
mod mock_gateway;

use std::collections::BTreeSet;
use std::path::PathBuf;

use mock_gateway::MockGateway;
use serde_json::{Map, Value, json};
use talaria_app::models::ModelService;
use talaria_core::models::{Effort, ModelsEffect, ModelsInput};
use talaria_core::mutation::{MutationOutcome, MutationRequest};

const SESSION: &str = "sess-model-1";

fn path() -> PathBuf {
    std::env::temp_dir().join(format!("talaria-models-{}.json", std::process::id()))
}

fn keys_of(value: &Value) -> BTreeSet<String> {
    value
        .as_object()
        .expect("params object")
        .keys()
        .cloned()
        .collect()
}

fn payload(value: Value) -> Map<String, Value> {
    value.as_object().expect("object").clone()
}

fn catalogue() -> Value {
    json!({
        "model": "minimax-m3.1-flash",
        "provider": "ollama-cloud",
        "providers": [{
            "slug": "ollama-cloud",
            "name": "Ollama Cloud",
            "models": ["minimax-m3.1-flash", "glm-5"],
            "authenticated": true
        }]
    })
}

/// Issue 230, scenario "options request exact keys".
#[tokio::test]
async fn options_request_exact_keys() {
    let (gateway, client, _stream) = MockGateway::start().await;
    let catalogue = catalogue();
    gateway.set_model_options(catalogue.clone(), catalogue);
    let service = ModelService::new(client, path());

    let loaded = service
        .execute(ModelsEffect::LoadOptions {
            session_id: SESSION.to_string(),
            refresh: false,
        })
        .await;
    let refreshed = service
        .execute(ModelsEffect::LoadOptions {
            session_id: SESSION.to_string(),
            refresh: true,
        })
        .await;

    let seen = gateway.requests("model.options");
    assert_eq!(seen.len(), 2);
    assert_eq!(
        seen[0].params,
        json!({
            "session_id": SESSION,
            "explicit_only": true,
            "include_unconfigured": true
        })
    );
    assert!(
        !seen[0]
            .params
            .as_object()
            .expect("object")
            .contains_key("profile")
    );
    assert!(
        !seen[0]
            .params
            .as_object()
            .expect("object")
            .contains_key("refresh")
    );
    assert_eq!(
        seen[1].params,
        json!({
            "session_id": SESSION,
            "explicit_only": true,
            "include_unconfigured": true,
            "refresh": true
        })
    );
    assert!(
        !seen[1]
            .params
            .as_object()
            .expect("object")
            .contains_key("profile")
    );

    match &loaded[..] {
        [ModelsInput::OptionsLoaded { session_id, result }] => {
            assert_eq!(session_id, SESSION);
            assert_eq!(result.model, "minimax-m3.1-flash");
            assert_eq!(result.provider, "ollama-cloud");
        }
        other => panic!("expected a loaded catalogue, got {other:?}"),
    }
    match &refreshed[..] {
        [ModelsInput::OptionsLoaded { session_id, result }] => {
            assert_eq!(session_id, SESSION);
            assert_eq!(result.model, "minimax-m3.1-flash");
        }
        other => panic!("expected a refreshed catalogue, got {other:?}"),
    }
}

/// Issue 230, scenario "effort read, set, and mutate params".
#[tokio::test]
async fn effort_read_set_and_mutate_params() {
    let (gateway, client, _stream) = MockGateway::start().await;
    gateway.set_reasoning_value(json!("high"));
    gateway.set_mutate_fences(12, 4);
    let service = ModelService::new(client, path());

    let read = service
        .execute(ModelsEffect::ReadEffort {
            session_id: SESSION.to_string(),
        })
        .await;
    let set_low = service
        .execute(ModelsEffect::SetEffort {
            session_id: SESSION.to_string(),
            level: Some(Effort::Low),
        })
        .await;
    let set_reset = service
        .execute(ModelsEffect::SetEffort {
            session_id: SESSION.to_string(),
            level: None,
        })
        .await;
    let request = MutationRequest {
        session_id: SESSION.to_string(),
        request_id: "req-m1".to_string(),
        expected_revision: 12,
        expected_generation: Some(4),
        operation: "model".to_string(),
        payload: payload(json!({
            "model": "glm-5",
            "provider": "ollama-cloud",
            "reasoning": "xhigh"
        })),
    };
    let changed = service.execute(ModelsEffect::Mutate(request)).await;

    let gets = gateway.requests("config.get");
    assert_eq!(gets.len(), 1);
    assert_eq!(
        gets[0].params,
        json!({"session_id": SESSION, "key": "reasoning"})
    );

    let sets = gateway.requests("config.set");
    assert_eq!(sets.len(), 2);
    assert_eq!(
        sets[0].params,
        json!({"session_id": SESSION, "key": "reasoning", "value": "low"})
    );
    assert_eq!(
        keys_of(&sets[0].params),
        BTreeSet::from([
            "session_id".to_string(),
            "key".to_string(),
            "value".to_string()
        ])
    );
    assert_eq!(
        sets[1].params,
        json!({"session_id": SESSION, "key": "reasoning", "value": "reset"})
    );
    assert!(
        !sets[1]
            .params
            .as_object()
            .expect("object")
            .contains_key("scope")
    );
    assert!(
        !sets[1]
            .params
            .as_object()
            .expect("object")
            .contains_key("confirm_expensive_model")
    );

    let mutates = gateway.requests("session.mutate");
    assert_eq!(mutates.len(), 1);
    assert_eq!(
        mutates[0].params,
        json!({
            "session_id": SESSION,
            "request_id": "req-m1",
            "expected_revision": 12,
            "expected_generation": 4,
            "operation": "model",
            "payload": {
                "model": "glm-5",
                "provider": "ollama-cloud",
                "reasoning": "xhigh"
            }
        })
    );

    match &read[..] {
        [
            ModelsInput::EffortRead {
                session_id,
                value: Some(level),
            },
        ] => {
            assert_eq!(session_id, SESSION);
            assert_eq!(level, "high");
        }
        other => panic!("expected the effort read, got {other:?}"),
    }
    match &set_low[..] {
        [
            ModelsInput::EffortSetReturned {
                session_id,
                level: Some(Effort::Low),
                outcome: Ok(()),
            },
        ] => assert_eq!(session_id, SESSION),
        other => panic!("expected the low effort write, got {other:?}"),
    }
    match &set_reset[..] {
        [
            ModelsInput::EffortSetReturned {
                session_id,
                level: None,
                outcome: Ok(()),
            },
        ] => assert_eq!(session_id, SESSION),
        other => panic!("expected the reset effort write, got {other:?}"),
    }
    match &changed[..] {
        [ModelsInput::MutationReturned { request, outcome }] => {
            assert_eq!(request.request_id, "req-m1");
            assert!(matches!(outcome, MutationOutcome::Committed(_)));
        }
        other => panic!("expected the mutate result, got {other:?}"),
    }
}
