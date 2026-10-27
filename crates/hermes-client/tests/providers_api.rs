//! Provider dashboard calls. They fail while the client sends nothing.

#[path = "support/mock_rest.rs"]
mod mock_rest;

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use hermes_client::providers_api::{
    DetailWire, EndpointBodyWire, EndpointListWire, EnvVarWire, OauthPollWire,
    ProviderValidationWire, delete_endpoint, get_env, list_endpoints, poll_oauth, save_endpoint,
    start_oauth, validate_endpoint, validate_provider,
};
use hermes_client::rest::{RestClient, RestError, RestOptions, TicketMinter};
use hermes_client::{Ticket, TicketError};
use mock_rest::MockRest;
use serde_json::{Value, json};

const PROVIDER_KEY: &str = "sk-test-SENTINEL-0123456789abcdef";
const ENDPOINT_KEY: &str = "sk-test-ENDPOINT-0123456789abcdef";

struct Idle;

impl TicketMinter for Idle {
    fn mint(&self) -> Pin<Box<dyn Future<Output = Result<Ticket, TicketError>> + Send + '_>> {
        Box::pin(async { Err(TicketError::NotAuthenticated) })
    }
}

fn client(mock: &MockRest) -> RestClient {
    let minter: Arc<dyn TicketMinter> = Arc::new(Idle);
    RestClient::new(&mock.origin(), minter, RestOptions::default()).expect("loopback origin")
}

fn endpoint_body() -> EndpointBodyWire<'static> {
    EndpointBodyWire {
        id: Some(String::new()),
        name: Some("example-endpoint".into()),
        base_url: Some("https://example.invalid/v1".into()),
        model: Some("example-model".into()),
        api_key: Some(ENDPOINT_KEY),
        api_mode: Some(String::new()),
        context_length: None,
        discover_models: Some(true),
        make_default: Some(false),
        models: None,
        model_details: None,
    }
}

fn find<'a>(
    requests: &'a [mock_rest::RecordedRequest],
    method: &str,
    path: &str,
) -> Option<&'a mock_rest::RecordedRequest> {
    requests
        .iter()
        .find(|request| request.method == method && request.path == path)
}

#[tokio::test]
async fn requests_match_v0224_shapes() {
    let mock = MockRest::start().await;
    let rest = client(&mock);
    let _ = get_env(&rest).await;
    let _ = get_env(&rest).await;
    let _ = validate_provider(&rest, "EXAMPLE_API_KEY", PROVIDER_KEY).await;
    let _ = start_oauth(&rest, "dev.code").await;
    let _ = poll_oauth(&rest, "dev.code", "sess~1").await;
    let _ = delete_endpoint(&rest, "a/b c").await;
    let _ = validate_endpoint(&rest, &endpoint_body()).await;
    let _ = save_endpoint(&rest, &endpoint_body()).await;

    let requests = mock.requests();
    let env = find(&requests, "GET", "/api/env");
    assert!(env.is_some(), "the env list was not sent: {requests:?}");
    let env_calls: Vec<_> = requests
        .iter()
        .filter(|request| request.method == "GET" && request.path == "/api/env")
        .collect();
    assert_eq!(env_calls.len(), 2, "the env list was not read twice");
    assert!(
        env_calls.iter().all(|request| request.has_ticket),
        "a ticket was missing"
    );
    assert_ne!(
        env_calls[0].ticket, env_calls[1].ticket,
        "the second call reused the ticket"
    );
    assert!(
        env_calls
            .iter()
            .all(|request| !request.has_origin && !request.has_profile)
    );
    assert!(
        env_calls
            .iter()
            .all(|request| request.content_type.is_none())
    );

    let check = find(&requests, "POST", "/api/providers/validate").expect("validate was not sent");
    let body: Value = serde_json::from_str(check.body.as_deref().unwrap_or("{}")).expect("body");
    assert_eq!(
        body,
        json!({"key": "EXAMPLE_API_KEY", "value": PROVIDER_KEY})
    );
    assert!(body.get("profile").is_none(), "validate carried a profile");
    assert!(!check.has_origin && !check.has_profile);
    assert_eq!(check.content_type.as_deref(), Some("application/json"));

    let start = find(&requests, "POST", "/api/providers/oauth/dev.code/start")
        .expect("sign-in start was not sent");
    assert_eq!(start.body.as_deref(), Some("{}"));
    assert!(
        find(
            &requests,
            "GET",
            "/api/providers/oauth/dev.code/poll/sess~1"
        )
        .is_some()
    );
    assert!(
        find(
            &requests,
            "DELETE",
            "/api/providers/custom-endpoints/a%2Fb%20c"
        )
        .is_some()
    );
    assert!(
        find(
            &requests,
            "POST",
            "/api/providers/custom-endpoints/validate"
        )
        .is_some()
    );
    assert!(find(&requests, "POST", "/api/providers/custom-endpoints").is_some());
}

#[tokio::test]
async fn replies_parse_leniently() {
    let env: EnvVarWire = serde_json::from_str(
        r#"{"is_set":null,"redacted_value":null,"tools":null,"extra":{"kept":false}}"#,
    )
    .expect("an env entry ignores unknown fields and nulls");
    assert_eq!(env.is_set, None);
    assert_eq!(env.redacted_value, None);

    let poll: OauthPollWire = serde_json::from_str(
        r#"{"status":"pending","expires_at":1792700000,"reason":null,"unused":true}"#,
    )
    .expect("a poll ignores unknown fields");
    assert_eq!(poll.status.as_deref(), Some("pending"));
    assert!(poll.expires_at.is_some());

    let list: EndpointListWire = serde_json::from_value(json!({
        "endpoints": [{"id": "e1", "name": null, "models": null, "extra": 1}],
        "current": null,
        "surprise": true
    }))
    .expect("an endpoint list ignores unknown fields");
    assert_eq!(list.endpoints.len(), 1);
    assert_eq!(list.current, None);

    let check: ProviderValidationWire =
        serde_json::from_str(r#"{"ok":false,"reachable":true,"message":"no","extra":1}"#)
            .expect("a key check ignores unknown fields");
    assert_eq!(check.ok, Some(false));

    assert!(matches!(
        serde_json::from_str::<DetailWire>(r#""plain""#).expect("string detail"),
        DetailWire::Text(text) if text == "plain"
    ));
    assert!(matches!(
        serde_json::from_str::<DetailWire>(r#"["a", 1]"#).expect("list detail"),
        DetailWire::List(items) if items.len() == 2
    ));

    let mock = MockRest::start().await;
    mock.script(
        "GET",
        "/api/env",
        200,
        json!({"EXAMPLE_API_KEY": {"is_set": true, "redacted_value": "wxyz", "extra": 1}}),
    );
    let rest = client(&mock);
    let loaded = get_env(&rest).await.expect("the env list was not returned");
    let entry = loaded.get("EXAMPLE_API_KEY").and_then(Value::as_object);
    assert_eq!(
        entry
            .and_then(|item| item.get("is_set"))
            .and_then(Value::as_bool),
        Some(true),
        "the scripted env entry was not parsed"
    );
}

#[tokio::test]
async fn http_failures_are_classified() {
    let mock = MockRest::start().await;
    mock.script(
        "POST",
        "/api/providers/validate",
        400,
        json!({"detail": "no"}),
    );
    let rest = client(&mock);
    let refused = validate_provider(&rest, "EXAMPLE_API_KEY", PROVIDER_KEY)
        .await
        .expect_err("a 400 should fail");
    assert!(
        matches!(
            refused,
            RestError::Refused {
                status: 400,
                detail: Some(ref text)
            } if text == "no"
        ),
        "400 was not classified as refused: {refused}"
    );

    mock.script("POST", "/api/providers/validate", 401, json!({}));
    let unauthorized = validate_provider(&rest, "EXAMPLE_API_KEY", PROVIDER_KEY)
        .await
        .expect_err("a 401 should fail");
    assert!(
        matches!(unauthorized, RestError::Unauthorized),
        "401 was not classified: {unauthorized}"
    );

    mock.script(
        "POST",
        "/api/providers/validate",
        403,
        json!({"detail": "profile"}),
    );
    let forbidden = validate_provider(&rest, "EXAMPLE_API_KEY", PROVIDER_KEY)
        .await
        .expect_err("a 403 should fail");
    assert!(
        matches!(forbidden, RestError::Forbidden { ref detail } if detail == "profile"),
        "403 was not classified: {forbidden}"
    );

    mock.script("GET", "/api/providers/custom-endpoints", 404, json!({}));
    let missing = list_endpoints(&rest).await.expect_err("a 404 should fail");
    assert!(
        matches!(
            missing,
            RestError::NotFound { ref route }
                if route.contains("/api/providers/custom-endpoints")
        ),
        "404 was not classified: {missing}"
    );

    mock.script(
        "POST",
        "/api/providers/validate",
        422,
        json!({"detail": ["field", "missing"]}),
    );
    let invalid = validate_provider(&rest, "EXAMPLE_API_KEY", PROVIDER_KEY)
        .await
        .expect_err("a 422 should fail");
    assert!(
        matches!(
            invalid,
            RestError::Refused {
                status: 422,
                detail: None
            }
        ),
        "a list detail was not dropped: {invalid}"
    );

    mock.script(
        "POST",
        "/api/providers/validate",
        429,
        json!({"detail": "slow"}),
    );
    let limited = validate_provider(&rest, "EXAMPLE_API_KEY", PROVIDER_KEY)
        .await
        .expect_err("a 429 should fail");
    assert!(
        matches!(limited, RestError::Refused { status: 429, .. }),
        "429 was not classified: {limited}"
    );

    mock.script("POST", "/api/providers/validate", 500, json!({}));
    let server = validate_provider(&rest, "EXAMPLE_API_KEY", PROVIDER_KEY)
        .await
        .expect_err("a 500 should fail");
    assert!(
        matches!(server, RestError::Server { status: 500 }),
        "500 was not classified: {server}"
    );

    mock.script("POST", "/api/providers/validate", 504, json!({}));
    let gateway = validate_provider(&rest, "EXAMPLE_API_KEY", PROVIDER_KEY)
        .await
        .expect_err("a 504 should fail");
    assert!(
        matches!(gateway, RestError::Server { status: 504 }),
        "504 was not classified: {gateway}"
    );

    let closed = RestClient::new(
        "http://127.0.0.1:9",
        Arc::new(Idle) as Arc<dyn TicketMinter>,
        RestOptions::default(),
    )
    .expect("loopback");
    let connect = get_env(&closed)
        .await
        .expect_err("a closed port should fail");
    assert!(
        matches!(connect, RestError::Connect(_)),
        "a refused connection was not classified: {connect}"
    );

    mock.delay_after_read(
        "POST",
        "/api/providers/custom-endpoints",
        std::time::Duration::from_secs(31),
    );
    let timed = save_endpoint(&rest, &endpoint_body())
        .await
        .expect_err("a late reply should fail");
    assert!(
        matches!(timed, RestError::Timeout { sent: true }) && timed.outcome_unknown(),
        "a timeout after send was not classified: {timed}"
    );
}
