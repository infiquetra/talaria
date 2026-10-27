//! The model roles service sends the dashboard calls and nothing else.

#![allow(clippy::too_many_lines)]

#[path = "support/mock_gateway.rs"]
mod mock_gateway;
#[path = "../../hermes-client/tests/support/mock_rest.rs"]
mod mock_rest;

use std::sync::Arc;

use mock_gateway::MockGateway;
use mock_rest::{CountingMinter, MockRest};
use serde_json::{Value, json};
use talaria_app::model_roles::{ModelRolesOpen, ModelRolesService};
use talaria_core::model_roles::{
    ApplyStep, AssignScopeView, Assignment, EffortChange, MoaConfigView, MoaPresetView,
    RolesEffect, RolesError, RolesInput, Section, SlotView,
};

fn assignment(
    scope: AssignScopeView,
    task: &str,
    provider: &str,
    model: &str,
    effort: EffortChange,
    base_url: &str,
    confirmed: bool,
) -> Assignment {
    Assignment {
        scope,
        task: task.to_owned(),
        provider: provider.to_owned(),
        model: model.to_owned(),
        reasoning_effort: effort,
        base_url: base_url.to_owned(),
        confirmed,
    }
}

fn catalogue() -> Value {
    json!({
        "model": "minimax-m3.1-flash",
        "provider": "ollama-cloud",
        "providers": [
            {"slug": "ollama-cloud", "name": "Ollama Cloud", "models": ["minimax-m3.1-flash"], "authenticated": true},
            {"slug": "openrouter", "name": "OpenRouter", "models": ["alpha"], "authenticated": true},
            {"slug": "skeleton", "name": "Skeleton", "models": [], "authenticated": false},
            {"slug": "my-lab", "name": "My Lab", "models": ["local"], "authenticated": true, "is_user_defined": true, "api_url": "https://endpoint.invalid"},
            {"slug": "moa", "name": "Mixture of Agents", "models": ["preset"], "source": "virtual"}
        ]
    })
}

struct Rig {
    rest: MockRest,
    gateway: MockGateway,
    service: ModelRolesService,
}

async fn rig(profile: &str, connection: &str, origin: bool) -> Rig {
    let rest = MockRest::start().await;
    let (gateway, socket, _inbound) = MockGateway::start().await;
    gateway.set_model_options(catalogue(), catalogue());
    let client = hermes_client::RestClient::new(
        &rest.origin(),
        Arc::new(CountingMinter::default()),
        hermes_client::RestOptions::default(),
    )
    .expect("loopback client");
    let service = ModelRolesService::new(ModelRolesOpen {
        profile: profile.to_owned(),
        connection_profile: connection.to_owned(),
        api_origin: origin.then(|| rest.origin()),
        rest: origin.then_some(client),
        socket: Some(socket),
        served_profiles: vec![connection.to_owned(), "other".to_owned()],
    });
    Rig {
        rest,
        gateway,
        service,
    }
}

fn body_of(requests: &[mock_rest::RecordedRequest], method: &str, path: &str) -> Value {
    let found = requests
        .iter()
        .find(|item| item.method == method && item.path.split('?').next() == Some(path));
    let found = found.unwrap_or_else(|| panic!("missing {method} {path}"));
    serde_json::from_slice(&found.body).unwrap_or(Value::Null)
}

fn assert_no_profile(requests: &[mock_rest::RecordedRequest]) {
    for item in requests {
        assert!(
            !item.path.contains("profile="),
            "{} {} carries a profile selector",
            item.method,
            item.path
        );
        if let Ok(value) = serde_json::from_slice::<Value>(&item.body) {
            assert!(
                value.get("profile").is_none(),
                "{} {} carries a profile field",
                item.method,
                item.path
            );
            assert!(
                value.get("api_key").is_none() || item.path.contains("/api/config"),
                "a model write carries no api key"
            );
        }
    }
}

#[tokio::test]
async fn main_write_and_read_back() {
    let rig = rig("fixture", "fixture", true).await;
    rig.rest.response_body("POST", "/api/model/set", br#"{"ok":true,"scope":"main","provider":"ollama-cloud","model":"minimax-m3.1-flash","base_url":"","gateway_tools":[],"stale_aux":[]}"#.to_vec()).await;
    rig.rest
        .response_body(
            "GET",
            "/api/model/auxiliary",
            br#"{"tasks":[],"main":{"provider":"ollama-cloud","model":"minimax-m3.1-flash"}}"#
                .to_vec(),
        )
        .await;
    rig.rest
        .response_body(
            "GET",
            "/api/config",
            br#"{"model_context_length":0,"agent":{"reasoning_effort":""}}"#.to_vec(),
        )
        .await;
    let inputs = rig
        .service
        .execute(RolesEffect::Assign {
            step: ApplyStep::MainModel,
            assignment: assignment(
                AssignScopeView::Main,
                "",
                "ollama-cloud",
                "minimax-m3.1-flash",
                EffortChange::Omit,
                "",
                false,
            ),
        })
        .await;
    let requests = rig.rest.requests().await;
    assert_no_profile(&requests);
    let body = body_of(&requests, "POST", "/api/model/set");
    assert_eq!(body["scope"], "main");
    assert_eq!(body["provider"], "ollama-cloud");
    assert_eq!(body["model"], "minimax-m3.1-flash");
    assert!(body.get("task").is_none());
    assert!(body.get("base_url").is_none());
    assert!(matches!(
        inputs.first(),
        Some(RolesInput::Applied {
            step: ApplyStep::MainModel,
            reply: talaria_core::model_roles::ApplyReply::Assigned(_)
        })
    ));
    let _ = rig
        .service
        .execute(RolesEffect::ReadBack {
            section: Section::Main,
        })
        .await;
    let requests = rig.rest.requests().await;
    assert!(
        requests
            .iter()
            .any(|item| item.method == "GET" && item.path.contains("include_defaults=false"))
    );
}

#[tokio::test]
async fn fallbacks_and_subagents_put_config() {
    let rig = rig("fixture", "fixture", true).await;
    let fallbacks = json!({"fallback_providers": [
        {"provider": "ollama-cloud", "model": "minimax-m3.1-flash", "api_key": "${EXAMPLE_API_KEY}"},
        {"provider": "openrouter", "model": "alpha"}
    ]});
    rig.service
        .execute(RolesEffect::PutConfig {
            step: ApplyStep::Fallbacks,
            patch: fallbacks.clone(),
        })
        .await;
    let requests = rig.rest.requests().await;
    let body = body_of(&requests, "PUT", "/api/config");
    assert_eq!(
        body["config"]["fallback_providers"][0]["api_key"],
        "${EXAMPLE_API_KEY}"
    );
    assert_no_profile(&requests);

    let delegation = json!({"delegation": {"provider": "ollama-cloud", "model": "minimax-m3.1-flash", "base_url": "", "api_key": "", "api_mode": ""}});
    rig.service
        .execute(RolesEffect::PutConfig {
            step: ApplyStep::Subagents,
            patch: delegation.clone(),
        })
        .await;
    let requests = rig.rest.requests().await;
    let bodies: Vec<Value> = requests
        .iter()
        .filter(|item| item.method == "PUT" && item.path.starts_with("/api/config"))
        .map(|item| serde_json::from_slice(&item.body).unwrap_or(Value::Null))
        .collect();
    assert!(
        bodies
            .iter()
            .any(|body| body["config"]["delegation"]["base_url"] == "")
    );
    assert!(
        requests
            .iter()
            .filter(|item| item.method == "GET" && item.path.contains("/api/config"))
            .all(|item| item.path.contains("include_defaults=false"))
    );
}

#[tokio::test]
async fn moa_put_and_422() {
    let rig = rig("fixture", "fixture", true).await;
    let config = MoaConfigView {
        default_preset: "alpha".to_owned(),
        active_preset: String::new(),
        presets: vec![MoaPresetView {
            name: "alpha".to_owned(),
            enabled: true,
            references: vec![SlotView {
                provider: "public".to_owned(),
                model: "a".to_owned(),
                reasoning_effort: None,
                enabled: Some(true),
                extra: serde_json::Map::new(),
            }],
            aggregator: SlotView {
                provider: "public".to_owned(),
                model: "b".to_owned(),
                reasoning_effort: None,
                enabled: None,
                extra: serde_json::Map::new(),
            },
            reference_temperature: None,
            aggregator_temperature: None,
            reference_timeout: None,
            degraded_reference_policy: "loud".to_owned(),
            fanout: "user_turn".to_owned(),
            extra: serde_json::Map::new(),
        }],
        privacy_filter: Some("full".to_owned()),
    };
    rig.rest
        .response_body(
            "PUT",
            "/api/model/moa",
            br#"{"ok":true,"default_preset":"alpha","active_preset":"","presets":{}}"#.to_vec(),
        )
        .await;
    rig.service
        .execute(RolesEffect::SaveMoa(config.clone()))
        .await;
    let body = body_of(&rig.rest.requests().await, "PUT", "/api/model/moa");
    let keys: Vec<_> = body.as_object().expect("object").keys().cloned().collect();
    assert_eq!(
        keys,
        vec![
            "default_preset".to_owned(),
            "active_preset".to_owned(),
            "presets".to_owned()
        ]
    );
    assert!(body.get("privacy_filter").is_none());

    rig.rest
        .fail_status(
            "PUT",
            "/api/model/moa",
            422,
            Some("Invalid MoA config: preset 'alpha' reference 1: provider and model are required"),
        )
        .await;
    let inputs = rig.service.execute(RolesEffect::SaveMoa(config)).await;
    assert!(matches!(
        inputs.first(),
        Some(RolesInput::ApplyFailed { error: RolesError::Refused { status: 422, detail: Some(text) }, .. }) if text.contains("Invalid MoA config")
    ));
}

#[tokio::test]
async fn restart_required_503() {
    let rig = rig("fixture", "fixture", true).await;
    rig.rest
        .fail_status(
            "POST",
            "/api/model/set",
            503,
            Some("Restart required: test"),
        )
        .await;
    let inputs = rig
        .service
        .execute(RolesEffect::Assign {
            step: ApplyStep::MainModel,
            assignment: assignment(
                AssignScopeView::Main,
                "",
                "ollama-cloud",
                "minimax-m3.1-flash",
                EffortChange::Omit,
                "",
                false,
            ),
        })
        .await;
    assert!(matches!(
        inputs.first(),
        Some(RolesInput::ApplyFailed {
            error: RolesError::RestartRequired,
            ..
        })
    ));
    let posts = rig
        .rest
        .requests()
        .await
        .into_iter()
        .filter(|item| item.method == "POST")
        .count();
    assert_eq!(posts, 1, "a restart refusal is not retried");
}

#[tokio::test]
async fn no_socket_writes() {
    let rig = rig("fixture", "fixture", true).await;
    rig.service.execute(RolesEffect::LoadAll).await;
    rig.service
        .execute(RolesEffect::LoadCatalog { refresh: false })
        .await;
    rig.service
        .execute(RolesEffect::Assign {
            step: ApplyStep::MainModel,
            assignment: assignment(
                AssignScopeView::Main,
                "",
                "ollama-cloud",
                "m",
                EffortChange::Omit,
                "",
                false,
            ),
        })
        .await;
    rig.service
        .execute(RolesEffect::PutConfig {
            step: ApplyStep::Fallbacks,
            patch: json!({"fallback_providers": []}),
        })
        .await;
    rig.service
        .execute(RolesEffect::Assign {
            step: ApplyStep::Helper {
                task: "vision".to_owned(),
            },
            assignment: assignment(
                AssignScopeView::Auxiliary,
                "vision",
                "auto",
                "",
                EffortChange::Inherit,
                "",
                false,
            ),
        })
        .await;
    rig.service
        .execute(RolesEffect::PutConfig {
            step: ApplyStep::Subagents,
            patch: json!({"delegation": {"provider": "", "model": ""}}),
        })
        .await;
    rig.service
        .execute(RolesEffect::SaveMoa(MoaConfigView {
            default_preset: "alpha".to_owned(),
            active_preset: String::new(),
            presets: Vec::new(),
            privacy_filter: None,
        }))
        .await;
    let methods: std::collections::BTreeSet<_> = rig
        .gateway
        .seen()
        .iter()
        .filter_map(|frame| frame.method.clone())
        .collect();
    assert_eq!(
        methods,
        std::collections::BTreeSet::from(["model.options".to_owned()])
    );
    for frame in rig.gateway.seen() {
        if frame.method.as_deref() == Some("model.options") {
            assert_eq!(frame.params["explicit_only"], true);
            assert_eq!(frame.params["include_unconfigured"], true);
            assert!(frame.params.get("session_id").is_none());
        }
    }
    let requests = rig.rest.requests().await;
    assert!(requests.iter().any(|item| item.method == "POST"));
    assert!(
        requests
            .iter()
            .any(|item| item.method == "PUT" && item.path.contains("/api/config"))
    );
    assert!(
        requests
            .iter()
            .any(|item| item.method == "PUT" && item.path.contains("/api/model/moa"))
    );
}

#[tokio::test]
async fn other_profile_and_not_connected() {
    let other = rig("other", "fixture", true).await;
    other
        .service
        .execute(RolesEffect::LoadCatalog { refresh: false })
        .await;
    let frames = other.gateway.seen();
    let options = frames
        .iter()
        .find(|frame| frame.method.as_deref() == Some("model.options"))
        .expect("catalogue");
    assert_eq!(options.params["profile"], "other");
    let requests = other.rest.requests().await;
    assert!(
        !requests.is_empty()
            || frames
                .iter()
                .any(|frame| frame.method.as_deref() == Some("model.options"))
    );
    assert_no_profile(&requests);

    let unserved = ModelRolesService::new(ModelRolesOpen {
        profile: "missing".to_owned(),
        connection_profile: "fixture".to_owned(),
        api_origin: Some(other.rest.origin()),
        rest: None,
        socket: None,
        served_profiles: vec!["fixture".to_owned()],
    });
    let inputs = unserved.execute(RolesEffect::LoadAll).await;
    assert!(
        matches!(inputs.first(), Some(RolesInput::NotServed { profile }) if profile == "missing")
    );
    assert!(other.rest.requests().await.len() == requests.len());

    let offline = ModelRolesService::new(ModelRolesOpen {
        profile: "fixture".to_owned(),
        connection_profile: "fixture".to_owned(),
        api_origin: None,
        rest: None,
        socket: None,
        served_profiles: vec!["fixture".to_owned()],
    });
    let inputs = offline.execute(RolesEffect::LoadAll).await;
    assert_eq!(inputs, vec![RolesInput::NotConnected]);
}
