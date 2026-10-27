#[path = "support/mock_rest.rs"]
mod mock_rest;

use hermes_client::{
    TicketError,
    rest::{Method, RequestOpts, RestClient, RestError, RestOptions},
};
use mock_rest::{CountingMinter, MockRest, RecordedRequest};
use serde_json::json;
use std::{sync::Arc, time::Duration};

fn client(origin: &str, minter: Arc<CountingMinter>) -> RestClient {
    RestClient::new(origin, minter, RestOptions::default()).expect("loopback origin")
}
fn headers(request: &RecordedRequest) -> Vec<String> {
    let mut names: Vec<_> = request
        .headers
        .iter()
        .map(|(name, _)| name.to_ascii_lowercase())
        .collect();
    names.sort();
    names
}
fn header<'a>(request: &'a RecordedRequest, name: &str) -> &'a str {
    let values: Vec<_> = request
        .headers
        .iter()
        .filter(|(key, _)| key.eq_ignore_ascii_case(name))
        .map(|(_, value)| value.as_str())
        .collect();
    assert_eq!(values.len(), 1);
    values[0]
}

#[test]
fn refuses_non_loopback_and_https_origins() {
    let minter = Arc::new(CountingMinter::default());
    for origin in ["https://127.0.0.1:1234", "https://[::1]:1234"] {
        assert!(matches!(
            RestClient::new(origin, minter.clone(), RestOptions::default()),
            Err(RestError::TlsNotSupported)
        ));
    }
    for origin in ["http://192.0.2.1:1234", "http://0.0.0.0:1234"] {
        assert!(matches!(
            RestClient::new(origin, minter.clone(), RestOptions::default()),
            Err(RestError::NotLoopback)
        ));
    }
    assert_eq!(minter.count(), 0);
}

#[tokio::test]
async fn sends_exact_request_with_fresh_ticket_and_no_origin() {
    let mock = MockRest::start().await;
    let minter = Arc::new(CountingMinter::default());
    let rest = client(&mock.origin(), minter.clone());
    rest.get_json("/api/config").await.unwrap();
    rest.get_json("/api/config").await.unwrap();
    let _ = rest.post_json("/api/config", &json!({"config":{}})).await;
    let _ = rest.delete_json("/api/config").await;
    rest.request(
        Method::Get,
        "/api/config/schema",
        None,
        RequestOpts {
            ticket: false,
            ..RequestOpts::default()
        },
    )
    .await
    .unwrap();
    let requests = mock.requests().await;
    assert_eq!(requests.len(), 5);
    assert_eq!(minter.count(), 4);
    assert_ne!(
        header(&requests[0], "x-hermes-gateway-ticket"),
        header(&requests[1], "x-hermes-gateway-ticket")
    );
    let base = vec!["accept", "connection", "host", "x-hermes-gateway-ticket"];
    assert_eq!(headers(&requests[0]), base);
    assert_eq!(headers(&requests[1]), base);
    assert_eq!(
        headers(&requests[2]),
        vec![
            "accept",
            "connection",
            "content-length",
            "content-type",
            "host",
            "x-hermes-gateway-ticket"
        ]
    );
    assert_eq!(headers(&requests[3]), base);
    assert_eq!(headers(&requests[4]), vec!["accept", "connection", "host"]);
    assert_eq!(
        requests
            .iter()
            .map(|r| r.method.as_str())
            .collect::<Vec<_>>(),
        ["GET", "GET", "POST", "DELETE", "GET"]
    );
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&requests[2].body).unwrap(),
        json!({"config":{}})
    );
    assert_eq!(header(&requests[2], "content-type"), "application/json");
    assert_eq!(
        header(&requests[2], "content-length")
            .parse::<usize>()
            .unwrap(),
        requests[2].body.len()
    );
    assert!(requests[3].body.is_empty());
    for request in &requests {
        assert_eq!(header(request, "accept"), "application/json");
        assert_eq!(header(request, "connection").to_ascii_lowercase(), "close");
    }
}

#[tokio::test]
async fn retries_401_once_with_a_fresh_ticket() {
    let mock = MockRest::start().await;
    mock.first_401("GET", "/api/config").await;
    let minter = Arc::new(CountingMinter::default());
    let rest = client(&mock.origin(), minter.clone());
    rest.get_json("/api/config").await.unwrap();
    assert_eq!(minter.count(), 2);
    let requests = mock.requests().await;
    assert_eq!(requests.len(), 2);
    assert_ne!(
        header(&requests[0], "x-hermes-gateway-ticket"),
        header(&requests[1], "x-hermes-gateway-ticket")
    );

    let mock = MockRest::start().await;
    mock.fail_status("GET", "/api/config", 401, None).await;
    let minter = Arc::new(CountingMinter::default());
    let rest = client(&mock.origin(), minter.clone());
    assert!(matches!(
        rest.get_json("/api/config").await,
        Err(RestError::Unauthorized)
    ));
    assert_eq!(minter.count(), 2);
    assert_eq!(mock.requests().await.len(), 2);

    let mock = MockRest::start().await;
    mock.fail_status("GET", "/api/config/schema", 401, None)
        .await;
    let minter = Arc::new(CountingMinter::default());
    let rest = client(&mock.origin(), minter.clone());
    assert!(matches!(
        rest.request(
            Method::Get,
            "/api/config/schema",
            None,
            RequestOpts {
                ticket: false,
                ..RequestOpts::default()
            }
        )
        .await,
        Err(RestError::Unauthorized)
    ));
    assert_eq!(minter.count(), 0);
    assert_eq!(mock.requests().await.len(), 1);

    let mock = MockRest::start().await;
    let minter = Arc::new(CountingMinter::default());
    let rest = client(&mock.origin(), minter.clone());
    assert_eq!(
        rest.probe_ticket_reuse("/api/config").await.unwrap(),
        (200, 401)
    );
    assert_eq!(minter.count(), 1);
}

#[tokio::test]
async fn maps_status_and_transport_failures() {
    for status in [400, 403, 404, 422, 500] {
        let mock = MockRest::start().await;
        mock.fail_status(
            "GET",
            "/api/config/schema?query=1",
            status,
            if status == 422 { None } else { Some("detail") },
        )
        .await;
        let rest = client(&mock.origin(), Arc::new(CountingMinter::default()));
        let error = rest
            .get_json("/api/config/schema?query=1")
            .await
            .unwrap_err();
        match (status, &error) {
            (
                400,
                RestError::Refused {
                    status: 400,
                    detail: Some(detail),
                },
            ) => assert_eq!(detail, "detail"),
            (403, RestError::Forbidden { detail }) => assert_eq!(detail, "detail"),
            (404, RestError::NotFound { route }) => assert_eq!(route, "GET /api/config/schema"),
            (
                422,
                RestError::Refused {
                    status: 422,
                    detail: None,
                },
            ) => {}
            (500, RestError::Server { status: 500 }) => {}
            _ => panic!("wrong mapping: {error:?}"),
        }
        assert!(!error.outcome_unknown());
    }
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    drop(listener);
    let rest = client(
        &format!("http://{address}"),
        Arc::new(CountingMinter::default()),
    );
    let error = rest.get_json("/api/config").await.unwrap_err();
    assert!(matches!(error, RestError::Connect(_)));
    assert!(!error.outcome_unknown());

    let mock = MockRest::start().await;
    mock.drop_after_request("PUT", "/api/config").await;
    let rest = client(&mock.origin(), Arc::new(CountingMinter::default()));
    let error = rest
        .put_json("/api/config", &json!({"config":{}}))
        .await
        .unwrap_err();
    assert!(matches!(error, RestError::Dropped { sent: true }));
    assert!(error.outcome_unknown());

    let mock = MockRest::start().await;
    mock.delay("GET", "/api/config", Duration::from_secs(2))
        .await;
    let rest = client(&mock.origin(), Arc::new(CountingMinter::default()));
    let error = rest
        .request(
            Method::Get,
            "/api/config",
            None,
            RequestOpts {
                deadline: Some(Duration::from_millis(200)),
                ..RequestOpts::default()
            },
        )
        .await
        .unwrap_err();
    assert!(matches!(error, RestError::Timeout { sent: true }));
    assert!(error.outcome_unknown());
    for error in [
        RestError::Timeout { sent: false },
        RestError::Dropped { sent: false },
    ] {
        assert!(!error.outcome_unknown());
    }

    for (body, oversized) in [
        (vec![b'x'; 9 * 1024 * 1024], true),
        (b"not json".to_vec(), false),
    ] {
        let mock = MockRest::start().await;
        mock.response_body("GET", "/api/config/schema", body).await;
        let rest = client(&mock.origin(), Arc::new(CountingMinter::default()));
        let error = rest.get_json("/api/config/schema").await.unwrap_err();
        if oversized {
            assert!(matches!(error, RestError::BodyTooLarge));
        } else {
            assert!(matches!(error, RestError::BadBody(_)));
        }
        assert!(!error.outcome_unknown());
    }
}

#[tokio::test]
async fn ticket_never_appears_in_errors_or_debug() {
    let mock = MockRest::start().await;
    let minter = Arc::new(CountingMinter::default());
    let rest = client(&mock.origin(), minter.clone());
    mock.fail_status("GET", "/api/config", 403, Some("TICKET-SENTINEL-0"))
        .await;
    let echoed = rest.get_json("/api/config").await.unwrap_err();
    assert_eq!(minter.count(), 1);
    assert!(!format!("{echoed:?}").contains("TICKET-SENTINEL"));
    assert!(!format!("{echoed}").contains("TICKET-SENTINEL"));
    let errors = [
        RestError::NotLoopback,
        RestError::TlsNotSupported,
        RestError::Ticket(TicketError::Refused("TICKET-SENTINEL-0".into())),
        RestError::Connect(std::io::Error::other("connect")),
        RestError::Timeout { sent: true },
        RestError::Dropped { sent: true },
        RestError::Unauthorized,
        RestError::Forbidden {
            detail: "denied".into(),
        },
        RestError::NotFound {
            route: "GET /api/config".into(),
        },
        RestError::Refused {
            status: 400,
            detail: Some("invalid".into()),
        },
        RestError::Server { status: 500 },
        RestError::BodyTooLarge,
        RestError::BadBody("bad JSON".into()),
    ];
    assert!(!format!("{rest:?}").contains("TICKET-SENTINEL"));
    assert!(!format!("{rest}").contains("TICKET-SENTINEL"));
    for error in errors {
        assert!(!format!("{error:?}").contains("TICKET-SENTINEL"));
        assert!(!format!("{error}").contains("TICKET-SENTINEL"));
    }
}
