#[path = "../../hermes-client/tests/support/mock_rest.rs"]
mod mock_rest;

use hermes_client::{Ticket, TicketError, rest::TicketMinter};
use mock_rest::{CountingMinter, MockRest};
use serde_json::json;
use std::{collections::HashSet, sync::Arc};

struct FailingMinter;
impl TicketMinter for FailingMinter {
    fn mint(
        &self,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<Ticket, TicketError>> + Send + '_>>
    {
        Box::pin(async { Err(TicketError::Malformed) })
    }
}

use talaria_app::{
    comforts::prompt_history::PromptHistory,
    settings::{MinterFactory, SettingsService, append_history},
};
use talaria_core::settings::{
    ConfigSchemaView, FailReason, SettingsEffect, SettingsInput, WriteResult,
};

fn factory(minter: Arc<CountingMinter>) -> MinterFactory {
    Arc::new(move |_| minter.clone())
}

fn build_service(origin: Option<String>, minter: Arc<CountingMinter>) -> SettingsService {
    SettingsService::new(origin, vec!["fixture".into()], factory(minter))
}

fn load(profile: &str) -> SettingsEffect {
    SettingsEffect::Load {
        profile: profile.into(),
    }
}

fn failure(events: &[SettingsInput], expected: FailReason) {
    assert!(matches!(events, [SettingsInput::LoadFailed(actual)] if *actual == expected));
}

#[tokio::test]
async fn load_runs_four_reads_and_maps_failures() {
    let minter = Arc::new(CountingMinter::default());
    let disconnected = build_service(None, minter.clone());
    failure(
        &disconnected.execute(load("fixture")).await,
        FailReason::NotConnected,
    );
    assert_eq!(minter.count(), 0);

    let mock = MockRest::start().await;
    let service = build_service(Some(mock.origin()), minter.clone());
    failure(
        &service.execute(load("other")).await,
        FailReason::ProfileNotServed,
    );
    assert_eq!(minter.count(), 0);
    assert!(mock.requests().await.is_empty());

    let events = service.execute(load("fixture")).await;
    let [SettingsInput::Loaded(snapshot)] = events.as_slice() else {
        panic!("expected loaded snapshot")
    };
    let expected_schema =
        ConfigSchemaView::from_json_str(include_str!("../../../fixtures/rest/config-schema.json"))
            .expect("schema fixture");
    assert_eq!(snapshot.profile, "fixture");
    assert_eq!(snapshot.schema, expected_schema);
    let requests = mock.requests().await;
    assert_eq!(requests.len(), 4);
    assert!(requests.iter().all(|r| r.method == "GET"));
    let paths: HashSet<_> = requests.iter().map(|r| r.path.as_str()).collect();
    assert_eq!(
        paths,
        HashSet::from([
            "/api/config/schema",
            "/api/config/defaults",
            "/api/config",
            "/api/config?include_defaults=false"
        ])
    );
    let tickets: Vec<_> = requests
        .iter()
        .map(|r| {
            let headers: Vec<_> = r
                .headers
                .iter()
                .filter(|(name, _)| name.eq_ignore_ascii_case("x-hermes-gateway-ticket"))
                .collect();
            assert_eq!(headers.len(), 1);
            headers[0].1.as_str()
        })
        .collect();
    assert_eq!(tickets.iter().copied().collect::<HashSet<_>>().len(), 4);
    assert_eq!(minter.count(), 4);

    for (status, expected) in [
        (401, FailReason::Unauthorized),
        (403, FailReason::Http { status: 403 }),
        (
            404,
            FailReason::NotFound {
                route: "GET /api/config/schema".into(),
            },
        ),
        (400, FailReason::Http { status: 400 }),
        (422, FailReason::Http { status: 422 }),
        (500, FailReason::Http { status: 500 }),
    ] {
        let mock = MockRest::start().await;
        mock.fail_status("GET", "/api/config/schema", status, Some("unavailable"))
            .await;
        let service = build_service(Some(mock.origin()), Arc::new(CountingMinter::default()));
        failure(&service.execute(load("fixture")).await, expected);
    }
    let mock = MockRest::start().await;
    mock.response_body("GET", "/api/config/schema", b"not json".to_vec())
        .await;
    let service = build_service(Some(mock.origin()), Arc::new(CountingMinter::default()));
    failure(&service.execute(load("fixture")).await, FailReason::BadData);

    let mock = MockRest::start().await;
    mock.drop_after_request("GET", "/api/config/schema").await;
    let service = build_service(Some(mock.origin()), Arc::new(CountingMinter::default()));
    failure(&service.execute(load("fixture")).await, FailReason::Dropped);

    let mock = MockRest::start().await;
    mock.response_body("GET", "/api/config/schema", vec![b'x'; 9 * 1024 * 1024])
        .await;
    let service = build_service(Some(mock.origin()), Arc::new(CountingMinter::default()));
    failure(&service.execute(load("fixture")).await, FailReason::BadData);

    let mock = MockRest::start().await;
    mock.delay(
        "GET",
        "/api/config/schema",
        std::time::Duration::from_secs(16),
    )
    .await;
    let service = build_service(Some(mock.origin()), Arc::new(CountingMinter::default()));
    failure(
        &service.execute(load("fixture")).await,
        FailReason::Timeout { secs: 15 },
    );

    let mock = MockRest::start().await;
    let minter: MinterFactory = Arc::new(|_| Arc::new(FailingMinter));
    let service = SettingsService::new(Some(mock.origin()), vec!["fixture".into()], minter);
    failure(
        &service.execute(load("fixture")).await,
        FailReason::Ticket(TicketError::Malformed.to_string()),
    );

    let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("reserve port");
    let origin = format!("http://{}", listener.local_addr().expect("address"));
    drop(listener);
    let service = build_service(Some(origin), Arc::new(CountingMinter::default()));
    failure(
        &service.execute(load("fixture")).await,
        FailReason::NotConnected,
    );

    let service = build_service(
        Some("https://127.0.0.1:1234".into()),
        Arc::new(CountingMinter::default()),
    );
    failure(
        &service.execute(load("fixture")).await,
        FailReason::NotLoopback,
    );
}

#[tokio::test]
async fn apply_sequence_and_unknown_outcome() {
    let mock = MockRest::start().await;
    let minter = Arc::new(CountingMinter::default());
    let service = build_service(Some(mock.origin()), minter.clone());
    assert!(matches!(
        service.execute(load("fixture")).await.as_slice(),
        [SettingsInput::Loaded(_)]
    ));
    let start = mock.requests().await.len();
    assert!(matches!(
        service.execute(SettingsEffect::FetchFresh).await.as_slice(),
        [SettingsInput::Fresh(_)]
    ));
    mock.drop_after_request("PUT", "/api/config").await;
    let patch = json!({"display": {"compact": true}});
    assert!(matches!(
        service
            .execute(SettingsEffect::Write {
                patch: patch.clone()
            })
            .await
            .as_slice(),
        [SettingsInput::Written(WriteResult::Unknown)]
    ));
    assert!(matches!(
        service.execute(SettingsEffect::ReadBack).await.as_slice(),
        [SettingsInput::ReadBack { .. }]
    ));
    let requests = mock.requests().await;
    let sent = &requests[start..];
    assert_eq!(sent.len(), 4);
    assert_eq!((&*sent[0].method, &*sent[0].path), ("GET", "/api/config"));
    assert_eq!((&*sent[1].method, &*sent[1].path), ("PUT", "/api/config"));
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&sent[1].body).expect("request body"),
        json!({"config": patch})
    );
    let reads: HashSet<_> = sent[2..]
        .iter()
        .map(|r| (r.method.as_str(), r.path.as_str()))
        .collect();
    assert_eq!(
        reads,
        HashSet::from([
            ("GET", "/api/config"),
            ("GET", "/api/config?include_defaults=false")
        ])
    );
    assert_eq!(minter.count(), 8);
}

#[test]
fn secret_config_line_is_never_saved() {
    let dir = tempfile::tempdir().expect("temporary history");
    let (mut history, _) = PromptHistory::open(dir.path(), true, "fixture");
    let secret = "/config delegation.api_key=example-secret";
    let ordinary = "/config display.compact=true";
    append_history(&mut history, secret);
    append_history(&mut history, ordinary);
    let texts: Vec<_> = history
        .snapshot()
        .iter()
        .map(|entry| entry.text.clone())
        .collect();
    assert_eq!(texts, [ordinary]);
    let stored = std::fs::read_to_string(dir.path().join("history.jsonl")).expect("history file");
    assert!(stored.contains(ordinary));
    assert!(!stored.contains(secret));
    assert!(!stored.contains("example-secret"));
}
