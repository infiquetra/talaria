//! Provider service calls. They fail while execute returns no inputs.

#[path = "support/mock_gateway.rs"]
mod mock_gateway;

#[path = "../../hermes-client/tests/support/mock_rest.rs"]
mod mock_rest;

use std::collections::BTreeSet;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;

use hermes_client::rest::{RestClient, RestOptions, TicketMinter};
use hermes_client::{Recorder, RecorderOptions, Ticket, TicketError, scan_literals};
use mock_gateway::MockGateway;
use mock_rest::MockRest;
use serde_json::{Value, json};
use talaria_app::providers::{self, ProviderService};
use talaria_core::providers::{
    ApiKeyChange, EndpointDraft, Failure, ProvidersEffect, ProvidersInput,
};
use zeroize::Zeroizing;

const PROVIDER_KEY: &str = "sk-test-SENTINEL-0123456789abcdef";
const ENDPOINT_KEY: &str = "sk-test-ENDPOINT-0123456789abcdef";
const NOW: i64 = 1_792_656_000;

struct Idle;

impl TicketMinter for Idle {
    fn mint(&self) -> Pin<Box<dyn Future<Output = Result<Ticket, TicketError>> + Send + '_>> {
        Box::pin(async { Err(TicketError::NotAuthenticated) })
    }
}

fn rest_client() -> RestClient {
    let minter: Arc<dyn TicketMinter> = Arc::new(Idle);
    RestClient::new("http://127.0.0.1:9", minter, RestOptions::default()).expect("loopback")
}

fn draft(change: ApiKeyChange) -> EndpointDraft {
    EndpointDraft {
        id: String::new(),
        name: "example-endpoint".into(),
        base_url: "https://example.invalid/v1".into(),
        model: "example-model".into(),
        api_key: change,
        api_mode: Some(String::new()),
        context_length: None,
        discover_models: true,
        models: None,
        model_details: None,
    }
}

fn keys_of(value: &Value) -> BTreeSet<String> {
    value
        .as_object()
        .map(|object| object.keys().cloned().collect())
        .unwrap_or_default()
}

fn flag_ok(value: &Value, name: &str) -> bool {
    match value.get(name) {
        None => true,
        Some(flag) => flag == &json!(false),
    }
}

#[tokio::test]
async fn socket_calls_are_typed_and_closed() {
    let (gateway, client, _inbound) = MockGateway::start().await;
    let service = ProviderService::new(client, rest_client(), "fixture".into());

    let loaded = service.execute(ProvidersEffect::LoadOptions, None).await;
    assert!(
        matches!(loaded.as_slice(), [ProvidersInput::OptionsLoaded(Ok(_))]),
        "model.options returned no catalogue: {loaded:?}"
    );
    let seen = gateway.requests("model.options");
    assert_eq!(seen.len(), 1, "model.options was not sent once");
    let allowed = [
        "explicit_only",
        "include_unconfigured",
        "profile",
        "refresh",
        "session_id",
    ];
    let keys = keys_of(&seen[0].params);
    assert!(
        keys.iter().all(|key| allowed.contains(&key.as_str())),
        "model.options sent an unexpected key: {keys:?}"
    );
    assert_eq!(
        seen[0].params.get("include_unconfigured"),
        Some(&json!(true))
    );
    assert!(flag_ok(&seen[0].params, "explicit_only"));
    assert!(flag_ok(&seen[0].params, "refresh"));
    assert!(seen[0].params.get("profile").is_none());
    assert!(seen[0].params.get("session_id").is_none());

    let secret = Zeroizing::new(PROVIDER_KEY.to_owned());
    let saved = service
        .execute(
            ProvidersEffect::SaveKey {
                slug: "ollama-cloud".into(),
            },
            Some(secret),
        )
        .await;
    assert!(
        matches!(saved.as_slice(), [ProvidersInput::KeySaved(Ok(_))]),
        "model.save_key returned no row: {saved:?}"
    );
    let saves = gateway.requests("model.save_key");
    assert_eq!(saves.len(), 1, "model.save_key was not sent once");
    assert_eq!(
        saves[0].params,
        json!({"slug": "ollama-cloud", "api_key": PROVIDER_KEY})
    );

    let removed = service
        .execute(
            ProvidersEffect::Disconnect {
                slug: "ollama-cloud".into(),
            },
            None,
        )
        .await;
    assert!(
        matches!(removed.as_slice(), [ProvidersInput::Disconnected(Ok(()))]),
        "model.disconnect returned no result: {removed:?}"
    );
    let disconnects = gateway.requests("model.disconnect");
    assert_eq!(disconnects.len(), 1, "model.disconnect was not sent once");
    assert_eq!(disconnects[0].params, json!({"slug": "ollama-cloud"}));
}

#[tokio::test]
async fn secret_reaches_only_two_requests() {
    let (gateway, client, _inbound) = MockGateway::start().await;
    let mock = MockRest::start().await;
    let rest = {
        let minter: Arc<dyn TicketMinter> = Arc::new(Idle);
        RestClient::new(&mock.origin(), minter, RestOptions::default()).expect("loopback")
    };
    let service = ProviderService::new(client, rest, "fixture".into());
    let provider = Zeroizing::new(PROVIDER_KEY.to_owned());
    let endpoint = Zeroizing::new(ENDPOINT_KEY.to_owned());

    let checked = service
        .execute(
            ProvidersEffect::CheckKey {
                env_var: "OPENROUTER_API_KEY".into(),
            },
            Some(provider.clone()),
        )
        .await;
    let saved = service
        .execute(
            ProvidersEffect::SaveKey {
                slug: "openrouter".into(),
            },
            Some(provider),
        )
        .await;
    let tested = service
        .execute(
            ProvidersEffect::CheckEndpoint(draft(ApiKeyChange::Typed)),
            Some(endpoint.clone()),
        )
        .await;
    let stored = service
        .execute(
            ProvidersEffect::SaveEndpoint(draft(ApiKeyChange::Typed)),
            Some(endpoint),
        )
        .await;

    let save_frames = gateway
        .requests("model.save_key")
        .iter()
        .filter(|frame| frame.params.to_string().contains(PROVIDER_KEY))
        .count();
    assert_eq!(
        save_frames, 1,
        "the provider key was not sent on exactly one save"
    );
    let validate_bodies = mock
        .requests()
        .iter()
        .filter(|request| {
            request.path == "/api/providers/validate"
                && request
                    .body
                    .as_deref()
                    .is_some_and(|body| body.contains(PROVIDER_KEY))
        })
        .count();
    assert_eq!(
        validate_bodies, 1,
        "the provider key was not sent on exactly one check"
    );
    let endpoint_bodies = mock
        .requests()
        .iter()
        .filter(|request| {
            request
                .body
                .as_deref()
                .is_some_and(|body| body.contains(ENDPOINT_KEY))
        })
        .count();
    assert_eq!(
        endpoint_bodies, 2,
        "the endpoint key was not sent on the test and the save"
    );

    let mut dumped = String::new();
    for inputs in [&checked, &saved, &tested, &stored] {
        dumped.push_str(&format!("{inputs:?}\n"));
    }
    let wire = hermes_client::providers_api::EndpointBodyWire {
        id: None,
        name: Some("example-endpoint".into()),
        base_url: Some("https://example.invalid/v1".into()),
        model: Some("example-model".into()),
        api_key: Some(ENDPOINT_KEY),
        api_mode: None,
        context_length: None,
        discover_models: Some(true),
        make_default: Some(false),
        models: None,
        model_details: None,
    };
    dumped.push_str(&format!("{wire:?}\n"));
    let hits = scan_literals(&dumped, &[PROVIDER_KEY.to_owned(), ENDPOINT_KEY.to_owned()]);
    assert!(
        hits.is_empty(),
        "a key was visible in debug output: {hits:?}"
    );

    let file = tempfile::NamedTempFile::new().expect("temp file");
    let recorder = Recorder::create(
        file.path(),
        RecorderOptions {
            endpoint: "http://127.0.0.1:9".into(),
            ..RecorderOptions::default()
        },
    )
    .expect("recorder");
    for frame in gateway.seen() {
        recorder.record_outbound(frame.connection, &frame.params.to_string());
    }
    recorder.finish().expect("finish");
    let recorded = std::fs::read_to_string(file.path()).expect("read recording");
    let recorded_hits = scan_literals(
        &recorded,
        &[PROVIDER_KEY.to_owned(), ENDPOINT_KEY.to_owned()],
    );
    assert!(
        recorded_hits.is_empty(),
        "a recording held a key: {recorded_hits:?}"
    );
}

#[tokio::test]
async fn unknown_outcomes_reload_and_never_resend() {
    let (gateway, client, _inbound) = MockGateway::start().await;
    gateway.set_silent_all(true);
    let service = ProviderService::new(client, rest_client(), "fixture".into());
    let secret = Zeroizing::new(PROVIDER_KEY.to_owned());
    let saved = service
        .execute(
            ProvidersEffect::SaveKey {
                slug: "ollama-cloud".into(),
            },
            Some(secret),
        )
        .await;
    assert!(
        matches!(
            saved.as_slice(),
            [ProvidersInput::KeySaved(Err(Failure::Unknown))]
        ),
        "a dropped save was not an unknown outcome: {saved:?}"
    );
    assert_eq!(
        gateway.requests("model.save_key").len(),
        1,
        "the dropped save was sent again"
    );

    let mock = MockRest::start().await;
    mock.delay_after_read(
        "POST",
        "/api/providers/custom-endpoints",
        Duration::from_secs(31),
    );
    let rest = {
        let minter: Arc<dyn TicketMinter> = Arc::new(Idle);
        RestClient::new(&mock.origin(), minter, RestOptions::default()).expect("loopback")
    };
    let (gateway, client, _inbound) = MockGateway::start().await;
    let service = ProviderService::new(client, rest, "fixture".into());
    let stored = service
        .execute(
            ProvidersEffect::SaveEndpoint(draft(ApiKeyChange::Keep)),
            None,
        )
        .await;
    assert!(
        matches!(
            stored.as_slice(),
            [ProvidersInput::EndpointSaved(Err(Failure::Unknown))]
        ),
        "a late endpoint save was not an unknown outcome: {stored:?}"
    );
    let writes = mock
        .requests()
        .iter()
        .filter(|request| request.path == "/api/providers/custom-endpoints")
        .count();
    assert_eq!(writes, 1, "the late save was sent again");
    let _ = gateway;
}

#[tokio::test]
async fn closing_cancels_open_sign_ins() {
    let (gateway, client, _inbound) = MockGateway::start().await;
    let mock = MockRest::start().await;
    mock.script(
        "POST",
        "/api/providers/oauth/nous/start",
        200,
        json!({
            "session_id": "sess-1",
            "flow": "device_code",
            "user_code": "ABCD-1234",
            "verification_url": "https://example.invalid/device",
            "expires_in": 90,
            "poll_interval": 5
        }),
    );
    let rest = {
        let minter: Arc<dyn TicketMinter> = Arc::new(Idle);
        RestClient::new(&mock.origin(), minter, RestOptions::default()).expect("loopback")
    };
    let service = ProviderService::new(client, rest, "fixture".into());
    let started = service
        .execute(
            ProvidersEffect::StartSignIn {
                id: "nous".into(),
                attempt: 1,
            },
            None,
        )
        .await;
    assert!(
        matches!(
            started.as_slice(),
            [ProvidersInput::SignInStarted {
                attempt: 1,
                result: Ok(_)
            }]
        ),
        "sign-in start was not returned: {started:?}"
    );

    service.close().await;
    let late = service
        .execute(
            ProvidersEffect::StartSignIn {
                id: "nous".into(),
                attempt: 2,
            },
            None,
        )
        .await;
    let cancels = mock
        .requests()
        .iter()
        .filter(|request| request.method == "DELETE" && request.path.contains("/sessions/"))
        .count();
    assert!(
        cancels >= 1,
        "a start that finished after close was not cancelled: {late:?}"
    );

    let polled = service
        .execute(
            ProvidersEffect::Poll {
                id: "nous".into(),
                session_id: "sess-1".into(),
                after: Duration::ZERO,
            },
            None,
        )
        .await;
    assert!(
        polled.is_empty(),
        "a poll for a cancelled session returned an input: {polled:?}"
    );
    assert!(
        mock.requests()
            .iter()
            .all(|request| !request.path.contains("/poll/")),
        "a poll for a cancelled session was sent"
    );

    let https = "https://example.invalid/device";
    let http = "http://example.invalid/device";
    #[cfg(target_os = "macos")]
    assert_eq!(
        providers::opener_argv(https),
        Some(vec!["open".into(), https.into()])
    );
    #[cfg(target_os = "linux")]
    assert_eq!(
        providers::opener_argv(https),
        Some(vec!["xdg-open".into(), https.into()])
    );
    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    assert_eq!(providers::opener_argv(https), None);
    assert_eq!(providers::opener_argv(http), None);
    let _ = gateway;
}

#[test]
fn expiry_formats_convert() {
    assert_eq!(
        providers::expires_in_s(&json!("2026-10-22T10:00:00Z"), NOW),
        Some(7200),
        "an instant two hours ahead was not converted"
    );
    assert_eq!(
        providers::expires_in_s(&json!("2026-10-22T10:00:00.5+02:00"), NOW),
        Some(0),
        "a fractional offset instant was not truncated toward zero"
    );
    assert_eq!(
        providers::expires_in_s(&json!(1_792_700_000_i64), NOW),
        Some(44_000)
    );
    assert_eq!(
        providers::expires_in_s(&json!(1_792_700_000_000_i64), NOW),
        Some(44_000)
    );
    assert_eq!(providers::expires_in_s(&json!("soon"), NOW), None);
    assert_eq!(providers::expires_in_s(&json!(true), NOW), None);
    assert_eq!(providers::expires_in_s(&Value::Null, NOW), None);
}
