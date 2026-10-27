//! Messaging and pairing request shapes against the loopback dashboard.

#![allow(clippy::too_many_lines)]

#[path = "support/mock_rest.rs"]
mod mock_rest;

use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Duration;

use hermes_client::messaging_api::{
    PlatformUpdate, approve_pairing, list_pairing, list_platforms, revoke_pairing, test_platform,
    update_platform,
};
use hermes_client::rest::{RestClient, RestError, RestOptions};
use mock_rest::{CountingMinter, MockRest};
use serde_json::json;
use zeroize::Zeroizing;

fn client(origin: &str, minter: Arc<CountingMinter>, get_deadline: Duration) -> RestClient {
    RestClient::new(
        origin,
        minter,
        RestOptions {
            get_deadline,
            write_deadline: Duration::from_secs(30),
            max_body: 8 * 1024 * 1024,
        },
    )
    .expect("loopback client")
}

fn save(url: &str) -> PlatformUpdate {
    let mut env = BTreeMap::new();
    env.insert("SIGNAL_HTTP_URL".into(), Zeroizing::new(url.to_owned()));
    PlatformUpdate {
        enabled: None,
        env,
        clear_env: Vec::new(),
    }
}

#[tokio::test]
async fn request_shapes() {
    let server = MockRest::start()
        .await
        .allow_own_profile_selector("fixture");
    let minter = Arc::new(CountingMinter::default());
    let rest = client(&server.origin(), minter.clone(), Duration::from_secs(15));

    let listed = list_platforms(&rest, "fixture").await.expect("platforms");
    assert!(!listed.platforms.is_empty());

    let spaced = list_platforms(&rest, "fixture ").await;
    assert!(spaced.is_err(), "a trailing space is another profile");

    let other = list_platforms(&rest, "default").await;
    assert!(
        matches!(other, Err(RestError::Forbidden { .. })),
        "another profile is refused"
    );

    let saved = update_platform(
        &rest,
        "fixture",
        "signal",
        &save("https://example.invalid/bridge"),
    )
    .await
    .expect("save");
    assert!(!saved.hot_served);
    let switched = update_platform(
        &rest,
        "fixture",
        "signal",
        &PlatformUpdate {
            enabled: Some(true),
            env: BTreeMap::new(),
            clear_env: Vec::new(),
        },
    )
    .await
    .expect("switch");
    assert!(switched.ok);

    let _tested = test_platform(&rest, "fixture", "signal")
        .await
        .expect("test");
    let _pairing = list_pairing(&rest, "fixture").await.expect("pairing");
    let _approved = approve_pairing(&rest, "fixture", "telegram", "req-test-1")
        .await
        .expect("approve");
    revoke_pairing(&rest, "fixture", "telegram", "user-test-1")
        .await
        .expect("revoke");

    let requests = server.requests().await;
    assert_eq!(
        minter.count(),
        requests.len(),
        "one fresh ticket per request"
    );
    let paths: Vec<&str> = requests
        .iter()
        .map(|request| request.path.as_str())
        .collect();
    assert!(paths.iter().any(|path| path.contains("profile=fixture%20")));
    assert!(paths.contains(&"/api/messaging/platforms?profile=fixture"));
    assert!(paths.contains(&"/api/messaging/platforms/signal?profile=fixture"));
    assert!(paths.contains(&"/api/messaging/platforms/signal/test?profile=fixture"));
    assert!(paths.contains(&"/api/pairing?profile=fixture"));
    assert!(paths.contains(&"/api/pairing/approve"));
    assert!(paths.contains(&"/api/pairing/revoke"));
    for request in &requests {
        assert!(
            request.headers.iter().all(|(name, _)| name != "origin"),
            "no origin header"
        );
        assert!(
            request
                .headers
                .iter()
                .any(|(name, _)| name == "x-hermes-gateway-ticket")
        );
    }
    let save_body = requests
        .iter()
        .find(|request| request.method == "PUT")
        .expect("put");
    let save_json: serde_json::Value = serde_json::from_slice(&save_body.body).expect("put json");
    assert_eq!(
        save_json,
        json!({"env": {"SIGNAL_HTTP_URL": "https://example.invalid/bridge"}, "clear_env": []})
    );
    let switch_body = requests
        .iter()
        .filter(|request| request.method == "PUT")
        .nth(1)
        .expect("second put");
    let switch_json: serde_json::Value =
        serde_json::from_slice(&switch_body.body).expect("switch json");
    assert_eq!(switch_json, json!({"enabled": true}));
    let approve_body = requests
        .iter()
        .find(|request| request.path == "/api/pairing/approve")
        .expect("approve");
    let approve_json: serde_json::Value =
        serde_json::from_slice(&approve_body.body).expect("approve json");
    assert_eq!(
        approve_json,
        json!({"platform": "telegram", "request_id": "req-test-1", "profile": "fixture"})
    );
}

#[tokio::test]
async fn replies_and_failures() {
    let server = MockRest::start()
        .await
        .allow_own_profile_selector("fixture");
    let minter = Arc::new(CountingMinter::default());
    let rest = client(&server.origin(), minter, Duration::from_secs(15));

    let listed = list_platforms(&rest, "fixture").await.expect("platforms");
    let signal = listed
        .platforms
        .iter()
        .find(|row| row.id == "signal")
        .expect("signal");
    assert_eq!(signal.env_vars[0].key, "SIGNAL_HTTP_URL");
    assert!(signal.env_vars[0].value.is_none());
    assert!(listed.gateway_start_command.is_some());

    let extra = r#"{"ok":true,"user":{"id":"user-test-1","name":"test-user"},"extra":1}"#;
    let approved: hermes_client::messaging_api::ApproveReply =
        serde_json::from_str(extra).expect("extra fields");
    assert!(approved.ok);
    let nulled: hermes_client::messaging_api::TestReply =
        serde_json::from_str(r#"{"ok":false,"state":null,"message":null,"auth_posture":null}"#)
            .expect("nulls");
    assert!(!nulled.ok);
    assert!(nulled.auth_posture.is_none());

    let platforms = "/api/messaging/platforms?profile=fixture";
    server
        .fail_status("GET", platforms, 400, Some("bad platform"))
        .await;
    let bad = list_platforms(&rest, "fixture").await;
    assert!(matches!(bad, Err(RestError::Refused { status: 400, .. })));

    server
        .fail_status("GET", platforms, 404, Some("missing"))
        .await;
    let missing = list_platforms(&rest, "fixture").await;
    assert!(matches!(missing, Err(RestError::NotFound { .. })));

    server
        .fail_status(
            "PUT",
            "/api/messaging/platforms/signal?profile=fixture",
            409,
            Some("conflict"),
        )
        .await;
    let conflict = update_platform(
        &rest,
        "fixture",
        "signal",
        &save("https://example.invalid/x"),
    )
    .await;
    assert!(matches!(
        conflict,
        Err(RestError::Refused { status: 409, .. })
    ));

    server
        .fail_status("POST", "/api/pairing/approve", 429, Some("slow down"))
        .await;
    let limited = approve_pairing(&rest, "fixture", "telegram", "req-test-1").await;
    assert!(matches!(
        limited,
        Err(RestError::Refused { status: 429, .. })
    ));

    server.fail_status("GET", platforms, 500, None).await;
    let server_error = list_platforms(&rest, "fixture").await;
    assert!(matches!(
        server_error,
        Err(RestError::Server { status: 500 })
    ));

    let slow = MockRest::start()
        .await
        .allow_own_profile_selector("fixture");
    let slow_minter = Arc::new(CountingMinter::default());
    let slow_rest = client(&slow.origin(), slow_minter, Duration::from_millis(50));
    slow.delay(
        "GET",
        "/api/messaging/platforms?profile=fixture",
        Duration::from_secs(2),
    )
    .await;
    let timed = list_platforms(&slow_rest, "fixture").await;
    assert!(matches!(timed, Err(RestError::Timeout { sent: true })));
}
