#[path = "support/memory_mock.rs"]
mod memory_mock;
#[path = "support/mock_rest.rs"]
mod mock_rest;

use hermes_client::{
    memory_api::*,
    rest::{RestClient, RestError, RestOptions},
};
use mock_rest::{CountingMinter, MockRest};
use proptest::prelude::*;
use serde_json::{Value, json};
use std::{path::Path, sync::Arc, time::Duration};

fn fixture(name: &str) -> Value {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/rest/memory")
        .join(name);
    serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap()
}
fn client(mock: &MockRest) -> RestClient {
    RestClient::new(
        &mock.origin(),
        Arc::new(CountingMinter::default()),
        RestOptions::default(),
    )
    .unwrap()
}

#[test]
fn fixtures_parse_and_are_private() {
    for name in ["status-builtin.json", "status-retaindb-active.json"] {
        parse_memory_status(&fixture(name)).unwrap();
    }
    for (name, surface) in [
        ("retaindb-plain.json", ConfigSurface::Plain),
        ("byterover-plain.json", ConfigSurface::Plain),
        ("declared-empty.json", ConfigSurface::Declared),
        ("declared-sample.json", ConfigSurface::Declared),
    ] {
        parse_provider_config(&fixture(name), surface).unwrap();
    }
    parse_curator(&fixture("curator.json")).unwrap();
    parse_action_status(&fixture("action-curator-run.json")).unwrap();
    let fields = &fixture("retaindb-plain.json")["fields"];
    assert_eq!(fields[0]["key"], "api_key");
    assert_eq!(fields[0]["required"], true);
    assert_eq!(fields[0]["is_set"], false);
    assert_eq!(fields[1]["value"], "https://api.retaindb.com");
    assert_eq!(fields[2]["key"], "project");
    let directory = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/rest/memory");
    let email = regex::Regex::new(r"@[A-Za-z0-9.-]+\.[A-Za-z]{2,}").unwrap();
    for entry in std::fs::read_dir(directory).unwrap() {
        let text = std::fs::read_to_string(entry.unwrap().path()).unwrap();
        for denied in [
            format!("/{}/", "Users"),
            "/home/".into(),
            "/private/".into(),
            "/root/".into(),
            "C:\\\\".into(),
            "~/".into(),
        ] {
            assert!(!text.contains(&denied));
        }
        assert!(!email.is_match(&text));
        let value: Value = serde_json::from_str(&text).unwrap();
        fn profiles(v: &Value) {
            match v {
                Value::Object(m) => {
                    for (k, v) in m {
                        if k == "profile" {
                            assert!(v == "default" || v == "fixture");
                        }
                        profiles(v);
                    }
                }
                Value::Array(a) => {
                    for v in a {
                        profiles(v);
                    }
                }
                _ => {}
            }
        }
        profiles(&value);
    }
}

#[test]
fn parsing_tolerates_missing_and_unknown_fields() {
    let status =
        parse_memory_status(&json!({"providers":[{"name":"example","status":"future"}]})).unwrap();
    assert_eq!(status.active, "");
    assert_eq!(status.providers[0].status, "future");
    assert!(!status.providers[0].available);
    assert!(status.providers[0].setup.dependencies_installed);
    assert_eq!(status.builtin_files.memory, 0);
    let config = parse_provider_config(
        &json!({"name":"example","label":"Example","fields":[{"key":"x","kind":"future"}]}),
        ConfigSurface::Declared,
    )
    .unwrap();
    assert_eq!(config.fields[0].kind, FieldKind::Other("future".into()));
    let c = parse_curator(&json!({"interval_hours":12.5,"stale_after_days":14})).unwrap();
    assert!(c.enabled);
    assert!(!c.paused);
    assert_eq!(c.interval_hours, Some(12.5));
    assert_eq!(c.stale_after_days, Some(14.0));
    assert!(
        matches!(parse_memory_status(&json!({"providers":42})), Err(MemoryApiError::BadReply(path)) if path == "providers")
    );
}

#[test]
fn secrets_never_in_debug_or_errors() {
    let sentinel = "secret-sentinel-for-test";
    let error = MemoryApiError::Rest(RestError::Refused {
        status: 400,
        detail: Some(sentinel.into()),
    });
    assert!(!format!("{error:?}").contains(sentinel));
    assert!(!format!("{error}").contains(sentinel));
    let error = parse_memory_status(&json!({"providers": sentinel})).unwrap_err();
    assert!(!format!("{error:?}").contains(sentinel));
    assert!(!format!("{error}").contains(sentinel));
}

async fn routes() -> MockRest {
    MockRest::start()
        .await
        .with_route("GET", "/api/memory", |_| {
            (200, fixture("status-builtin.json"))
        })
        .with_route("PUT", "/api/memory/provider", |_| {
            (200, json!({"ok":true,"active":""}))
        })
        .with_route("POST", "/api/memory/reset?profile=fixture", |_| {
            (200, json!({"ok":true,"deleted":["user"]}))
        })
        .with_route(
            "GET",
            "/api/memory/providers/retaindb/config?surface=declared",
            |_| (200, fixture("declared-empty.json")),
        )
        .with_route("GET", "/api/memory/providers/retaindb/config", |_| {
            (200, fixture("retaindb-plain.json"))
        })
        .with_route("PUT", "/api/memory/providers/retaindb/config", |_| {
            (200, json!({"ok":true,"active":"retaindb"}))
        })
        .with_route("GET", "/api/curator", |_| (200, fixture("curator.json")))
        .with_route("PUT", "/api/curator/paused", |_| {
            (200, json!({"ok":true,"paused":true}))
        })
        .with_route("POST", "/api/curator/run?profile=fixture", |_| {
            (200, json!({"ok":true,"name":"curator-run","pid":4242}))
        })
        .with_route("GET", "/api/actions/curator-run/status?lines=40", |_| {
            (200, fixture("action-curator-run.json"))
        })
}

#[tokio::test]
async fn routes_and_bodies_are_exact() {
    let mock = routes().await;
    let rest = client(&mock);
    memory_status(&rest).await.unwrap();
    set_memory_provider(&rest, "").await.unwrap();
    reset_memory(&rest, "fixture", ResetTarget::User)
        .await
        .unwrap();
    provider_config(&rest, "retaindb").await.unwrap();
    let values = json!({"project":"test"}).as_object().unwrap().clone();
    save_provider_config(&rest, "retaindb", ConfigSurface::Plain, &values)
        .await
        .unwrap();
    curator_status(&rest).await.unwrap();
    set_curator_paused(&rest, true).await.unwrap();
    run_curator(&rest, "fixture").await.unwrap();
    action_status(&rest, "curator-run", 40).await.unwrap();
    let requests = mock.requests().await;
    assert_eq!(
        requests
            .iter()
            .map(|r| (r.method.as_str(), r.path.as_str()))
            .collect::<Vec<_>>(),
        [
            ("GET", "/api/memory"),
            ("PUT", "/api/memory/provider"),
            ("POST", "/api/memory/reset?profile=fixture"),
            (
                "GET",
                "/api/memory/providers/retaindb/config?surface=declared"
            ),
            ("GET", "/api/memory/providers/retaindb/config"),
            ("PUT", "/api/memory/providers/retaindb/config"),
            ("GET", "/api/curator"),
            ("PUT", "/api/curator/paused"),
            ("POST", "/api/curator/run?profile=fixture"),
            ("GET", "/api/actions/curator-run/status?lines=40")
        ]
    );
    for (i, expected) in [
        (1, json!({"provider":""})),
        (2, json!({"target":"user"})),
        (5, json!({"values":values})),
        (7, json!({"paused":true})),
        (8, json!({})),
    ] {
        assert_eq!(
            serde_json::from_slice::<Value>(&requests[i].body).unwrap(),
            expected
        );
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(32))]
    #[test]
    fn destructive_routes_always_name_the_profile(profile in "[a-z0-9][a-z0-9_-]{0,63}") {
        let runtime = tokio::runtime::Runtime::new().unwrap(); runtime.block_on(async {
            let reset = format!("/api/memory/reset?profile={profile}"); let run = format!("/api/curator/run?profile={profile}");
            let mock = MockRest::start().await.with_route("POST",&reset, |_| (200,json!({"ok":true,"deleted":[]}))).with_route("POST",&run, |_| (200,json!({"ok":true,"name":"curator-run","pid":1})));
            let rest = client(&mock); reset_memory(&rest,&profile,ResetTarget::All).await.unwrap(); run_curator(&rest,&profile).await.unwrap();
            let requests = mock.requests().await; assert_eq!(requests[0].path,reset); assert_eq!(requests[1].path,run);
        });
    }
}

#[tokio::test]
async fn unsafe_provider_names_are_never_sent() {
    let mock = routes().await;
    let rest = client(&mock);
    assert!(matches!(
        reset_memory(&rest, "", ResetTarget::All).await,
        Err(MemoryApiError::MissingProfile)
    ));
    assert!(matches!(
        run_curator(&rest, "").await,
        Err(MemoryApiError::MissingProfile)
    ));
    for name in ["", "../x", "a/b", ".x", &"a".repeat(65), "name with space"] {
        assert!(matches!(
            provider_config(&rest, name).await,
            Err(MemoryApiError::BadProviderName)
        ));
        assert!(matches!(
            save_provider_config(&rest, name, ConfigSurface::Plain, &Default::default()).await,
            Err(MemoryApiError::BadProviderName)
        ));
        if !name.is_empty() {
            assert!(matches!(
                set_memory_provider(&rest, name).await,
                Err(MemoryApiError::BadProviderName)
            ));
        }
    }
    assert!(mock.requests().await.is_empty());
    set_memory_provider(&rest, "").await.unwrap();
    assert_eq!(
        serde_json::from_slice::<Value>(&mock.requests().await[0].body).unwrap(),
        json!({"provider":""})
    );
}

#[tokio::test]
async fn settings_fall_back_to_plain_surface() {
    let declared = MockRest::start().await.with_route(
        "GET",
        "/api/memory/providers/sampleprov/config?surface=declared",
        |_| (200, fixture("declared-sample.json")),
    );
    assert_eq!(
        provider_config(&client(&declared), "sampleprov")
            .await
            .unwrap()
            .surface,
        ConfigSurface::Declared
    );
    assert_eq!(declared.requests().await.len(), 1);
    let fallback = routes().await;
    assert_eq!(
        provider_config(&client(&fallback), "retaindb")
            .await
            .unwrap()
            .surface,
        ConfigSurface::Plain
    );
    assert_eq!(fallback.requests().await.len(), 2);
    let missing = MockRest::start().await.with_route(
        "GET",
        "/api/memory/providers/retaindb/config?surface=declared",
        |_| (200, fixture("declared-empty.json")),
    );
    assert!(matches!(
        provider_config(&client(&missing), "retaindb").await,
        Err(MemoryApiError::Rest(RestError::NotFound { .. }))
    ));
}

#[tokio::test(start_paused = true)]
async fn deadlines_per_route() {
    let mock = routes().await;
    let rest = client(&mock);
    mock.delay("GET", "/api/memory", Duration::from_secs(44))
        .await;
    assert!(memory_status(&rest).await.is_ok());
    mock.delay("GET", "/api/memory", Duration::from_secs(46))
        .await;
    assert!(memory_status(&rest).await.is_err());
    mock.delay(
        "GET",
        "/api/memory/providers/retaindb/config",
        Duration::from_secs(20),
    )
    .await;
    assert!(provider_config(&rest, "retaindb").await.is_ok());
    mock.delay(
        "GET",
        "/api/actions/curator-run/status?lines=40",
        Duration::from_secs(6),
    )
    .await;
    assert!(action_status(&rest, "curator-run", 40).await.is_err());
}
