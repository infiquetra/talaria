//! Live provider lists. Ignored unless a person asks for them.
//!
//! Prints `LIVE_PROVIDERS_RESULT: PASSED` or `SKIPPED (<reason>)`. With
//! `TALARIA_LIVE_REQUIRE=1` a skip fails. The write test also needs
//! `TALARIA_PROBE_WRITES=1` and the `fixture` profile.

use std::collections::BTreeSet;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use hermes_client::rest::{NativeHttpMinter, RestClient, RestOptions};
use hermes_client::{
    Client, DiscoverOptions, DiscoveryError, Endpoint, EnsurePolicy, ProfileId, ProfileLookupError,
    Purpose, Scope, connect, default_home, discover_with, handshake, mint_ticket,
};
use serde_json::{Value, json};
use talaria_app::providers::ProviderService;
use talaria_core::providers::{
    ApiKeyChange, EndpointDraft, ProvidersEffect, ProvidersInput, ProvidersState, update,
};
use zeroize::Zeroizing;

const JUNK_KEY: &str = "sk-test-junk-openrouter-0000";
const ENDPOINT_KEY: &str = "sk-test-ENDPOINT-0123456789abcdef";

fn scrub(reason: &str) -> String {
    reason
        .split_whitespace()
        .map(|token| if token.contains('/') { "<path>" } else { token })
        .collect::<Vec<_>>()
        .join(" ")
}

fn skip(reason: &str) {
    let reason = scrub(reason);
    println!("LIVE_PROVIDERS_RESULT: SKIPPED ({reason})");
    if std::env::var("TALARIA_LIVE_REQUIRE").ok().as_deref() == Some("1") {
        panic!("live test skipped: {reason}");
    }
}

fn hermes_home() -> PathBuf {
    std::env::var_os("HERMES_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(default_home)
}

fn open_rest(
    home: &PathBuf,
    endpoint: &Arc<Endpoint>,
    profile: &ProfileId,
    origin: &str,
) -> Result<RestClient, String> {
    let minter = Arc::new(NativeHttpMinter::new(
        home.clone(),
        Arc::clone(endpoint),
        profile.clone(),
    ));
    RestClient::new(origin, minter, RestOptions::default()).map_err(|error| error.to_string())
}

struct Attached {
    runtime: tokio::runtime::Runtime,
    client: Client,
    rest: RestClient,
    probe: RestClient,
}

fn attach() -> Result<Attached, String> {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|error| error.to_string())?;
    let (client, rest, probe) = runtime.block_on(async {
        let home = hermes_home();
        let options = DiscoverOptions {
            control_timeout: Duration::from_secs(5),
            ensure: EnsurePolicy::Never,
        };
        let endpoint = discover_with(&home, &options)
            .await
            .map_err(|error| match error {
                DiscoveryError::NoGateway => {
                    "no Hermes gateway is running for this home".to_owned()
                }
                other => other.to_string(),
            })?;
        if let Some(reason) = hermes_contract::pinned::live_skip_reason(endpoint.code_sha()) {
            return Err(reason);
        }
        let origin = endpoint.api_origin().to_owned();
        let profile = endpoint
            .resolve_profile("fixture")
            .map_err(|error| match error {
                ProfileLookupError::NeverDiscovered { name } => {
                    format!("the gateway never discovered a profile named {name}")
                }
                other => other.to_string(),
            })?;
        let endpoint = Arc::new(endpoint);
        let rest = open_rest(&home, &endpoint, &profile, &origin)?;
        let probe = open_rest(&home, &endpoint, &profile, &origin)?;
        let ticket = mint_ticket(
            &home,
            endpoint.as_ref(),
            &profile,
            Purpose::Interactive,
            Scope::profile(),
        )
        .await
        .map_err(|error| error.to_string())?;
        let mut connection = connect(endpoint.as_ref(), ticket)
            .await
            .map_err(|error| error.to_string())?;
        handshake(&mut connection)
            .await
            .map_err(|error| error.to_string())?;
        Ok((Client::start(connection.into_parts()).0, rest, probe))
    })?;
    Ok(Attached {
        runtime,
        client,
        rest,
        probe,
    })
}

fn field_names(value: &Value, prefix: &str, names: &mut BTreeSet<String>) {
    match value {
        Value::Object(object) => {
            for (key, child) in object {
                let label = if prefix.is_empty() {
                    key.clone()
                } else {
                    format!("{prefix}.{key}")
                };
                names.insert(label.clone());
                field_names(child, &label, names);
            }
        }
        Value::Array(items) => {
            if let Some(first) = items.first() {
                field_names(first, prefix, names);
            }
        }
        _ => {}
    }
}

fn env_fields(value: &Value) -> Vec<String> {
    let mut names = BTreeSet::new();
    if let Some(object) = value.as_object() {
        for entry in object.values() {
            field_names(entry, "<VAR>", &mut names);
        }
    }
    names.into_iter().collect()
}

fn object_fields(value: &Value) -> Vec<String> {
    let mut names = BTreeSet::new();
    field_names(value, "", &mut names);
    names.into_iter().collect()
}

fn write_fields(body: &Value) {
    let path = std::env::temp_dir().join(format!("providers-fields-{}.json", std::process::id()));
    std::fs::write(&path, serde_json::to_vec_pretty(body).expect("fields")).expect("write fields");
    println!("LIVE_PROVIDERS_FIELDS: {}", path.display());
}

fn stub_server() -> String {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
    let port = listener.local_addr().expect("addr").port();
    std::thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            let mut stream = stream;
            let mut buf = [0_u8; 2048];
            let _ = stream.read(&mut buf);
            let text = String::from_utf8_lossy(&buf);
            let body = if text.contains("GET /v1/models") {
                r#"{"data":[{"id":"stub-model"}]}"#
            } else {
                "{}"
            };
            let mut response = format!("HTTP/1.1 200 OK\r\ncontent-length: {}\r\n", body.len());
            response.push_str("content-type: application/json\r\nconnection: close\r\n\r\n");
            response.push_str(body);
            let _ = stream.write_all(response.as_bytes());
        }
    });
    format!("http://127.0.0.1:{port}/v1")
}

fn is_set(inputs: &[ProvidersInput], env_var: &str) -> Option<bool> {
    inputs.iter().find_map(|input| match input {
        ProvidersInput::EnvLoaded(Ok(rows)) => rows
            .iter()
            .find(|row| row.env_var == env_var)
            .map(|row| row.is_set),
        _ => None,
    })
}

fn endpoint_ids(inputs: &[ProvidersInput]) -> BTreeSet<String> {
    inputs
        .iter()
        .find_map(|input| match input {
            ProvidersInput::EndpointsLoaded(Ok(list)) => {
                Some(list.endpoints.iter().map(|row| row.id.clone()).collect())
            }
            _ => None,
        })
        .unwrap_or_default()
}

fn saved_endpoint_id(inputs: &[ProvidersInput], name: &str) -> Option<String> {
    inputs.iter().find_map(|input| match input {
        ProvidersInput::EndpointSaved(Ok(list)) => list
            .endpoints
            .iter()
            .find(|row| row.name == name)
            .map(|row| row.id.clone()),
        _ => None,
    })
}

#[test]
#[ignore = "talks to a running gateway"]
fn live_read_tabs_on_fixture() {
    let Attached {
        runtime,
        client,
        rest,
        probe,
    } = match attach() {
        Ok(attached) => attached,
        Err(reason) => {
            skip(&reason);
            return;
        }
    };
    runtime.block_on(async {
        let service = ProviderService::new(client, rest, "fixture".into());
        let env = service.execute(ProvidersEffect::LoadEnv, None).await;
        let options = service.execute(ProvidersEffect::LoadOptions, None).await;
        let accounts = service.execute(ProvidersEffect::LoadOauth, None).await;
        let endpoints = service.execute(ProvidersEffect::LoadEndpoints, None).await;
        assert!(
            matches!(env.as_slice(), [ProvidersInput::EnvLoaded(Ok(_))]),
            "keys did not load: {env:?}"
        );
        assert!(
            matches!(options.as_slice(), [ProvidersInput::OptionsLoaded(Ok(_))]),
            "options did not load: {options:?}"
        );
        assert!(
            matches!(accounts.as_slice(), [ProvidersInput::OauthLoaded(Ok(_))]),
            "accounts did not load: {accounts:?}"
        );
        assert!(
            matches!(
                endpoints.as_slice(),
                [ProvidersInput::EndpointsLoaded(Ok(_))]
            ),
            "endpoints did not load: {endpoints:?}"
        );
        let ready = options.iter().any(|input| {
            matches!(
                input,
                ProvidersInput::OptionsLoaded(Ok(result))
                    if result.providers.iter().any(|provider| {
                        provider.slug == "ollama-cloud" && provider.authenticated == Some(true)
                    })
            )
        });
        assert!(ready, "ollama-cloud was not ready: {options:?}");

        let mut state = ProvidersState::new();
        let now = std::time::Instant::now();
        for inputs in [&env, &options, &accounts, &endpoints] {
            for input in inputs {
                let _ = update(&mut state, input.clone(), now);
            }
        }
        let shown = talaria_core::providers::view(&state, now);
        let text = shown.rows.join("\n");
        let folded = text.to_ascii_lowercase();
        assert!(
            folded.contains("ollama") && text.contains("ready"),
            "the keys tab did not show ollama-cloud ready: {text}"
        );

        let env_body = probe.get_json("/api/env").await.expect("env list");
        let oauth_body = probe
            .get_json("/api/providers/oauth")
            .await
            .expect("account list");
        let endpoint_body = probe
            .get_json("/api/providers/custom-endpoints")
            .await
            .expect("endpoint list");
        write_fields(&json!({
            "GET /api/env": env_fields(&env_body),
            "GET /api/providers/oauth": object_fields(&oauth_body),
            "GET /api/providers/custom-endpoints": object_fields(&endpoint_body),
        }));
    });
    println!("LIVE_PROVIDERS_RESULT: PASSED");
}

#[test]
#[ignore = "talks to a running gateway and to OpenRouter with a junk key"]
fn live_rejected_key_is_not_saved() {
    let Attached {
        runtime,
        client,
        rest,
        probe: _,
    } = match attach() {
        Ok(attached) => attached,
        Err(reason) => {
            skip(&reason);
            return;
        }
    };
    runtime.block_on(async {
        let service = ProviderService::new(client, rest, "fixture".into());
        let before = service.execute(ProvidersEffect::LoadEnv, None).await;
        let before_set = is_set(&before, "OPENROUTER_API_KEY");
        assert!(
            before_set.is_some(),
            "OPENROUTER_API_KEY was not listed: {before:?}"
        );
        let checked = service
            .execute(
                ProvidersEffect::CheckKey {
                    env_var: "OPENROUTER_API_KEY".into(),
                },
                Some(Zeroizing::new(JUNK_KEY.to_owned())),
            )
            .await;
        match checked.first() {
            Some(ProvidersInput::KeyChecked(Ok(check))) if !check.reachable => {
                skip("the provider did not answer");
                return;
            }
            _ => {}
        }
        assert!(
            matches!(
                checked.first(),
                Some(ProvidersInput::KeyChecked(Ok(check))) if !check.ok && check.reachable
            ),
            "the junk key was not rejected: {checked:?}"
        );
        let after = service.execute(ProvidersEffect::LoadEnv, None).await;
        assert_eq!(
            is_set(&after, "OPENROUTER_API_KEY"),
            before_set,
            "the junk key changed the stored flag"
        );
        println!("LIVE_PROVIDERS_RESULT: PASSED");
    });
}

#[test]
#[ignore = "writes an endpoint on the fixture profile"]
fn live_endpoint_round_trip_on_fixture() {
    if std::env::var("TALARIA_PROBE_WRITES").ok().as_deref() != Some("1") {
        skip("probe writes are off");
        return;
    }
    let Attached {
        runtime,
        client,
        rest,
        probe: _,
    } = match attach() {
        Ok(attached) => attached,
        Err(reason) => {
            skip(&reason);
            return;
        }
    };
    let base_url = stub_server();
    let suffix = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| elapsed.as_millis())
        .unwrap_or(0);
    let name = format!("talaria-live-{suffix}");
    runtime.block_on(async {
        let service = ProviderService::new(client, rest, "fixture".into());
        let before = service.execute(ProvidersEffect::LoadEndpoints, None).await;
        let starting = endpoint_ids(&before);
        assert!(
            matches!(before.as_slice(), [ProvidersInput::EndpointsLoaded(Ok(_))]),
            "the endpoint list did not load: {before:?}"
        );
        let mut draft = EndpointDraft {
            id: String::new(),
            name: name.clone(),
            base_url,
            model: "stub-model".into(),
            api_key: ApiKeyChange::Typed,
            api_mode: None,
            context_length: None,
            discover_models: true,
            models: None,
            model_details: None,
        };
        let checked = service
            .execute(
                ProvidersEffect::CheckEndpoint(draft.clone()),
                Some(Zeroizing::new(ENDPOINT_KEY.to_owned())),
            )
            .await;
        assert!(
            matches!(
                checked.as_slice(),
                [ProvidersInput::EndpointChecked(Ok(check))] if check.ok
            ),
            "the stub endpoint was not accepted: {checked:?}"
        );
        let saved = service
            .execute(
                ProvidersEffect::SaveEndpoint(draft.clone()),
                Some(Zeroizing::new(ENDPOINT_KEY.to_owned())),
            )
            .await;
        let saved_id = saved_endpoint_id(&saved, &name);
        assert!(saved_id.is_some(), "the endpoint was not saved: {saved:?}");
        draft.id = saved_id.expect("id");
        draft.model = "stub-model-2".into();
        draft.api_key = ApiKeyChange::Keep;
        let edited = service
            .execute(ProvidersEffect::SaveEndpoint(draft.clone()), None)
            .await;
        assert!(
            matches!(edited.as_slice(), [ProvidersInput::EndpointSaved(Ok(_))]),
            "the endpoint was not edited: {edited:?}"
        );
        let deleted = service
            .execute(ProvidersEffect::DeleteEndpoint { id: draft.id }, None)
            .await;
        assert!(
            matches!(deleted.as_slice(), [ProvidersInput::EndpointDeleted(Ok(_))]),
            "the endpoint was not deleted: {deleted:?}"
        );
        let after = service.execute(ProvidersEffect::LoadEndpoints, None).await;
        assert_eq!(
            endpoint_ids(&after),
            starting,
            "the endpoint list did not return to its starting set"
        );
        println!("LIVE_PROVIDERS_RESULT: PASSED");
    });
}
