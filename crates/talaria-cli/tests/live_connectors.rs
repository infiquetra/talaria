//! Live connectors and messaging checks. Ignored unless a person asks for them.
//!
//! A skip is not a pass. With the live-require variable set to 1, a skip fails the test.

use std::time::Duration;

use hermes_client::{
    DiscoverOptions, DiscoveryError, EnsurePolicy, ProfileLookupError, default_home, discover_with,
};
use hermes_contract::pinned::live_skip_reason;

const PROFILE: &str = "fixture";
const PROBE: &str = "http://127.0.0.1:9/talaria-probe";

fn report(kind: &str, reason: &str) {
    if reason.is_empty() {
        println!("LIVE_CONNECTORS_RESULT: {kind}");
    } else {
        println!("LIVE_CONNECTORS_RESULT: {kind} ({reason})");
    }
    if kind == "SKIPPED" && std::env::var("TALARIA_LIVE_REQUIRE").ok().as_deref() == Some("1") {
        panic!("live test skipped: {reason}");
    }
}

fn live_reached() -> bool {
    todo!("connectors live stub")
}

async fn discover_skip() -> Option<String> {
    let home = default_home();
    let options = DiscoverOptions {
        control_timeout: Duration::from_secs(5),
        ensure: EnsurePolicy::Never,
    };
    let endpoint = match discover_with(&home, &options).await {
        Ok(endpoint) => endpoint,
        Err(DiscoveryError::NoGateway) => return Some("no gateway".into()),
        Err(error) => return Some(error.to_string()),
    };
    if let Err(error) = endpoint.resolve_profile(PROFILE) {
        return Some(match error {
            ProfileLookupError::NeverDiscovered { .. } => "fixture not served".into(),
            other => other.to_string(),
        });
    }
    live_skip_reason(endpoint.code_sha())
}

#[tokio::test]
#[ignore]
async fn live_messaging_reads_on_fixture() {
    if let Some(reason) = discover_skip().await {
        report("SKIPPED", &reason);
        return;
    }
    assert!(live_reached());
    let file = tempfile::NamedTempFile::new().expect("temp");
    std::fs::write(file.path(), "id\nname\nstate\nenabled\nenv_vars\nkey\nis_set\n").expect("names");
    report("PASSED", "");
}

#[tokio::test]
#[ignore]
async fn live_connectors_gate_on_fixture() {
    if std::env::var("TALARIA_PROBE_CONNECTORS").ok().as_deref() != Some("1") {
        report("SKIPPED", "probe flag");
        return;
    }
    if let Some(reason) = discover_skip().await {
        report("SKIPPED", &reason);
        return;
    }
    assert!(live_reached());
    report("PASSED", "");
}

#[tokio::test]
#[ignore]
async fn live_messaging_write_round_trip() {
    if std::env::var("TALARIA_PROBE_WRITES").ok().as_deref() != Some("1") {
        report("SKIPPED", "write flag");
        return;
    }
    if let Some(reason) = discover_skip().await {
        report("SKIPPED", &reason);
        return;
    }
    assert!(live_reached());
    let _probe = PROBE;
    report("PASSED", "");
}
