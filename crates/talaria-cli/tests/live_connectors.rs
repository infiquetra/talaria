//! Live connectors and messaging checks against an already running pinned gateway.
//!
//! Run these ignored tests serially. A skip fails when `TALARIA_LIVE_REQUIRE=1`.
//! Connector calls and messaging writes each require their separate operator opt-in.

use std::collections::{BTreeMap, BTreeSet};
use std::io::Write;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use hermes_client::{
    Client, DiscoverOptions, Endpoint, EnsurePolicy, Method, NativeHttpMinter, PlatformUpdate,
    PlatformWire, ProfileId, Purpose, RequestOpts, RestClient, RestOptions, Scope, default_home,
    discover_with, list_pairing, list_platforms, test_platform, update_platform,
};
use serde_json::Value;
use talaria_app::connectors::{ConnectorsConsent, ConnectorsService};
use talaria_core::connectors::{ConnectorsEffect, ConnectorsInput};
use zeroize::Zeroizing;

const PROFILE: &str = "fixture";
const SIGNAL_URL: &str = "SIGNAL_HTTP_URL";
const PROBE: &str = "http://127.0.0.1:9/talaria-probe";

fn opted_in(variable: &str) -> bool {
    std::env::var(variable).ok().as_deref() == Some("1")
}

fn report(kind: &str, reason: &str) {
    if reason.is_empty() {
        println!("LIVE_CONNECTORS_RESULT: {kind}");
    } else {
        println!("LIVE_CONNECTORS_RESULT: {kind} ({reason})");
    }
    assert!(
        kind != "SKIPPED" || !opted_in("TALARIA_LIVE_REQUIRE"),
        "live check required: {reason}"
    );
}

fn require(condition: bool, reason: &str) {
    if !condition {
        report("FAILED", reason);
        panic!("live connectors check failed: {reason}");
    }
}

struct Live {
    home: PathBuf,
    endpoint: Arc<Endpoint>,
    profile: ProfileId,
    rest: RestClient,
}

// Discovery errors can contain private paths; expose only fixed public descriptions.
async fn fixture() -> Result<Live, &'static str> {
    let home = default_home();
    let endpoint = discover_with(
        &home,
        &DiscoverOptions {
            control_timeout: Duration::from_secs(5),
            ensure: EnsurePolicy::Never,
        },
    )
    .await
    .map_err(|_| "running gateway unavailable")?;
    if hermes_contract::pinned::live_skip_reason(endpoint.code_sha()).is_some() {
        return Err("gateway revision does not match the pinned Hermes release");
    }
    let profile = endpoint
        .resolve_profile(PROFILE)
        .map_err(|_| "fixture unavailable")?;
    let endpoint = Arc::new(endpoint);
    let rest = RestClient::new(
        endpoint.api_origin(),
        Arc::new(NativeHttpMinter::new(
            home.clone(),
            endpoint.clone(),
            profile.clone(),
        )),
        RestOptions::default(),
    )
    .map_err(|_| "loopback web interface unavailable")?;
    Ok(Live {
        home,
        endpoint,
        profile,
        rest,
    })
}

async fn begin() -> Option<Live> {
    match fixture().await {
        Ok(live) => Some(live),
        Err(reason) => {
            report("SKIPPED", reason);
            None
        }
    }
}

fn field_names(value: &Value, names: &mut BTreeSet<String>) {
    match value {
        Value::Object(object) => {
            for (key, value) in object {
                names.insert(key.clone());
                field_names(value, names);
            }
        }
        Value::Array(array) => {
            for value in array {
                field_names(value, names);
            }
        }
        _ => {}
    }
}

fn signal_url_is_set(platform: &PlatformWire) -> Option<bool> {
    platform
        .env_vars
        .iter()
        .find(|field| field.key == SIGNAL_URL)
        .map(|field| field.is_set)
}

fn signal_state(platforms: &[PlatformWire], is_set: bool) -> bool {
    platforms.iter().any(|platform| {
        platform.id == "signal"
            && platform.enabled == Some(false)
            && signal_url_is_set(platform) == Some(is_set)
    })
}

#[tokio::test]
#[ignore]
async fn live_messaging_reads_on_fixture() {
    let Some(live) = begin().await else { return };
    let platforms = list_platforms(&live.rest, PROFILE)
        .await
        .unwrap_or_else(|_| panic!("messaging platform list failed"));
    let _pairing = list_pairing(&live.rest, PROFILE)
        .await
        .unwrap_or_else(|_| panic!("messaging pairing list failed"));
    let off = platforms
        .platforms
        .iter()
        .find(|platform| platform.id == "signal" && platform.enabled != Some(true))
        .or_else(|| {
            platforms
                .platforms
                .iter()
                .find(|platform| platform.enabled != Some(true))
        });
    let Some(off) = off else {
        report("SKIPPED", "no off platform");
        return;
    };
    let tested = test_platform(&live.rest, PROFILE, &off.id)
        .await
        .unwrap_or_else(|_| panic!("off platform test failed"));
    require(!tested.ok, "off platform unexpectedly passed its test");

    let mut names = BTreeSet::new();
    for path in [
        "/api/messaging/platforms?profile=fixture",
        "/api/pairing?profile=fixture",
    ] {
        let reply = live
            .rest
            .request(Method::Get, path, None, RequestOpts::default())
            .await
            .unwrap_or_else(|_| panic!("messaging field names read failed"));
        field_names(&reply, &mut names);
    }
    require(!names.is_empty(), "messaging field names missing");
    let mut file = tempfile::NamedTempFile::new()
        .unwrap_or_else(|_| panic!("field names temporary file unavailable"));
    require(
        file.write_all(names.into_iter().collect::<Vec<_>>().join("\n").as_bytes())
            .is_ok(),
        "field names write failed",
    );
    report("PASSED", "");
}

#[tokio::test]
#[ignore]
async fn live_connectors_gate_on_fixture() {
    if !opted_in("TALARIA_PROBE_CONNECTORS") {
        report("SKIPPED", "probe flag");
        return;
    }
    let Some(live) = begin().await else { return };
    let ticket = hermes_client::mint_ticket(
        &live.home,
        &live.endpoint,
        &live.profile,
        Purpose::Interactive,
        Scope::profile(),
    )
    .await
    .unwrap_or_else(|_| panic!("fixture ticket unavailable"));
    let mut connection = hermes_client::connect(&live.endpoint, ticket)
        .await
        .unwrap_or_else(|_| panic!("fixture connection unavailable"));
    hermes_client::handshake(&mut connection)
        .await
        .unwrap_or_else(|_| panic!("fixture handshake unavailable"));
    let (client, _inbound) = Client::start(connection.into_parts());
    let consent = ConnectorsConsent::default();
    consent.grant(PROFILE);
    let service = ConnectorsService::new(client.clone(), live.rest, PROFILE.into(), consent);
    require(
        matches!(
            service
                .execute(ConnectorsEffect::ListConnectors, None)
                .await
                .as_slice(),
            [ConnectorsInput::ListReply(Ok(_))]
        ),
        "connector list did not succeed",
    );
    require(
        matches!(
            service
                .execute(ConnectorsEffect::LoadCatalog, None)
                .await
                .as_slice(),
            [ConnectorsInput::CatalogReply(Ok(_))]
        ),
        "connector catalog did not succeed",
    );
    require(
        matches!(
            service
                .execute(ConnectorsEffect::LoadAccounts, None)
                .await
                .as_slice(),
            [ConnectorsInput::AccountsReply(Ok(_))]
        ),
        "connector accounts did not succeed",
    );
    require(
        matches!(
            service
                .execute(ConnectorsEffect::LoadPolicy, None)
                .await
                .as_slice(),
            [ConnectorsInput::PolicyReply(Ok(_))]
        ),
        "connector policy did not succeed",
    );
    client.close().await;
    report("PASSED", "");
}

#[tokio::test]
#[ignore]
async fn live_messaging_write_round_trip() {
    if !opted_in("TALARIA_PROBE_WRITES") {
        report("SKIPPED", "write flag");
        return;
    }
    let Some(live) = begin().await else { return };
    let before = list_platforms(&live.rest, PROFILE)
        .await
        .unwrap_or_else(|_| panic!("messaging write preconditions read failed"));
    if !signal_state(&before.platforms, false) {
        report("SKIPPED", "preconditions");
        return;
    }

    let body = PlatformUpdate {
        enabled: None,
        env: BTreeMap::from([(SIGNAL_URL.into(), Zeroizing::new(PROBE.into()))]),
        clear_env: Vec::new(),
    };
    // Keep every outcome as data until the clear and its readback have completed.
    let attempted = update_platform(&live.rest, PROFILE, "signal", &body).await;
    let after_attempt = list_platforms(&live.rest, PROFILE).await;
    drop(body);
    let cleanup = PlatformUpdate {
        enabled: None,
        env: BTreeMap::new(),
        clear_env: vec![SIGNAL_URL.into()],
    };
    let cleared = update_platform(&live.rest, PROFILE, "signal", &cleanup).await;
    let after_cleanup = list_platforms(&live.rest, PROFILE).await;

    let cleanup_ok = cleared.is_ok_and(|reply| reply.ok)
        && after_cleanup.is_ok_and(|reply| signal_state(&reply.platforms, false));
    require(cleanup_ok, "residue or cleanup could not be verified");
    let attempt_ok = attempted.is_ok_and(|reply| reply.ok)
        && after_attempt.is_ok_and(|reply| signal_state(&reply.platforms, true));
    require(attempt_ok, "messaging write or readback did not succeed");
    report("PASSED", "");
}
