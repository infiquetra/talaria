#[path = "support/mock_rest.rs"]
mod mock_rest;

use hermes_client::{
    cron_api::{self, TRIGGER_DEADLINE},
    rest::{RestClient, RestOptions},
};
use mock_rest::{CountingMinter, MockRest, RecordedRequest};
use serde_json::json;
use std::{sync::Arc, time::Duration};

fn client(origin: &str) -> RestClient {
    RestClient::new(
        origin,
        Arc::new(CountingMinter::default()),
        RestOptions::default(),
    )
    .expect("loopback origin")
}

fn header<'a>(request: &'a RecordedRequest, name: &str) -> Option<&'a str> {
    request
        .headers
        .iter()
        .find(|(key, _)| key.eq_ignore_ascii_case(name))
        .map(|(_, value)| value.as_str())
}

#[tokio::test]
#[allow(clippy::bool_assert_comparison)]
async fn trigger_request_shape() {
    let opts = cron_api::trigger_opts();
    assert_eq!(opts.ticket, true);
    assert_eq!(opts.deadline, Some(TRIGGER_DEADLINE));
    assert_eq!(TRIGGER_DEADLINE, Duration::from_secs(24 * 60 * 60));
    assert_eq!(cron_api::encode_component("a b&c"), "a%20b%26c");
    assert_eq!(cron_api::encode_component("abc/1 x"), "abc%2F1%20x");

    let path = "/api/cron/jobs/abc%2F1%20x/trigger?profile=fixture";
    let mock = MockRest::start()
        .await
        .with_route("POST", path, |_request| {
            (200, json!({"id": "abc/1 x", "secret": "hidden"}))
        });
    let rest = client(&mock.origin());
    let () = cron_api::trigger_job(&rest, "fixture", "abc/1 x", opts)
        .await
        .expect("a 200 body is dropped");
    let requests = mock.requests().await;
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0].method, "POST");
    assert_eq!(requests[0].path, path);
    assert!(requests[0].body.is_empty());
    assert!(header(&requests[0], "x-hermes-gateway-ticket").is_some());
    assert!(header(&requests[0], "origin").is_none());
}

#[tokio::test]
async fn runs_and_targets_request_shapes() {
    let runs_path = "/api/cron/jobs/a1b2c3d4e5f6/runs?profile=fixture&limit=20";
    let targets_path = "/api/cron/delivery-targets";
    let mock = MockRest::start()
        .await
        .with_route("GET", runs_path, |_request| {
            (200, json!({"runs": [], "limit": 20}))
        })
        .with_route("GET", targets_path, |_request| {
            (200, json!({"targets": [{"id": "local", "name": "Local (save only)", "home_target_set": true}]}))
        });
    let rest = client(&mock.origin());
    let runs = cron_api::job_runs(&rest, "fixture", "a1b2c3d4e5f6", 20)
        .await
        .expect("runs");
    assert!(runs.get("runs").is_some());
    let targets = cron_api::delivery_targets(&rest).await.expect("targets");
    assert!(targets.get("targets").is_some());
    let requests = mock.requests().await;
    assert_eq!(requests[0].method, "GET");
    assert_eq!(requests[0].path, runs_path);
    assert_eq!(requests[1].method, "GET");
    assert_eq!(requests[1].path, targets_path);
    assert!(!requests[1].path.contains('?'));
    for request in &requests {
        assert!(header(request, "x-hermes-gateway-ticket").is_some());
        assert!(header(request, "origin").is_none());
    }
}
