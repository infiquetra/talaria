//! Live reads and the recorded model-role fixtures.
//!
//! The recorder and the two live checks are ignored. The allowlist check is not.

#![allow(clippy::too_many_lines)]

#[path = "support/model_roles_scrub.rs"]
mod model_roles_scrub;

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use hermes_client::{
    CallOptions, Client, DiscoverOptions, DiscoveryError, EnsurePolicy, NativeHttpMinter,
    ProfileLookupError, Purpose, RestClient, RestOptions, Scope, connect, default_home,
    discover_with, handshake, mint_ticket,
};
use model_roles_scrub::{allowlist_violations, provider_map, scrub_fixture};
use serde_json::{Value, json};

const PROFILE: &str = "fixture";
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

fn hermes_home() -> PathBuf {
    std::env::var_os("HERMES_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(default_home)
}

fn sanitize(reason: &str) -> String {
    let account = std::env::var("USER").unwrap_or_default();
    reason
        .split_whitespace()
        .map(|word| {
            let path = word
                .split(['\'', '"', '(', ')', '=', ':'])
                .any(|piece| Path::new(piece).is_absolute());
            let named = !account.is_empty() && word.contains(&account);
            if path || named { "<path>" } else { word }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn fixture_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/rest/model-roles")
}

fn envelope(method: &str, path: &str, status: u16, body: Value) -> Value {
    json!({
        "method": method,
        "path_without_query": path,
        "status": status,
        "body": body,
    })
}

fn write_fixture(dir: &Path, name: &str, document: &Value) {
    let text = serde_json::to_string_pretty(document).expect("fixture json");
    let path = dir.join(name);
    std::fs::write(&path, text).unwrap_or_else(|_| panic!("could not write {name}"));
}

/// Record the five scrubbed replies from the running gateway.
#[tokio::test]
#[ignore = "records fixtures from the running gateway"]
async fn record_fixtures() {
    let out = match std::env::var("TALARIA_FIXTURE_OUT") {
        Ok(value) => PathBuf::from(value),
        Err(_) => panic!("TALARIA_FIXTURE_OUT is not set"),
    };
    std::fs::create_dir_all(&out)
        .unwrap_or_else(|_| panic!("could not create the fixture directory"));
    let home = hermes_home();
    let options = DiscoverOptions {
        control_timeout: Duration::from_secs(5),
        ensure: EnsurePolicy::Never,
    };
    let endpoint = discover_with(&home, &options)
        .await
        .unwrap_or_else(|error| {
            let reason = match error {
                DiscoveryError::NoGateway => {
                    "no Hermes gateway is running for this home".to_owned()
                }
                other => sanitize(&other.to_string()),
            };
            panic!("discovery failed: {reason}");
        });
    if let Some(reason) = hermes_contract::pinned::live_skip_reason(endpoint.code_sha()) {
        panic!("gateway precondition failed: {}", sanitize(&reason));
    }
    let profile = endpoint.resolve_profile(PROFILE).unwrap_or_else(|error| {
        let reason = match error {
            ProfileLookupError::NeverDiscovered { name } => {
                format!("the gateway never discovered a profile named {name}")
            }
            other => sanitize(&other.to_string()),
        };
        panic!("profile lookup failed: {reason}");
    });
    let endpoint = Arc::new(endpoint);
    let minter = Arc::new(NativeHttpMinter::new(
        home.clone(),
        Arc::clone(&endpoint),
        profile.clone(),
    ));
    let rest = RestClient::new(endpoint.api_origin(), minter, RestOptions::default())
        .unwrap_or_else(|error| panic!("rest client: {}", sanitize(&error.to_string())));
    let info = rest
        .get_json("/api/model/info")
        .await
        .unwrap_or_else(|error| panic!("model info: {}", sanitize(&error.to_string())));
    let auxiliary = rest
        .get_json("/api/model/auxiliary")
        .await
        .unwrap_or_else(|error| panic!("auxiliary: {}", sanitize(&error.to_string())));
    let moa = rest
        .get_json("/api/model/moa")
        .await
        .unwrap_or_else(|error| panic!("moa: {}", sanitize(&error.to_string())));
    let raw = rest
        .get_json("/api/config?include_defaults=false")
        .await
        .unwrap_or_else(|error| panic!("raw config: {}", sanitize(&error.to_string())));
    let ticket = mint_ticket(
        &home,
        endpoint.as_ref(),
        &profile,
        Purpose::Interactive,
        Scope::profile(),
    )
    .await
    .unwrap_or_else(|error| panic!("ticket: {}", sanitize(&error.to_string())));
    let mut connection = connect(endpoint.as_ref(), ticket)
        .await
        .unwrap_or_else(|error| panic!("connect: {}", sanitize(&error.to_string())));
    handshake(&mut connection)
        .await
        .unwrap_or_else(|error| panic!("handshake: {}", sanitize(&error.to_string())));
    let (client, _inbound) = Client::start(connection.into_parts());
    let catalogue = client
        .call_raw(
            "model.options",
            json!({"explicit_only": true, "include_unconfigured": true}),
            CallOptions::default(),
        )
        .await
        .unwrap_or_else(|error| panic!("model options: {}", sanitize(&error.to_string())));
    let providers = provider_map(&catalogue);
    let files = [
        (
            "model-info.json",
            envelope("GET", "/api/model/info", 200, info),
        ),
        (
            "auxiliary.json",
            envelope("GET", "/api/model/auxiliary", 200, auxiliary),
        ),
        ("moa.json", envelope("GET", "/api/model/moa", 200, moa)),
        ("config-raw.json", envelope("GET", "/api/config", 200, raw)),
        (
            "model-options.json",
            envelope("model.options", "model.options", 200, catalogue),
        ),
    ];
    for (name, document) in files {
        let scrubbed = scrub_fixture(name, &document, &providers);
        let violations = allowlist_violations(name, &scrubbed);
        assert!(
            violations.is_empty(),
            "{name} broke the allowlist: {}",
            violations.join(", ")
        );
        write_fixture(&out, name, &scrubbed);
    }
}

fn skip_live(reason: &str) {
    let reason = sanitize(reason);
    println!("LIVE_MODEL_ROLES_RESULT: SKIPPED ({reason})");
    if std::env::var("TALARIA_LIVE_REQUIRE").ok().as_deref() == Some("1") {
        panic!("live test skipped: {reason}");
    }
}

/// Read every section from the running gateway and check the decoded shapes.
#[tokio::test]
#[ignore = "reads the running gateway"]
async fn reads_all_sections() {
    let home = hermes_home();
    let options = DiscoverOptions {
        control_timeout: Duration::from_secs(5),
        ensure: EnsurePolicy::Never,
    };
    let endpoint = match discover_with(&home, &options).await {
        Ok(endpoint) => endpoint,
        Err(error) => {
            let reason = match error {
                DiscoveryError::NoGateway => {
                    "no Hermes gateway is running for this home".to_owned()
                }
                other => sanitize(&other.to_string()),
            };
            skip_live(&reason);
            return;
        }
    };
    if let Some(reason) = hermes_contract::pinned::live_skip_reason(endpoint.code_sha()) {
        skip_live(&reason);
        return;
    }
    let profile = match endpoint.resolve_profile(PROFILE) {
        Ok(profile) => profile,
        Err(error) => {
            skip_live(&sanitize(&error.to_string()));
            return;
        }
    };
    let endpoint = Arc::new(endpoint);
    let minter = Arc::new(NativeHttpMinter::new(
        home.clone(),
        Arc::clone(&endpoint),
        profile.clone(),
    ));
    let rest = match RestClient::new(endpoint.api_origin(), minter, RestOptions::default()) {
        Ok(rest) => rest,
        Err(error) => {
            skip_live(&sanitize(&error.to_string()));
            return;
        }
    };
    let info = hermes_client::model_info(&rest)
        .await
        .expect("model info decodes");
    let auxiliary = hermes_client::auxiliary_models(&rest)
        .await
        .expect("auxiliary decodes");
    let moa = hermes_client::moa_config(&rest).await.expect("moa decodes");
    let _raw = hermes_client::config_read(&rest, false)
        .await
        .expect("raw config decodes");
    let tasks: Vec<&str> = auxiliary
        .tasks
        .iter()
        .map(|task| task.task.as_str())
        .collect();
    assert!(
        tasks.starts_with(&BUILTIN_TASKS),
        "helper order starts with the twelve built-in slots"
    );
    if info.config_context_length > 0 {
        assert_eq!(info.effective_context_length, info.config_context_length);
    } else {
        assert_eq!(info.effective_context_length, info.auto_context_length);
    }
    assert!(
        !moa.presets.is_empty(),
        "a profile always has at least the example preset"
    );
    let ticket = mint_ticket(
        &home,
        endpoint.as_ref(),
        &profile,
        Purpose::Interactive,
        Scope::profile(),
    )
    .await
    .expect("interactive ticket");
    let mut connection = connect(endpoint.as_ref(), ticket).await.expect("socket");
    handshake(&mut connection).await.expect("handshake");
    let (client, _inbound) = Client::start(connection.into_parts());
    let catalogue = client
        .call_raw(
            "model.options",
            json!({"explicit_only": true, "include_unconfigured": true}),
            CallOptions::default(),
        )
        .await
        .expect("catalogue");
    let skeleton = catalogue
        .get("providers")
        .and_then(Value::as_array)
        .is_some_and(|rows| {
            rows.iter()
                .any(|row| row.get("authenticated") == Some(&Value::Bool(false)))
        });
    assert!(
        skeleton,
        "the catalogue holds a provider that is not set up"
    );
    println!("LIVE_MODEL_ROLES_RESULT: PASSED");
}

/// Write fallbacks, one helper, the delegation route, and a preset, then put them back.
#[tokio::test]
#[ignore = "writes the fixture profile and restores it"]
async fn round_trip_writes_on_fixture() {
    if std::env::var("TALARIA_PROBE_WRITES").ok().as_deref() != Some("1") {
        println!("LIVE_MODEL_ROLES_RESULT: SKIPPED (write opt-in not set)");
        if std::env::var("TALARIA_LIVE_REQUIRE").ok().as_deref() == Some("1") {
            panic!("live test skipped: write opt-in not set");
        }
        return;
    }
    let home = hermes_home();
    let options = DiscoverOptions {
        control_timeout: Duration::from_secs(5),
        ensure: EnsurePolicy::Never,
    };
    let endpoint = discover_with(&home, &options).await.expect("gateway");
    let profile = endpoint.resolve_profile(PROFILE).expect("fixture profile");
    let endpoint = Arc::new(endpoint);
    let minter = Arc::new(NativeHttpMinter::new(home, Arc::clone(&endpoint), profile));
    let rest =
        RestClient::new(endpoint.api_origin(), minter, RestOptions::default()).expect("rest");
    let raw = hermes_client::config_read(&rest, false)
        .await
        .expect("raw config");
    let auxiliary = hermes_client::auxiliary_models(&rest)
        .await
        .expect("auxiliary");
    let moa = hermes_client::moa_config(&rest).await.expect("moa");
    let original_fallbacks = raw.get("fallback_providers").cloned().unwrap_or(json!([]));
    let patch = json!({"fallback_providers": [{"provider": "ollama-cloud", "model": "minimax-m3.1-flash"}]});
    hermes_client::config_write(&rest, &patch)
        .await
        .expect("write fallbacks");
    let read_back = hermes_client::config_read(&rest, false)
        .await
        .expect("read fallbacks");
    assert_eq!(
        read_back["fallback_providers"][0]["provider"],
        "ollama-cloud"
    );
    assert_eq!(
        read_back["fallback_providers"][0]["model"],
        "minimax-m3.1-flash"
    );
    hermes_client::config_write(&rest, &json!({"fallback_providers": original_fallbacks}))
        .await
        .expect("restore fallbacks");

    let title = auxiliary
        .tasks
        .iter()
        .find(|task| task.task == "title_generation")
        .expect("title generation slot");
    let was_auto = title.provider == "auto" && title.model.is_empty();
    let pin = hermes_client::ModelAssignment {
        scope: hermes_client::AssignScope::Auxiliary,
        provider: "ollama-cloud".to_owned(),
        model: "minimax-m3.1-flash".to_owned(),
        task: "title_generation".to_owned(),
        reasoning_effort: hermes_client::EffortField::Level("low".to_owned()),
        base_url: String::new(),
        confirm_expensive_model: false,
    };
    let first = hermes_client::assign_model(&rest, &pin)
        .await
        .expect("pin helper");
    let pin = if matches!(
        first,
        hermes_client::AssignmentReply::ConfirmRequired { .. }
    ) {
        hermes_client::ModelAssignment {
            confirm_expensive_model: true,
            ..pin
        }
    } else {
        pin
    };
    if matches!(
        first,
        hermes_client::AssignmentReply::ConfirmRequired { .. }
    ) {
        hermes_client::assign_model(&rest, &pin)
            .await
            .expect("confirm helper");
    }
    if was_auto {
        let restore = hermes_client::ModelAssignment {
            provider: title.provider.clone(),
            model: title.model.clone(),
            reasoning_effort: match title.reasoning_effort.clone() {
                Some(level) if !level.is_empty() => hermes_client::EffortField::Level(level),
                _ => hermes_client::EffortField::Inherit,
            },
            confirm_expensive_model: true,
            ..pin
        };
        hermes_client::assign_model(&rest, &restore)
            .await
            .expect("restore helper");
        let after = hermes_client::auxiliary_models(&rest)
            .await
            .expect("read helper");
        let slot = after
            .tasks
            .iter()
            .find(|task| task.task == "title_generation")
            .expect("slot");
        assert_eq!(slot.provider, title.provider);
        assert_eq!(slot.model, title.model);
        assert_eq!(slot.base_url, title.base_url);
        assert_eq!(slot.reasoning_effort, title.reasoning_effort);
    }

    let delegation = raw.get("delegation").cloned().unwrap_or(json!({}));
    hermes_client::config_write(
        &rest,
        &json!({"delegation": {"provider": "ollama-cloud", "model": "minimax-m3.1-flash"}}),
    )
    .await
    .expect("write delegation");
    let read_delegation = hermes_client::config_read(&rest, false)
        .await
        .expect("read delegation");
    assert_eq!(read_delegation["delegation"]["provider"], "ollama-cloud");
    hermes_client::config_write(&rest, &json!({"delegation": delegation}))
        .await
        .expect("restore delegation");

    let had_presets = raw.get("moa").and_then(|moa| moa.get("presets")).is_some();
    if had_presets {
        let mut written = moa.clone();
        written.presets.push(hermes_client::NamedPreset {
            name: "talaria-live".to_owned(),
            preset: hermes_client::MoaPreset {
                enabled: false,
                reference_models: vec![hermes_client::MoaSlot {
                    provider: "ollama-cloud".to_owned(),
                    model: "minimax-m3.1-flash".to_owned(),
                    reasoning_effort: None,
                    enabled: Some(true),
                    extra: serde_json::Map::new(),
                }],
                aggregator: hermes_client::MoaSlot {
                    provider: "ollama-cloud".to_owned(),
                    model: "minimax-m3.1-flash".to_owned(),
                    reasoning_effort: None,
                    enabled: Some(true),
                    extra: serde_json::Map::new(),
                },
                reference_temperature: None,
                aggregator_temperature: None,
                reference_timeout: None,
                degraded_reference_policy: "loud".to_owned(),
                fanout: "user_turn".to_owned(),
                extra: serde_json::Map::new(),
            },
        });
        let saved = hermes_client::save_moa(&rest, &written)
            .await
            .expect("add preset");
        let added = saved
            .presets
            .iter()
            .find(|preset| preset.name == "talaria-live")
            .expect("preset");
        assert!(added.preset.reference_timeout.is_none());
        assert!(added.preset.reference_temperature.is_none());
        assert!(added.preset.aggregator_temperature.is_none());
        hermes_client::save_moa(&rest, &moa)
            .await
            .expect("remove preset");
    }
    println!("LIVE_MODEL_ROLES_RESULT: PASSED");
}

fn synthetic_reply() -> Value {
    json!({
        "method": "GET",
        "path_without_query": "/api/model/auxiliary",
        "status": 200,
        "body": {
            "x_private": "do-not-keep",
            "warning": "drop this text",
            "pricing": {"input": "9"},
            "model": "public-model",
            "provider": "public",
            "auto_context_length": 1,
            "config_context_length": 0,
            "effective_context_length": 1,
            "capabilities": {"supports_tools": true, "pricing": {"hidden": true}},
            "providers": [
                {
                    "slug": "public",
                    "name": "Public",
                    "models": ["a", "b", "c", "d", "e", "f"],
                    "authenticated": true,
                    "is_user_defined": false,
                    "api_url": "https://user:pw@lab.example:8443/v1?team=x",
                    "source": "built-in",
                    "warning": "drop"
                },
                {
                    "slug": "my-lab",
                    "name": "My Lab",
                    "models": ["secret-model"],
                    "authenticated": true,
                    "is_user_defined": true,
                    "api_url": "https://user:pw@lab.example:8443/v1?team=x",
                    "pricing": {"hidden": true}
                },
                {"slug": "moa", "name": "Mixture of Agents", "models": ["preset"], "source": "virtual"}
            ],
            "tasks": [
                {
                    "task": "vision",
                    "provider": "auto",
                    "model": "",
                    "base_url": "",
                    "reasoning_effort": null,
                    "local_endpoint": false,
                    "x_private": true
                },
                {
                    "task": "my_task",
                    "provider": "my-lab",
                    "model": "secret-model",
                    "base_url": "https://user:pw@lab.example:8443/v1?team=x",
                    "reasoning_effort": null,
                    "local_endpoint": false,
                    "label": "Mine",
                    "hint": "H",
                    "plugin": "my-plugin",
                    "inherit_from": "vision",
                    "effective": {
                        "provider": "my-lab",
                        "model": "secret-model",
                        "base_url": "http://127.0.0.1:9/v1",
                        "x_private": 1
                    }
                }
            ],
            "main": {"provider": "public", "model": "a", "x_private": 1},
            "fallback_providers": [
                "public/a",
                "my-lab/secret-model",
                {
                    "provider": "my-lab",
                    "model": "secret-model",
                    "base_url": "https://user:pw@lab.example:8443/v1?team=x",
                    "api_key": "not-a-token",
                    "key_env": "HOME_KEY",
                    "api_key_env": "OTHER_KEY",
                    "api_mode": "chat",
                    "x_private": "no"
                }
            ],
            "delegation": {
                "provider": "my-lab",
                "model": "secret-model",
                "base_url": "https://user:pw@lab.example:8443/v1?team=x",
                "api_key": "not-a-token",
                "api_mode": "",
                "concurrency": 4
            },
            "model_context_length": 0,
            "agent": {"reasoning_effort": "low", "service_tier": "drop"},
            "moa": {"presets": {"alpha": {"enabled": true, "secret": "no"}}, "privacy_filter": "off"},
            "presets": {
                "alpha": {
                    "enabled": true,
                    "reference_models": [{"provider": "my-lab", "model": "secret-model", "enabled": true, "x_private": 1}],
                    "aggregator": {"provider": "public", "model": "a"},
                    "fanout": "user_turn",
                    "x_private": 1
                }
            },
            "default_preset": "alpha",
            "active_preset": "",
            "privacy_filter": "off",
            "ok": true
        }
    })
}

#[test]
fn fixture_scrub_is_an_allowlist() {
    let document = synthetic_reply();
    let providers = provider_map(&document["body"]);
    let options = scrub_fixture("model-options.json", &document, &providers);
    let body = &options["body"];
    assert!(body.get("x_private").is_none());
    assert!(body.get("warning").is_none());
    assert!(body.get("pricing").is_none());
    let rows = body["providers"].as_array().expect("providers");
    assert!(rows.iter().all(|row| row["slug"] != "moa"));
    let custom = rows
        .iter()
        .find(|row| row["slug"] == "custom-1")
        .expect("renamed provider");
    assert_eq!(custom["name"], "custom-1");
    assert_eq!(custom["models"][0], "model-1");
    assert_eq!(custom["api_url"], "https://endpoint.invalid");
    let public = rows
        .iter()
        .find(|row| row["slug"] == "public")
        .expect("public provider");
    assert_eq!(public["models"].as_array().map(Vec::len), Some(5));
    assert!(allowlist_violations("model-options.json", &options).is_empty());

    let auxiliary = scrub_fixture("auxiliary.json", &document, &providers);
    let plugin = auxiliary["body"]["tasks"]
        .as_array()
        .expect("tasks")
        .iter()
        .find(|task| task.get("plugin").is_some())
        .expect("plugin row");
    assert_eq!(plugin["plugin"], "plugin-1");
    assert_eq!(plugin["label"], "Plugin task 1");
    assert_eq!(plugin["hint"], "Plugin hint 1");
    assert_eq!(plugin["task"], "plugin_task_1");
    assert_eq!(plugin["provider"], "custom-1");
    assert_eq!(plugin["base_url"], "https://endpoint.invalid");
    assert!(plugin.get("x_private").is_none());
    assert!(allowlist_violations("auxiliary.json", &auxiliary).is_empty());

    let config = scrub_fixture("config-raw.json", &document, &providers);
    let entries = config["body"]["fallback_providers"]
        .as_array()
        .expect("fallbacks");
    assert_eq!(entries[0], "public/a");
    assert_eq!(entries[1], "custom-1/model-1");
    assert_eq!(entries[2]["provider"], "custom-1");
    assert_eq!(entries[2]["key_env"], "EXAMPLE_KEY_ENV");
    assert_eq!(entries[2]["api_key_env"], "EXAMPLE_KEY_ENV");
    assert_eq!(entries[2]["api_key"], "<redacted>");
    assert_eq!(entries[2]["base_url"], "https://endpoint.invalid");
    assert!(entries[2].get("x_private").is_none());
    assert_eq!(config["body"]["delegation"]["api_key"], "<redacted>");
    assert!(config["body"]["delegation"].get("concurrency").is_none());
    assert_eq!(config["body"]["moa"]["presets"]["alpha"], json!({}));
    assert!(config["body"].get("x_private").is_none());
    assert!(allowlist_violations("config-raw.json", &config).is_empty());

    let dir = fixture_dir();
    let entries =
        std::fs::read_dir(&dir).unwrap_or_else(|_| panic!("recorded fixture directory is missing"));
    let mut files = Vec::new();
    for entry in entries {
        let entry = entry.unwrap_or_else(|_| panic!("recorded fixture directory is unreadable"));
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.ends_with(".json") {
            files.push(name);
        }
    }
    assert!(!files.is_empty(), "recorded fixture directory is empty");
    for name in files {
        let path = dir.join(&name);
        let text = std::fs::read_to_string(&path)
            .unwrap_or_else(|_| panic!("recorded fixture is not available"));
        let document: Value = serde_json::from_str(&text)
            .unwrap_or_else(|_| panic!("recorded fixture is not json: {name}"));
        let violations = allowlist_violations(&name, &document);
        assert!(
            violations.is_empty(),
            "{name} broke the allowlist: {}",
            violations.join(", ")
        );
    }
}
