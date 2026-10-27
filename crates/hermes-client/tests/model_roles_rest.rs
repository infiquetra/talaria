//! Dashboard shapes for model roles.

#![allow(clippy::too_many_lines)]

#[path = "support/mock_rest.rs"]
mod mock_rest;

use std::collections::BTreeSet;
use std::sync::Arc;

use hermes_client::{
    AssignScope, AssignmentReply, AuxiliaryModels, EffortField, MoaConfig, MoaPreset, MoaSlot,
    ModelAssignment, NamedPreset, RestClient, RestError, RestOptions, StaleAux, model_info,
};
use mock_rest::{CountingMinter, MockRest};
use serde_json::{Map, Value, json};

const BUILTIN_TASKS: [&str; 12] = [
    "vision",
    "compression",
    "skills_hub",
    "approval",
    "mcp",
    "title_generation",
    "review",
    "voice_chat",
    "triage_specifier",
    "kanban_decomposer",
    "profile_describer",
    "curator",
];

fn fixture(name: &str) -> Value {
    let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/rest/model-roles")
        .join(name);
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|_| panic!("recorded fixture is not available"));
    serde_json::from_str(&text).unwrap_or_else(|_| panic!("recorded fixture is not json: {name}"))
}

fn empty_map() -> Map<String, Value> {
    Map::new()
}

#[test]
fn decodes_recorded_auxiliary() {
    let document = fixture("auxiliary.json");
    let decoded: AuxiliaryModels =
        serde_json::from_value(document["body"].clone()).expect("auxiliary decodes");
    let tasks: Vec<&str> = decoded
        .tasks
        .iter()
        .map(|task| task.task.as_str())
        .collect();
    assert!(
        tasks.starts_with(&BUILTIN_TASKS),
        "built-in helpers stay in display order"
    );
    let synthetic = json!({
        "tasks": [{
            "task": "plugin_task_1",
            "provider": "auto",
            "model": "",
            "base_url": "",
            "reasoning_effort": null,
            "local_endpoint": false,
            "label": "Plugin task 1",
            "hint": "Plugin hint 1",
            "plugin": "plugin-1",
            "inherit_from": "vision",
            "effective": {"provider": "public", "model": "a", "base_url": ""}
        }],
        "main": {"provider": "public", "model": "a"}
    });
    let plugin: AuxiliaryModels = serde_json::from_value(synthetic).expect("plugin row decodes");
    let row = &plugin.tasks[0];
    assert_eq!(row.label.as_deref(), Some("Plugin task 1"));
    assert_eq!(row.hint.as_deref(), Some("Plugin hint 1"));
    assert_eq!(row.plugin.as_deref(), Some("plugin-1"));
    assert_eq!(row.inherit_from.as_deref(), Some("vision"));
    let effective = row.effective.as_ref().expect("effective route");
    assert_eq!(effective.provider, "public");
    assert_eq!(effective.model, "a");
    assert_eq!(effective.base_url, "");
}

fn preset(name: &str, extra_key: Option<&str>) -> NamedPreset {
    let mut extra = empty_map();
    if let Some(key) = extra_key {
        extra.insert(key.to_owned(), json!(1));
    }
    NamedPreset {
        name: name.to_owned(),
        preset: MoaPreset {
            enabled: true,
            reference_models: vec![MoaSlot {
                provider: "public".to_owned(),
                model: "a".to_owned(),
                reasoning_effort: None,
                enabled: Some(true),
                extra: empty_map(),
            }],
            aggregator: MoaSlot {
                provider: "public".to_owned(),
                model: "a".to_owned(),
                reasoning_effort: None,
                enabled: None,
                extra: empty_map(),
            },
            reference_temperature: None,
            aggregator_temperature: None,
            reference_timeout: None,
            degraded_reference_policy: "loud".to_owned(),
            fanout: "user_turn".to_owned(),
            extra,
        },
    }
}

#[test]
fn moa_body_keeps_order_and_unknown_fields() {
    let config = MoaConfig {
        default_preset: "beta".to_owned(),
        active_preset: String::new(),
        presets: vec![preset("beta", Some("x_future")), preset("alpha", None)],
        privacy_filter: Some("full".to_owned()),
    };
    let text = serde_json::to_string(&config).expect("moa encodes");
    let value: Value = serde_json::from_str(&text).expect("encoded moa is json");
    let keys: BTreeSet<_> = value.as_object().expect("object").keys().cloned().collect();
    assert_eq!(
        keys,
        BTreeSet::from([
            "default_preset".to_owned(),
            "active_preset".to_owned(),
            "presets".to_owned()
        ])
    );
    assert!(
        value.get("privacy_filter").is_none(),
        "the privacy filter is never sent"
    );
    let beta = text.find("\"beta\"").expect("beta");
    let alpha = text.find("\"alpha\"").expect("alpha");
    assert!(beta < alpha, "preset order is the order that was read");
    assert!(text.contains("x_future"), "an unknown preset field is kept");
    let back: MoaConfig = serde_json::from_str(&text).expect("round trip");
    assert_eq!(
        back.presets
            .iter()
            .map(|item| item.name.as_str())
            .collect::<Vec<_>>(),
        vec!["beta", "alpha"]
    );
    assert_eq!(
        back.presets[0].preset.extra.get("x_future"),
        Some(&json!(1))
    );
    assert!(back.privacy_filter.is_none());
}

fn assignment(scope: AssignScope, effort: EffortField, base_url: &str) -> ModelAssignment {
    ModelAssignment {
        scope,
        provider: "ollama-cloud".to_owned(),
        model: "minimax-m3.1-flash".to_owned(),
        task: "vision".to_owned(),
        reasoning_effort: effort,
        base_url: base_url.to_owned(),
        confirm_expensive_model: false,
    }
}

fn object(value: Value) -> Map<String, Value> {
    value.as_object().expect("object").clone()
}

#[test]
fn assignment_body_fields() {
    let omitted = object(
        serde_json::to_value(assignment(AssignScope::Auxiliary, EffortField::Omit, "")).unwrap(),
    );
    assert!(omitted.get("reasoning_effort").is_none());
    assert_eq!(omitted.get("task").and_then(Value::as_str), Some("vision"));
    assert!(omitted.get("base_url").is_none());
    assert!(omitted.get("api_key").is_none());
    assert!(omitted.get("profile").is_none());

    let inherited = object(
        serde_json::to_value(assignment(AssignScope::Auxiliary, EffortField::Inherit, "")).unwrap(),
    );
    assert!(inherited["reasoning_effort"].is_null());

    let level = object(
        serde_json::to_value(assignment(
            AssignScope::Auxiliary,
            EffortField::Level("low".to_owned()),
            "https://endpoint.invalid",
        ))
        .unwrap(),
    );
    assert_eq!(level["reasoning_effort"], "low");
    assert_eq!(level["base_url"], "https://endpoint.invalid");
    assert!(level.get("api_key").is_none());
    assert!(level.get("profile").is_none());

    let main =
        object(serde_json::to_value(assignment(AssignScope::Main, EffortField::Omit, "")).unwrap());
    assert!(main.get("task").is_none(), "a main assignment has no task");
    assert!(main.get("api_key").is_none());
    assert!(main.get("profile").is_none());
}

#[test]
fn assignment_reply_variants() {
    let main_confirm: AssignmentReply = serde_json::from_value(json!({
        "ok": false,
        "scope": "main",
        "provider": "ollama-cloud",
        "model": "minimax-m3.1-flash",
        "confirm_required": true,
        "confirm_message": "this model is expensive"
    }))
    .unwrap();
    assert_eq!(
        main_confirm,
        AssignmentReply::ConfirmRequired {
            message: "this model is expensive".to_owned()
        }
    );
    let auxiliary_confirm: AssignmentReply = serde_json::from_value(json!({
        "ok": false,
        "scope": "auxiliary",
        "provider": "ollama-cloud",
        "model": "minimax-m3.1-flash",
        "confirm_required": true,
        "confirm_message": "confirm the helper"
    }))
    .unwrap();
    assert_eq!(
        auxiliary_confirm,
        AssignmentReply::ConfirmRequired {
            message: "confirm the helper".to_owned()
        }
    );
    let main: AssignmentReply = serde_json::from_value(json!({
        "ok": true,
        "scope": "main",
        "provider": "ollama-cloud",
        "model": "minimax-m3.1-flash",
        "base_url": "",
        "gateway_tools": ["web"],
        "stale_aux": [{"task": "vision", "provider": "other", "model": "x"}]
    }))
    .unwrap();
    assert_eq!(
        main,
        AssignmentReply::Main {
            provider: "ollama-cloud".to_owned(),
            model: "minimax-m3.1-flash".to_owned(),
            base_url: String::new(),
            gateway_tools: vec!["web".to_owned()],
            stale_aux: vec![StaleAux {
                task: "vision".to_owned(),
                provider: "other".to_owned(),
                model: "x".to_owned(),
            }],
        }
    );
    let auxiliary: AssignmentReply = serde_json::from_value(json!({
        "ok": true,
        "scope": "auxiliary",
        "tasks": ["vision"],
        "provider": "ollama-cloud",
        "model": "minimax-m3.1-flash"
    }))
    .unwrap();
    assert_eq!(
        auxiliary,
        AssignmentReply::Auxiliary {
            tasks: vec!["vision".to_owned()],
            provider: "ollama-cloud".to_owned(),
            model: "minimax-m3.1-flash".to_owned(),
        }
    );
    let reset: AssignmentReply = serde_json::from_value(json!({
        "ok": true,
        "scope": "auxiliary",
        "reset": true
    }))
    .unwrap();
    assert_eq!(reset, AssignmentReply::Reset);
}

async fn client_for(mock: &MockRest) -> RestClient {
    RestClient::new(
        &mock.origin(),
        Arc::new(CountingMinter::default()),
        RestOptions::default(),
    )
    .expect("loopback client")
}

#[tokio::test]
async fn errors_map_to_rest_variants() {
    let mock = MockRest::start().await;
    let client = client_for(&mock).await;
    mock.fail_status(
        "GET",
        "/api/model/info",
        400,
        Some("provider and model required for main"),
    )
    .await;
    let refused = model_info(&client).await.expect_err("400");
    assert!(matches!(
        refused,
        RestError::Refused { status: 400, detail: Some(text) } if text == "provider and model required for main"
    ));

    mock.fail_status(
        "GET",
        "/api/model/info",
        403,
        Some("profile_scope_mismatch"),
    )
    .await;
    let forbidden = model_info(&client).await.expect_err("403");
    assert!(
        matches!(forbidden, RestError::Forbidden { detail } if detail == "profile_scope_mismatch")
    );

    mock.fail_status(
        "GET",
        "/api/model/info",
        404,
        Some("No such API endpoint: /api/model/info"),
    )
    .await;
    let missing = model_info(&client).await.expect_err("404");
    assert!(matches!(missing, RestError::NotFound { route } if route == "GET /api/model/info"));

    mock.fail_status(
        "GET",
        "/api/model/info",
        422,
        Some("Invalid MoA config: preset"),
    )
    .await;
    let problem = model_info(&client).await.expect_err("422 string");
    assert!(matches!(
        problem,
        RestError::Refused { status: 422, detail: Some(text) } if text == "Invalid MoA config: preset"
    ));

    mock.status_body(
        "GET",
        "/api/model/info",
        422,
        br#"{"detail":[{"loc":["body"],"msg":"bad","type":"value_error"}]}"#.to_vec(),
    )
    .await;
    let listed = model_info(&client).await.expect_err("422 list");
    assert!(matches!(
        listed,
        RestError::Refused {
            status: 422,
            detail: None
        }
    ));

    mock.fail_status(
        "GET",
        "/api/model/info",
        500,
        Some("Failed to read auxiliary config"),
    )
    .await;
    let server = model_info(&client).await.expect_err("500");
    assert!(matches!(server, RestError::Server { status: 500 }));

    mock.fail_status(
        "GET",
        "/api/model/info",
        503,
        Some("Restart required: test"),
    )
    .await;
    let restart = model_info(&client).await.expect_err("503");
    assert!(matches!(restart, RestError::Server { status: 503 }));
}
