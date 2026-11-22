//! The model roles service sends the dashboard calls and nothing else.

#![allow(clippy::too_many_lines)]

use crate::client_support::mock_rest;
use crate::mock_gateway;

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
    // The page has no socket. The mock stays up to show nothing is ever sent to it.
    let (gateway, _socket, _inbound) = MockGateway::start().await;
    gateway.set_model_options(catalogue(), catalogue());
    gateway.set_connection_profile(connection);
    let client = hermes_client::RestClient::new(
        &rest.origin(),
        Arc::new(CountingMinter::default()),
        hermes_client::RestOptions::default(),
    )
    .expect("loopback client");
    let service = ModelRolesService::new(ModelRolesOpen {
        profile: profile.to_owned(),
        api_origin: origin.then(|| rest.origin()),
        rest: origin.then_some(client),
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
    let keys: std::collections::BTreeSet<_> =
        body.as_object().expect("object").keys().cloned().collect();
    assert_eq!(
        keys,
        std::collections::BTreeSet::from([
            "default_preset".to_owned(),
            "active_preset".to_owned(),
            "presets".to_owned()
        ])
    );
    // A parsed Value sorts its keys, so the order is checked on the bytes sent.
    let wire = rig
        .rest
        .requests()
        .await
        .into_iter()
        .find(|item| item.method == "PUT" && item.path == "/api/model/moa")
        .map(|item| String::from_utf8_lossy(&item.body).into_owned())
        .expect("moa request");
    let at = |key: &str| wire.find(&format!("\"{key}\":")).expect("key sent");
    assert!(at("default_preset") < at("active_preset") && at("active_preset") < at("presets"));
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
    let methods: Vec<_> = rig
        .gateway
        .seen()
        .iter()
        .filter_map(|frame| frame.method.clone())
        .collect();
    assert!(
        methods.is_empty(),
        "the page sends nothing on the socket: {methods:?}"
    );
    let requests = rig.rest.requests().await;
    assert_eq!(options_requests(&requests).len(), 1);
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
    let requests = other.rest.requests().await;
    assert_eq!(options_requests(&requests).len(), 1);
    assert!(other.gateway.seen().is_empty(), "the socket is never used");
    assert_no_profile(&requests);

    let unserved = ModelRolesService::new(ModelRolesOpen {
        profile: "missing".to_owned(),
        api_origin: Some(other.rest.origin()),
        rest: None,
        served_profiles: vec!["fixture".to_owned()],
    });
    for effect in [
        RolesEffect::LoadAll,
        RolesEffect::LoadCatalog { refresh: false },
    ] {
        let inputs = unserved.execute(effect).await;
        assert!(
            matches!(inputs.first(), Some(RolesInput::NotServed { profile }) if profile == "missing")
        );
    }
    assert!(other.rest.requests().await.len() == requests.len());

    let offline = ModelRolesService::new(ModelRolesOpen {
        profile: "fixture".to_owned(),
        api_origin: None,
        rest: None,
        served_profiles: vec!["fixture".to_owned()],
    });
    for effect in [
        RolesEffect::LoadAll,
        RolesEffect::LoadCatalog { refresh: false },
    ] {
        assert_eq!(
            offline.execute(effect).await,
            vec![RolesInput::NotConnected]
        );
    }
}

/// A failed list read carries its reason to the page.
#[tokio::test]
async fn a_failed_model_list_carries_its_reason() {
    let server = rig("other", "fixture", true).await;
    server
        .rest
        .fail_status("GET", "/api/model/options", 500, None)
        .await;
    let inputs = server
        .service
        .execute(RolesEffect::LoadCatalog { refresh: false })
        .await;
    assert_eq!(
        inputs,
        vec![RolesInput::CatalogFailed {
            error: RolesError::Server { status: 500 }
        }]
    );

    let forbidden = rig("other", "fixture", true).await;
    forbidden
        .rest
        .fail_status(
            "GET",
            "/api/model/options",
            403,
            Some("profile_scope_mismatch"),
        )
        .await;
    let inputs = forbidden
        .service
        .execute(RolesEffect::LoadCatalog { refresh: false })
        .await;
    assert!(
        matches!(
            inputs.as_slice(),
            [RolesInput::CatalogFailed { error: RolesError::Forbidden { detail } }]
                if detail.contains("profile_scope_mismatch")
        ),
        "{inputs:?}"
    );

    let dropped = rig("other", "fixture", true).await;
    dropped
        .rest
        .drop_after_request("GET", "/api/model/options")
        .await;
    let inputs = dropped
        .service
        .execute(RolesEffect::LoadCatalog { refresh: false })
        .await;
    assert!(
        matches!(
            inputs.as_slice(),
            [RolesInput::CatalogFailed {
                error: RolesError::NotSent { .. }
            }]
        ),
        "{inputs:?}"
    );
}

fn options_requests(requests: &[mock_rest::RecordedRequest]) -> Vec<String> {
    requests
        .iter()
        .filter(|item| item.method == "GET" && item.path.starts_with("/api/model/options"))
        .map(|item| item.path.clone())
        .collect()
}

fn expected_catalogue() -> talaria_core::model_roles::RouteCatalog {
    talaria_core::model_roles::RouteCatalog::from_result(
        &serde_json::from_value(catalogue()).expect("catalogue"),
    )
}

/// The route list is read over the page's own REST client, ticketed for the page's profile, so
/// another profile's list is never asked for over the connection's socket.
#[tokio::test]
async fn the_model_list_comes_over_rest_for_every_profile() {
    for profile in ["other", "fixture"] {
        let rig = rig(profile, "fixture", true).await;
        rig.rest
            .script("GET", "/api/model/options", 200, catalogue());
        let inputs = rig
            .service
            .execute(RolesEffect::LoadCatalog { refresh: false })
            .await;
        let requests = rig.rest.requests().await;
        let paths = options_requests(&requests);
        assert_eq!(paths.len(), 1, "{profile}: {paths:?}");
        assert!(paths[0].contains("explicit_only=true"), "{}", paths[0]);
        assert!(
            paths[0].contains("include_unconfigured=true"),
            "{}",
            paths[0]
        );
        assert!(!paths[0].contains("refresh"), "{}", paths[0]);
        assert_no_profile(&requests);
        assert!(
            rig.gateway
                .seen()
                .iter()
                .all(|frame| frame.method.as_deref() != Some("model.options")),
            "{profile}: nothing goes over the socket"
        );
        assert_eq!(
            inputs,
            vec![RolesInput::CatalogLoaded(expected_catalogue())],
            "{profile}"
        );
    }
}

#[tokio::test]
async fn a_model_list_refresh_asks_hermes_to_refresh() {
    let rig = rig("other", "fixture", true).await;
    rig.rest
        .script("GET", "/api/model/options", 200, catalogue());
    rig.service
        .execute(RolesEffect::LoadCatalog { refresh: true })
        .await;
    let paths = options_requests(&rig.rest.requests().await);
    assert_eq!(paths.len(), 1);
    assert!(paths[0].contains("refresh=true"), "{}", paths[0]);
}

/// Hermes can take longer than an ordinary read to build the list on a cold cache, so the list
/// has a deadline of its own instead of the client's read deadline.
#[tokio::test]
async fn a_slow_model_list_still_arrives() {
    let rest = MockRest::start().await;
    rest.script("GET", "/api/model/options", 200, catalogue());
    rest.delay(
        "GET",
        "/api/model/options",
        std::time::Duration::from_millis(1500),
    )
    .await;
    let client = hermes_client::RestClient::new(
        &rest.origin(),
        Arc::new(CountingMinter::default()),
        hermes_client::RestOptions {
            get_deadline: std::time::Duration::from_secs(1),
            ..hermes_client::RestOptions::default()
        },
    )
    .expect("loopback client");
    let service = ModelRolesService::new(ModelRolesOpen {
        profile: "other".to_owned(),
        api_origin: Some(rest.origin()),
        rest: Some(client),
        served_profiles: vec!["fixture".to_owned(), "other".to_owned()],
    });
    let inputs = service
        .execute(RolesEffect::LoadCatalog { refresh: false })
        .await;
    assert_eq!(
        inputs,
        vec![RolesInput::CatalogLoaded(expected_catalogue())]
    );
    // An ordinary read on the same client does run out.
    rest.delay(
        "GET",
        "/api/model/info",
        std::time::Duration::from_millis(1500),
    )
    .await;
    let inputs = service
        .execute(RolesEffect::Load {
            read: talaria_core::model_roles::Read::Info,
        })
        .await;
    assert!(
        matches!(
            inputs.as_slice(),
            [RolesInput::LoadFailed {
                error: RolesError::NotSent { .. },
                ..
            }]
        ),
        "{inputs:?}"
    );
}
