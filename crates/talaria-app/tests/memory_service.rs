#[path = "../../hermes-client/tests/support/mod.rs"]
mod gateway_support;
use gateway_support::mock_rest;

use std::{collections::BTreeMap, sync::Arc, time::Duration};

use hermes_client::rest::{RestClient, RestOptions};
use mock_rest::{CountingMinter, MockRest};
use serde_json::json;
use talaria_app::memory::MemoryService;
use talaria_core::memory::{
    ConfigSurface, MemoryEffect, MemoryInput, ResetTarget, SubmittedValues, WriteOutcome,
};

fn service(mock: &MockRest, profile: &str) -> MemoryService {
    let rest = RestClient::new(
        &mock.origin(),
        Arc::new(CountingMinter::default()),
        RestOptions::default(),
    )
    .unwrap();
    MemoryService::new(Arc::new(rest), profile.into())
}

async fn mock() -> MockRest {
    let stored = Arc::new(std::sync::Mutex::new(String::from("old")));
    let read_stored = stored.clone();
    MockRest::start().await
        .with_route("GET", "/api/memory", |_| (200, json!({"active":"", "providers":[], "builtin_files":{"memory":4,"user":2}})))
        .with_route("GET", "/api/curator", |_| (200, json!({"enabled":true,"paused":false})))
        .with_route("PUT", "/api/memory/provider", |_| (200, json!({"ok":true,"active":"demo"})))
        .with_route("GET", "/api/memory/providers/demo/config?surface=declared", |_| (200, json!({"name":"demo","label":"Demo","fields":[]})))
        .with_route("GET", "/api/memory/providers/demo/config", move |_| (200, json!({"name":"demo","label":"Demo","fields":[{"key":"endpoint","kind":"text","value":*read_stored.lock().unwrap()}]})))
        .with_route("PUT", "/api/memory/providers/demo/config", move |request| {
            let body: serde_json::Value = serde_json::from_slice(&request.body).unwrap();
            *stored.lock().unwrap() = body["values"]["endpoint"].as_str().unwrap().to_owned();
            (400, json!({"detail":"not ready"}))
        })
        .with_route("POST", "/api/memory/reset?profile=fixture", |_| (200, json!({"ok":true,"deleted":["memory"]})))
        .with_route("PUT", "/api/curator/paused", |_| (200, json!({"ok":true,"paused":true})))
        .with_route("POST", "/api/curator/run?profile=fixture", |_| (200, json!({"ok":true,"name":"curator-run","pid":42})))
        .with_route("GET", "/api/actions/curator-run/status?lines=40", |_| (200, json!({"name":"curator-run","running":true,"pid":42,"exit_code":null,"lines":[]})))
}

#[tokio::test(start_paused = true)]
async fn full_flow_against_mock_rest() {
    let mut mock = mock().await;
    mock.drive_paused_clock();
    let service = Arc::new(service(&mock, "fixture"));
    assert!(matches!(
        service.execute(MemoryEffect::LoadStatus).await.as_slice(),
        [MemoryInput::StatusLoaded(Ok(_))]
    ));
    assert!(matches!(
        service.execute(MemoryEffect::LoadCurator).await.as_slice(),
        [MemoryInput::CuratorLoaded(Ok(_))]
    ));
    assert!(matches!(
        service
            .execute(MemoryEffect::SetProvider {
                name: "demo".into()
            })
            .await
            .as_slice(),
        [MemoryInput::ProviderSet(WriteOutcome::Ok(_))]
    ));
    assert!(matches!(
        service.execute(MemoryEffect::LoadStatus).await.as_slice(),
        [MemoryInput::StatusLoaded(Ok(_))]
    ));
    let readback = service
        .execute(MemoryEffect::LoadConfig {
            name: "demo".into(),
        })
        .await;
    assert!(
        matches!(readback.as_slice(), [MemoryInput::ConfigLoaded { result: Ok(config), .. }] if config.fields[0].value == json!("old"))
    );
    let values = SubmittedValues {
        plain: BTreeMap::from([("endpoint".into(), json!("new"))]),
        secrets: BTreeMap::new(),
    };
    assert!(matches!(
        service
            .execute(MemoryEffect::SaveConfig {
                name: "demo".into(),
                surface: ConfigSurface::Plain,
                values
            })
            .await
            .as_slice(),
        [MemoryInput::ConfigSaved(WriteOutcome::Refused {
            status: 400,
            ..
        })]
    ));
    let readback = service
        .execute(MemoryEffect::LoadConfig {
            name: "demo".into(),
        })
        .await;
    assert!(
        matches!(readback.as_slice(), [MemoryInput::ConfigLoaded { result: Ok(config), .. }] if config.fields[0].value == json!("new"))
    );
    service.execute(MemoryEffect::LoadStatus).await;
    assert!(matches!(
        service
            .execute(MemoryEffect::Reset {
                target: ResetTarget::Memory
            })
            .await
            .as_slice(),
        [MemoryInput::ResetDone(WriteOutcome::Ok(_))]
    ));
    service.execute(MemoryEffect::LoadStatus).await;
    assert!(matches!(
        service
            .execute(MemoryEffect::SetPaused(true))
            .await
            .as_slice(),
        [MemoryInput::PausedSet(WriteOutcome::Ok(_))]
    ));
    service.execute(MemoryEffect::LoadCurator).await;
    assert!(matches!(
        service.execute(MemoryEffect::RunCurator).await.as_slice(),
        [MemoryInput::RunStarted(WriteOutcome::Ok(_))]
    ));
    let polling = tokio::spawn({
        let service = service.clone();
        async move {
            service
                .execute(MemoryEffect::PollAction {
                    follow: 7,
                    after: Duration::from_millis(1200),
                })
                .await
        }
    });
    tokio::task::yield_now().await;
    assert!(!polling.is_finished());
    tokio::time::advance(Duration::from_millis(1199)).await;
    assert!(!polling.is_finished());
    tokio::time::advance(Duration::from_millis(1)).await;
    assert!(matches!(
        polling.await.unwrap().as_slice(),
        [MemoryInput::ActionPolled {
            follow: 7,
            result: Ok(_)
        }]
    ));
    let requests = mock.requests().await;
    let actual: Vec<_> = requests
        .iter()
        .map(|r| (r.method.as_str(), r.path.as_str()))
        .collect();
    assert_eq!(
        actual,
        [
            ("GET", "/api/memory"),
            ("GET", "/api/curator"),
            ("PUT", "/api/memory/provider"),
            ("GET", "/api/memory"),
            ("GET", "/api/memory/providers/demo/config?surface=declared"),
            ("GET", "/api/memory/providers/demo/config"),
            ("PUT", "/api/memory/providers/demo/config"),
            ("GET", "/api/memory/providers/demo/config?surface=declared"),
            ("GET", "/api/memory/providers/demo/config"),
            ("GET", "/api/memory"),
            ("POST", "/api/memory/reset?profile=fixture"),
            ("GET", "/api/memory"),
            ("PUT", "/api/curator/paused"),
            ("GET", "/api/curator"),
            ("POST", "/api/curator/run?profile=fixture"),
            ("GET", "/api/actions/curator-run/status?lines=40")
        ]
    );
    for (index, body) in [
        (2, json!({"provider":"demo"})),
        (6, json!({"values":{"endpoint":"new"}})),
        (10, json!({"target":"memory"})),
        (12, json!({"paused":true})),
        (14, json!({})),
    ] {
        assert_eq!(
            serde_json::from_slice::<serde_json::Value>(&requests[index].body).unwrap(),
            body
        );
    }
}

#[tokio::test]
async fn profile_query_is_the_connection_profile() {
    use std::time::Instant;
    use talaria_core::memory::{BuiltinFiles, MemoryKey, MemoryState, StatusView};
    let mock =
        MockRest::start()
            .await
            .with_route("POST", "/api/memory/reset?profile=fixture", |_| {
                (403, json!({"detail":"profile_scope_mismatch"}))
            });
    let now = Instant::now();
    let mut state = MemoryState::new();
    state.update(
        MemoryInput::Open {
            profile: "fixture".into(),
        },
        now,
    );
    state.update(
        MemoryInput::StatusLoaded(Ok(StatusView {
            active: String::new(),
            providers: Vec::new(),
            builtin_files: BuiltinFiles { memory: 4, user: 2 },
        })),
        now,
    );
    state.update(MemoryInput::Key(MemoryKey::DeleteBoth), now);
    state.update(MemoryInput::Key(MemoryKey::Right), now);
    let mut effects = state.update(MemoryInput::Key(MemoryKey::Open), now);
    assert!(matches!(
        effects.as_slice(),
        [MemoryEffect::Reset {
            target: ResetTarget::All
        }]
    ));
    let events = service(&mock, "fixture").execute(effects.remove(0)).await;
    assert!(matches!(
        events.as_slice(),
        [MemoryInput::ResetDone(WriteOutcome::Refused {
            status: 403,
            ..
        })]
    ));
    let requests = mock.requests().await;
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0].path, "/api/memory/reset?profile=fixture");
    for event in events {
        state.update(event, now);
    }
    assert_eq!(
        state.view(now).banner.as_deref(),
        Some("Hermes refused this for profile fixture (profile_scope_mismatch).")
    );
}

#[tokio::test]
async fn secret_never_logged_or_recorded() {
    use gateway_support::{
        ControlReply, Echo, Home, INSTANCE_ID, WsPlan, ensure_fixture, ready_identify, serve,
        success_steps,
    };
    use hermes_client::{
        DiscoverOptions, EnsurePolicy, Purpose, Recorder, RecorderOptions, Scope, connect,
        discover_with, handshake, mint_ticket, scan_literals,
    };
    use std::time::Instant;
    use talaria_core::memory::{
        BuiltinFiles, ConfigView, FieldKind, FieldView, MemoryKey, MemoryState, ProviderRowView,
        ProviderSetup, StatusView,
    };

    let marker = "sentinel-secret-memory-save".to_owned();
    let mock =
        MockRest::start()
            .await
            .with_route("PUT", "/api/memory/providers/demo/config", |_| {
                (200, json!({"ok":true,"active":"demo"}))
            });
    let now = Instant::now();
    let mut state = MemoryState::new();
    let mut diagnostic_buffer = String::new();
    let inputs = [
        MemoryInput::Open {
            profile: "fixture".into(),
        },
        MemoryInput::StatusLoaded(Ok(StatusView {
            active: "demo".into(),
            providers: vec![ProviderRowView {
                name: "demo".into(),
                description: String::new(),
                available: true,
                configured: true,
                status: "ready".into(),
                setup: ProviderSetup {
                    pip_dependencies: vec![],
                    python_dependencies_declared: false,
                    external_dependencies: vec![],
                    required_env: vec![],
                    dependencies_installed: true,
                },
            }],
            builtin_files: BuiltinFiles { memory: 0, user: 0 },
        })),
        MemoryInput::Key(MemoryKey::Down),
        MemoryInput::Key(MemoryKey::Open),
        MemoryInput::ConfigLoaded {
            name: "demo".into(),
            result: Ok(ConfigView {
                name: "demo".into(),
                label: "Demo".into(),
                surface: ConfigSurface::Plain,
                fields: vec![FieldView {
                    key: "api_key".into(),
                    label: "API key".into(),
                    kind: FieldKind::Secret,
                    description: String::new(),
                    placeholder: String::new(),
                    required: false,
                    value: json!(""),
                    is_set: false,
                    options: vec![],
                    when: None,
                    minimum: None,
                    maximum: None,
                }],
            }),
        },
        MemoryInput::Key(MemoryKey::Open),
        MemoryInput::Key(MemoryKey::Paste(marker.clone())),
        MemoryInput::Key(MemoryKey::Open),
    ];
    for input in inputs {
        let effects = state.update(input, now);
        diagnostic_buffer.push_str(&format!("{:?}\n{effects:?}\n", state.view(now)));
    }
    let mut effects = state.update(MemoryInput::Key(MemoryKey::Save), now);
    diagnostic_buffer.push_str(&format!("{:?}\n{effects:?}\n", state.view(now)));
    assert!(matches!(
        effects.as_slice(),
        [MemoryEffect::SaveConfig { .. }]
    ));
    let effect = effects.remove(0);
    assert!(scan_literals(&diagnostic_buffer, std::slice::from_ref(&marker)).is_empty());
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("session.jsonl");
    let recorder = Recorder::create(&path, RecorderOptions::default()).unwrap();
    let home = Home::new();
    ensure_fixture(home.path());
    let canonical = home.canonical();
    let fixture_id = format!("{canonical}/profiles/fixture");
    let _gateway = serve(&home.path().join("gateway.sock"), {
        let fixture_id = fixture_id.clone();
        move |origin| vec![
            ControlReply::Line(json!({"ok":true,"protocol":1,"id":1,"result":ready_identify(&canonical, origin)}).to_string()),
            ControlReply::Issue { profile_id: fixture_id, instance_id: INSTANCE_ID.into(), expired: false },
        ]
    }, WsPlan::Talk { echo: Echo::Version1, steps: success_steps(&fixture_id), guard: false }).await;
    let endpoint = discover_with(
        home.path(),
        &DiscoverOptions {
            control_timeout: Duration::from_secs(5),
            ensure: EnsurePolicy::Never,
        },
    )
    .await
    .unwrap();
    let profile = endpoint.resolve_profile("fixture").unwrap();
    let ticket = mint_ticket(
        home.path(),
        &endpoint,
        &profile,
        Purpose::Interactive,
        Scope::profile(),
    )
    .await
    .unwrap();
    let mut connection = connect(&endpoint, ticket).await.unwrap();
    connection.set_recorder(recorder.clone(), 1);
    let service = service(&mock, "fixture");
    let (handshake_result, events) =
        tokio::join!(handshake(&mut connection), service.execute(effect));
    handshake_result.expect("recorded handshake");
    let output = format!("{events:?}");
    assert!(scan_literals(&output, std::slice::from_ref(&marker)).is_empty());
    diagnostic_buffer.push_str(&output);
    for event in events {
        let effects = state.update(event, now);
        diagnostic_buffer.push_str(&format!("{:?}\n{effects:?}\n", state.view(now)));
    }
    assert!(scan_literals(&diagnostic_buffer, std::slice::from_ref(&marker)).is_empty());
    let summary = recorder.finish().unwrap();
    assert!(summary.frames >= 5);
    for entry in std::fs::read_dir(dir.path()).unwrap() {
        let recording = std::fs::read_to_string(entry.unwrap().path()).unwrap();
        assert!(scan_literals(&recording, std::slice::from_ref(&marker)).is_empty());
    }
    let requests = mock.requests().await;
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&requests[0].body).unwrap()["values"]["api_key"],
        marker
    );
}
