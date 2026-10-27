#[path = "support/mock_rest.rs"]
mod mock_rest;

use hermes_client::{
    mcp_api::set_server_enabled,
    rest::{RestClient, RestError, RestOptions},
};
use mock_rest::{CountingMinter, MockRest};
use serde_json::json;
use std::sync::Arc;

#[tokio::test]
async fn enabled_toggle_request_shape() {
    let path = "/api/mcp/servers/a%20b%2Fc/enabled";
    let mock = MockRest::start().await.with_route("PUT", path, |_| {
        (200, json!({"ok":true,"name":"a b/c","enabled":false}))
    });
    let rest = RestClient::new(
        &mock.origin(),
        Arc::new(CountingMinter::default()),
        RestOptions::default(),
    )
    .unwrap();
    let reply = set_server_enabled(&rest, "a b/c", false).await.unwrap();
    assert!(reply.ok);
    assert_eq!(reply.name, "a b/c");
    assert!(!reply.enabled);
    let requests = mock.requests().await;
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0].method, "PUT");
    assert_eq!(requests[0].path, path);
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&requests[0].body).unwrap(),
        json!({"enabled":false})
    );
    assert!(
        !requests[0]
            .headers
            .iter()
            .any(|(name, _)| name.eq_ignore_ascii_case("origin"))
    );
    assert!(
        requests[0]
            .headers
            .iter()
            .any(|(name, _)| name.eq_ignore_ascii_case("x-hermes-gateway-ticket"))
    );
}

#[tokio::test]
async fn enabled_toggle_failures_classified() {
    let path = "/api/mcp/servers/example/enabled";
    let mock = MockRest::start().await;
    let rest = RestClient::new(
        &mock.origin(),
        Arc::new(CountingMinter::default()),
        RestOptions::default(),
    )
    .unwrap();
    for status in [400, 409] {
        mock.fail_status("PUT", path, status, Some("refused")).await;
        assert!(
            matches!(set_server_enabled(&rest, "example", true).await, Err(RestError::Refused { status: actual, .. }) if actual == status)
        );
    }
    mock.fail_status("PUT", path, 404, None).await;
    assert!(matches!(
        set_server_enabled(&rest, "example", true).await,
        Err(RestError::NotFound { .. })
    ));
    mock.fail_status("PUT", path, 500, None).await;
    assert!(matches!(
        set_server_enabled(&rest, "example", true).await,
        Err(RestError::Server { status: 500 })
    ));
}
