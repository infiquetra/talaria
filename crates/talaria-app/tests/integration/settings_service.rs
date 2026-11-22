use crate::client_support::mock_rest;

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
    settings::{MinterFactory, SettingsService, append_history, slash_choices},
};
use talaria_core::settings::{
    ConfigSchemaView, FailReason, RuntimeChoice, RuntimeList, SettingsEffect, SettingsInput,
    WriteResult,
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
    let expected_schema = ConfigSchemaView::from_json_str(include_str!(
        "../../../../fixtures/rest/config-schema.json"
    ))
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

    let mut mock = MockRest::start().await;
    mock.delay(
        "GET",
        "/api/config/schema",
        std::time::Duration::from_secs(16),
    )
    .await;
    mock.drive_paused_clock();
    tokio::time::pause();
    let service = build_service(Some(mock.origin()), Arc::new(CountingMinter::default()));
    failure(
        &service.execute(load("fixture")).await,
        FailReason::Timeout { secs: 15 },
    );
    drop(mock);
    tokio::time::resume();

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

fn list(key: &str, list: RuntimeList) -> SettingsEffect {
    SettingsEffect::LoadChoices {
        key: key.into(),
        list,
        profile: "fixture".into(),
    }
}

fn choice(value: &str, description: &str) -> RuntimeChoice {
    RuntimeChoice {
        value: value.into(),
        description: description.into(),
    }
}

fn listed(events: &[SettingsInput], key: &str) -> Vec<RuntimeChoice> {
    match events {
        [
            SettingsInput::ChoicesLoaded {
                key: answered,
                profile,
                items,
            },
        ] if answered == key && profile == "fixture" => items.clone(),
        _ => panic!("expected the {key} list"),
    }
}

fn refused(events: &[SettingsInput]) -> String {
    match events {
        [SettingsInput::ChoicesFailed { reason, .. }] => reason.clone(),
        _ => panic!("expected a failed list"),
    }
}

#[tokio::test]
async fn choice_lists_read_the_edited_profiles_routes() {
    let mock = MockRest::start()
        .await
        .with_route("GET", "/api/tools/terminal/backends", |_| {
            (
                200,
                json!({"active": "local", "backends": [
                    {"name": "local", "label": "Local", "description": "Run on this machine.",
                     "active": true, "status": "ready", "detail": ""},
                    {"name": "docker", "label": "Docker", "description": "Isolated container.",
                     "active": false, "status": "needs_setup", "detail": "docker is not installed"},
                    {"name": "example-plugin", "label": "Example", "description": "A plugin backend.",
                     "active": false, "status": "unavailable", "detail": ""}
                ]}),
            )
        })
        .with_route("GET", "/api/dashboard/themes", |_| {
            (
                200,
                json!({"active": "default", "themes": [
                    {"name": "default", "label": "Hermes Teal", "description": "Classic dark teal"},
                    {"name": "example-theme", "label": "Example", "description": "A user theme",
                     "definition": {"palette": {}}}
                ]}),
            )
        })
        .with_route("GET", "/api/dashboard/plugins/hub", |_| {
            (
                200,
                json!({"plugins": [], "orphan_dashboard_plugins": [], "providers": {
                    "memory_provider": "", "memory_options": [], "context_engine": "compressor",
                    "context_options": [{"name": "example-engine", "description": "A fixture engine"}]
                }}),
            )
        });
    let minter = Arc::new(CountingMinter::default());
    let service = SettingsService::new(
        Some(mock.origin()),
        vec!["fixture".into(), "example-work".into()],
        factory(minter.clone()),
    );
    // Nothing can be read before the settings of a profile are open.
    assert_eq!(
        refused(
            &service
                .execute(list("terminal.backend", RuntimeList::TerminalBackends))
                .await
        ),
        "the gateway is not reachable"
    );
    assert!(matches!(
        service.execute(load("fixture")).await.as_slice(),
        [SettingsInput::Loaded(_)]
    ));
    let start = mock.requests().await.len();

    assert_eq!(
        listed(
            &service
                .execute(list("terminal.backend", RuntimeList::TerminalBackends))
                .await,
            "terminal.backend"
        ),
        [
            choice("local", "Run on this machine."),
            choice(
                "docker",
                "Isolated container. Needs setup: docker is not installed."
            ),
            choice("example-plugin", "A plugin backend. Unavailable."),
        ]
    );
    assert_eq!(
        listed(
            &service
                .execute(list("dashboard.theme", RuntimeList::DashboardThemes))
                .await,
            "dashboard.theme"
        ),
        [
            choice("default", "Hermes Teal: Classic dark teal"),
            choice("example-theme", "Example: A user theme"),
        ]
    );
    assert_eq!(
        listed(
            &service
                .execute(list("context.engine", RuntimeList::ContextEngines))
                .await,
            "context.engine"
        ),
        [choice("example-engine", "A fixture engine")]
    );
    // The profile names are the ones this gateway serves; no request is needed.
    assert_eq!(
        listed(
            &service
                .execute(list("kanban.default_assignee", RuntimeList::Profiles))
                .await,
            "kanban.default_assignee"
        ),
        [choice("fixture", ""), choice("example-work", "")]
    );

    // Each read rides the edited profile's own ticket and names no other profile.
    let requests = mock.requests().await;
    let sent: Vec<_> = requests[start..]
        .iter()
        .map(|r| (r.method.as_str(), r.path.as_str()))
        .collect();
    assert_eq!(
        sent,
        [
            ("GET", "/api/tools/terminal/backends"),
            ("GET", "/api/dashboard/themes"),
            ("GET", "/api/dashboard/plugins/hub"),
        ]
    );
    assert_eq!(minter.count(), 7);

    mock.fail_status("GET", "/api/dashboard/plugins/hub", 500, Some("broken"))
        .await;
    assert_eq!(
        refused(
            &service
                .execute(list("context.engine", RuntimeList::ContextEngines))
                .await
        ),
        "HTTP 500"
    );
    mock.response_body("GET", "/api/dashboard/themes", b"[]".to_vec())
        .await;
    assert_eq!(
        refused(
            &service
                .execute(list("dashboard.theme", RuntimeList::DashboardThemes))
                .await
        ),
        "the reply was not valid settings data"
    );
}

#[tokio::test]
async fn skin_and_personality_lists_come_from_slash_completion() {
    let (mock, client, _inbound) = crate::mock_gateway::MockGateway::start().await;
    mock.set_skin_names(&["default", "example-user-skin"]);
    mock.set_skin_descriptions(&[
        ("default", "Classic Hermes"),
        ("example-user-skin", "A user skin"),
    ]);
    mock.set_personality_rows(&[
        ("none", "clear personality overlay"),
        ("helpful", "You are a helpful, friendly AI assistant."),
        ("example-persona", "A fixture personality"),
    ]);
    assert_eq!(
        slash_choices(&client, None, RuntimeList::Skins).await,
        Ok(vec![
            choice("default", "Classic Hermes"),
            choice("example-user-skin", "A user skin"),
        ])
    );
    // "none" is the "No personality" row Talaria already shows.
    assert_eq!(
        slash_choices(
            &client,
            Some(crate::mock_gateway::SESSION_ID),
            RuntimeList::Personalities
        )
        .await,
        Ok(vec![
            choice("helpful", "You are a helpful, friendly AI assistant."),
            choice("example-persona", "A fixture personality"),
        ])
    );
    let asked = mock.requests("complete.slash");
    assert_eq!(asked.len(), 2);
    assert_eq!(asked[0].params["text"], "/skin ");
    assert_eq!(asked[1].params["text"], "/personality ");
    assert_eq!(
        asked[1].params["session_id"],
        crate::mock_gateway::SESSION_ID
    );
    // The connection's own profile is used: no call names a profile.
    assert!(
        asked
            .iter()
            .all(|seen| seen.params.get("profile").is_none())
    );

    mock.set_skin_list_unavailable(true);
    assert_eq!(
        slash_choices(&client, None, RuntimeList::Skins).await,
        Err("Hermes refused the request".to_owned())
    );
    assert!(mock.violations().is_empty());
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
