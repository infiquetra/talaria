#[path = "support/live.rs"]
mod live;
use hermes_client::{
    gateway_api::gateway_status,
    rest::{NativeHttpMinter, RestClient, RestOptions},
};
use std::sync::Arc;
use talaria_app::doctor::DoctorService;
use talaria_core::doctor::{CheckId, Severity, StatusFacts, diagnose};

#[tokio::test]
#[ignore]
async fn reads_the_real_gateway() {
    let env = match live::live_preconditions().await {
        Ok(env) => env,
        Err(reason) => {
            live::report_skip("LIVE_DOCTOR_RESULT", &reason);
            return;
        }
    };
    let facts = DoctorService::new(env.home.clone()).gather("fixture").await;
    let report = diagnose(&facts);
    for id in [CheckId::Gateway, CheckId::Capabilities, CheckId::Attach] {
        let row = report
            .checks
            .iter()
            .find(|row| row.id == id)
            .expect("row present");
        assert_eq!(row.severity, Severity::Ok, "{id:?}");
    }
    let profiles = report
        .checks
        .iter()
        .find(|row| row.id == CheckId::Profiles)
        .expect("profile row");
    assert!(matches!(profiles.severity, Severity::Ok | Severity::Warn));
    assert!(matches!(facts.status, StatusFacts::Read(_)));
    let origin = env.endpoint.api_origin().to_owned();
    let minter = NativeHttpMinter::new(env.home, Arc::new(env.endpoint), env.fixture);
    let rest = RestClient::new(&origin, Arc::new(minter), RestOptions::default()).expect("client");
    let response = gateway_status(&rest).await.expect("status response");
    for pointer in [
        "/config_version",
        "/latest_config_version",
        "/gateway_heartbeat_stale_s",
        "/active_agents",
        "/components/storage/status",
        "/components/platforms/status",
        "/components/platforms/configured",
        "/components/platforms/connected",
        "/components/dashboard/status",
        "/components/dashboard/recent_unhandled_errors",
        "/components/dashboard/selftest",
        "/multiplex_standalone_reason",
    ] {
        assert!(
            response.pointer(pointer).is_some(),
            "status field {pointer}"
        );
    }
    if let Some(reason) = response.pointer("/components/storage/reason") {
        assert!(reason.is_null() || reason.is_string());
    }
    let words: Vec<_> = report
        .checks
        .iter()
        .map(|row| match row.severity {
            Severity::Ok => "ok",
            Severity::Warn => "warn",
            Severity::Fail => "FAIL",
            Severity::Skip => "skip",
            Severity::Info => "info",
        })
        .collect();
    println!("{}", words.join(" "));
    println!("LIVE_DOCTOR_RESULT: PASSED");
}
