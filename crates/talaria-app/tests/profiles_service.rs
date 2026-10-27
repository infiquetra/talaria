//! The profiles service: wire shapes, errors, topology and safe discovery. Issue #253.

#[path = "../../hermes-client/tests/support/mod.rs"]
mod client_support;
#[path = "support/mock_gateway.rs"]
mod mock_gateway;

use std::collections::BTreeSet;
use std::sync::Arc;

use client_support::mock_rest::{CountingMinter, MockRest};
use client_support::{ControlReply, Home, WsPlan, serve};
use mock_gateway::MockGateway;
use serde_json::{Value, json};
use talaria_app::profiles::{ProfilesService, parse_topology};
use talaria_core::profiles::{CreateRequest, ProfilesEffect, ProfilesInput, Section};

/// The keys the profiles calls must never send with a value. The soul rides on
/// configure only; every other surface refuses it too.
const FORBIDDEN: [&str; 10] = [
    "profile",
    "clone_all",
    "clone_channels",
    "share_auth",
    "no_alias",
    "no_skills",
    "soul",
    "model",
    "provider",
    "unattended",
];

fn non_null_keys(params: &Value) -> BTreeSet<String> {
    params
        .as_object()
        .map(|object| {
            object
                .iter()
                .filter(|(_, value)| !value.is_null())
                .map(|(key, _)| key.clone())
                .collect()
        })
        .unwrap_or_default()
}

fn service_with(client: hermes_client::Client) -> (ProfilesService, tempfile::TempDir) {
    let home = tempfile::tempdir().expect("a home");
    let service = ProfilesService::new(client, home.path().to_path_buf(), "default".to_owned());
    (service, home)
}

fn roster_result() -> Value {
    json!({
        "profiles": [
            {"name": "default", "path": "<home>", "is_default": true, "model": "minimax-m3.1-flash",
             "provider": "ollama-cloud", "description": "", "display_name": "default",
             "skill_count": 44, "ui_meta_revisions": {}, "has_avatar": false},
            {"name": "fixture", "path": "<home>/profiles/<profile>", "is_default": false,
             "model": "", "provider": "", "description": "", "display_name": "fixture",
             "skill_count": 0},
        ],
        "bot_mode_protocol": true,
        "install_id": "[redacted]",
    })
}

fn describe_result() -> Value {
    json!({
        "name": "fixture",
        "description": "Throwaway profile",
        "soul": "You are a careful reviewer.\n",
        "model": {"provider": "ollama-cloud", "default": "minimax-m3.1-flash"},
        "skills": [{"name": "plan", "enabled": true}],
        "toolsets": [{"name": "terminal", "enabled": true, "label": "Terminal",
                      "description": "Run shell commands.", "tool_count": 3}],
        "toolsets_pinned": true,
        "mcp_servers": [{"name": "files", "enabled": false, "transport": "stdio"}],
    })
}

fn create_result() -> Value {
    json!({
        "ok": true, "name": "fixture-new", "path": "<home>/profiles/<profile>",
        "soul_written": false, "model_set": false,
        "mirrored": {"env": true, "auth": false, "model_inherited": true, "voice": false},
    })
}

#[tokio::test]
async fn wire_params_are_exact() {
    let (gateway, client, _inbound) = MockGateway::start().await;
    let (service, _home) = service_with(client);

    // The roster call sends exactly one key.
    gateway.push_result(roster_result());
    let input = service
        .execute(ProfilesEffect::List { generation: 7 })
        .await;
    assert!(matches!(input, Some(ProfilesInput::Listed(7, Ok(_)))));
    let lists = gateway.requests("profiles.list");
    assert_eq!(lists.len(), 1);
    assert_eq!(lists[0].params, json!({"include_sessions": false}));

    // Describe sends the name only.
    gateway.push_result(describe_result());
    let input = service
        .execute(ProfilesEffect::Describe {
            generation: 7,
            name: "fixture".to_owned(),
        })
        .await;
    assert!(matches!(input, Some(ProfilesInput::Described(7, name, Ok(_))) if name == "fixture"));
    let describes = gateway.requests("profiles.describe");
    assert_eq!(describes.len(), 1);
    assert_eq!(
        non_null_keys(&describes[0].params),
        BTreeSet::from(["name".to_owned()])
    );
    assert_eq!(describes[0].params["name"], "fixture");

    // Create sends the name, the mirror flag and the set optionals only.
    gateway.push_result(create_result());
    let input = service
        .execute(ProfilesEffect::Create(CreateRequest {
            name: "fixture-new".to_owned(),
            description: Some("Scratch work".to_owned()),
            clone_from: None,
            mirror_credentials: true,
        }))
        .await;
    assert!(matches!(input, Some(ProfilesInput::CreateAnswered(Ok(_)))));
    gateway.push_result(create_result());
    let input = service
        .execute(ProfilesEffect::Create(CreateRequest {
            name: "fixture-bare".to_owned(),
            description: None,
            clone_from: Some("fixture".to_owned()),
            mirror_credentials: false,
        }))
        .await;
    assert!(matches!(input, Some(ProfilesInput::CreateAnswered(Ok(_)))));
    let creates = gateway.requests("profiles.create");
    assert_eq!(creates.len(), 2);
    assert_eq!(
        non_null_keys(&creates[0].params),
        BTreeSet::from([
            "name".to_owned(),
            "mirror_credentials".to_owned(),
            "description".to_owned()
        ])
    );
    assert_eq!(creates[0].params["mirror_credentials"], true);
    assert_eq!(
        non_null_keys(&creates[1].params),
        BTreeSet::from([
            "name".to_owned(),
            "mirror_credentials".to_owned(),
            "clone_from".to_owned()
        ])
    );
    assert_eq!(creates[1].params["clone_from"], "fixture");

    // Configure sends one section per request.
    gateway.push_result(json!({"ok": true, "applied": {"description": true}}));
    let input = service
        .execute(ProfilesEffect::Configure {
            name: "fixture".to_owned(),
            section: Section::Description,
            value: "Throwaway profile".to_owned(),
        })
        .await;
    assert!(matches!(
        input,
        Some(ProfilesInput::ConfigureAnswered(_, _, Ok(_)))
    ));
    gateway.push_result(json!({"ok": true, "applied": {"soul": true}}));
    let input = service
        .execute(ProfilesEffect::Configure {
            name: "fixture".to_owned(),
            section: Section::Soul,
            value: "You are a careful reviewer.\n".to_owned(),
        })
        .await;
    assert!(matches!(
        input,
        Some(ProfilesInput::ConfigureAnswered(_, _, Ok(_)))
    ));
    let configures = gateway.requests("profiles.configure");
    assert_eq!(configures.len(), 2);
    assert_eq!(
        non_null_keys(&configures[0].params),
        BTreeSet::from(["name".to_owned(), "description".to_owned()])
    );
    assert_eq!(
        non_null_keys(&configures[1].params),
        BTreeSet::from(["name".to_owned(), "soul".to_owned()])
    );
    assert_eq!(
        configures[1].params["soul"],
        "You are a careful reviewer.\n"
    );

    // No forbidden key carries a value on any profiles call.
    for (method, seen) in [
        ("profiles.list", lists),
        ("profiles.describe", describes),
        ("profiles.create", creates),
        ("profiles.configure", configures),
    ] {
        for frame in seen {
            for key in FORBIDDEN {
                if key == "soul" && method == "profiles.configure" {
                    continue;
                }
                assert!(
                    frame.params.get(key).is_none_or(Value::is_null),
                    "{method} sends {key}: {}",
                    frame.params
                );
            }
        }
    }

    // The status read goes out once, with no ticket.
    let rest = MockRest::start().await;
    let home = Home::new();
    let root = home.path().to_string_lossy().into_owned();
    let identify = json!({
        "runtime_protocol": 1, "state": "ready",
        "capabilities": ["session-authority-v1"], "authority_epoch": 3,
        "instance_id": "0123456789abcdef0123456789abcdef",
        "parked_profiles": {},
        "api_origin": rest.origin(),
        "supervisor": "none",
        "served_profiles": [{"profile_id": root, "home": root}],
    });
    let reply = json!({"ok": true, "protocol": 1, "id": 1, "result": identify}).to_string();
    let _control = serve(
        &home.path().join("gateway.sock"),
        |_| vec![ControlReply::Line(reply)],
        WsPlan::Idle,
    )
    .await;
    let minter = Arc::new(CountingMinter::default());
    let client = hermes_client::rest::RestClient::new(
        &rest.origin(),
        minter.clone(),
        hermes_client::rest::RestOptions::default(),
    )
    .expect("a loopback origin");
    let _ = hermes_client::gateway_api::gateway_status(&client).await;
    let requests = rest.requests().await;
    assert_eq!(requests.len(), 1, "the status is read once");
    assert_eq!(requests[0].method, "GET");
    assert_eq!(requests[0].path, "/api/status");
    assert!(
        !requests[0]
            .headers
            .iter()
            .any(|(name, _)| name.eq_ignore_ascii_case("x-hermes-gateway-ticket")),
        "the public status path carries no ticket"
    );
    assert_eq!(minter.count(), 0);
}

#[tokio::test]
async fn errors_are_classified() {
    let (gateway, client, _inbound) = MockGateway::start().await;
    let (service, _home) = service_with(client);

    // Handler refusals keep their code, message and reason.
    for (method, effect, code, message) in [
        (
            "profiles.describe",
            ProfilesEffect::Describe {
                generation: 1,
                name: "fixture".to_owned(),
            },
            4064,
            "profile 'fixture' not found",
        ),
        (
            "profiles.create",
            ProfilesEffect::Create(CreateRequest {
                name: "fixture-new".to_owned(),
                description: None,
                clone_from: None,
                mirror_credentials: true,
            }),
            4062,
            "A profile named 'fixture-new' already exists.",
        ),
        (
            "profiles.configure",
            ProfilesEffect::Configure {
                name: "fixture".to_owned(),
                section: Section::Soul,
                value: "text".to_owned(),
            },
            5064,
            "the store is unavailable",
        ),
    ] {
        gateway.script_error(method, None, code, message);
        let input = service.execute(effect).await.expect("an answer");
        match input {
            ProfilesInput::Described(
                _,
                _,
                Err(talaria_core::profiles::CallFailure::Refused {
                    code: seen_code,
                    message: seen_message,
                    reason,
                }),
            )
            | ProfilesInput::CreateAnswered(Err(talaria_core::profiles::CallFailure::Refused {
                code: seen_code,
                message: seen_message,
                reason,
            }))
            | ProfilesInput::ConfigureAnswered(
                _,
                _,
                Err(talaria_core::profiles::CallFailure::Refused {
                    code: seen_code,
                    message: seen_message,
                    reason,
                }),
            ) => {
                assert_eq!(seen_code, code);
                assert_eq!(seen_message, message);
                assert_eq!(reason.as_deref(), Some(message));
            }
            other => panic!("a refusal classifies as refused: {other:?}"),
        }
    }

    // The authority's 4001 carries its reason word.
    gateway.script_error("profiles.list", None, 4001, "invalid_params");
    let input = service
        .execute(ProfilesEffect::List { generation: 2 })
        .await;
    match input {
        Some(ProfilesInput::Listed(2, Err(words))) => {
            assert_eq!(words, "4001 invalid_params");
        }
        other => panic!("a refused roster is an error: {other:?}"),
    }

    // An unanswered method is -32601.
    let input = service
        .execute(ProfilesEffect::Describe {
            generation: 3,
            name: "fixture".to_owned(),
        })
        .await;
    assert!(
        matches!(
            input,
            Some(ProfilesInput::Described(
                3,
                _,
                Err(talaria_core::profiles::CallFailure::Refused { code: -32601, .. })
            ))
        ),
        "the default mock answer is unknown_method: {input:?}"
    );

    // A dropped call is unknown, never refused and never silent.
    let (gateway, client, _inbound) = MockGateway::start().await;
    let (service, _home) = service_with(client);
    let service = Arc::new(service);
    let worker = tokio::spawn({
        let service = Arc::clone(&service);
        async move {
            service
                .execute(ProfilesEffect::List { generation: 4 })
                .await
        }
    });
    for _ in 0..200 {
        if !gateway.requests("profiles.list").is_empty() {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    assert_eq!(
        gateway.requests("profiles.list").len(),
        1,
        "the call was sent"
    );
    gateway.kill_connection().await;
    let input = worker.await.expect("the worker ends");
    match input {
        Some(ProfilesInput::Listed(4, Err(words))) => {
            assert_eq!(words, "no answer", "a drop is unknown");
        }
        other => panic!("a dropped roster is an error: {other:?}"),
    }
}

#[tokio::test]
async fn topology_is_read_and_failures_are_soft() {
    let home = tempfile::tempdir().expect("a home");
    let root = home.path().to_string_lossy().into_owned();

    // Four fields only; the reason is scrubbed.
    let topo = parse_topology(
        &json!({
            "profiles": ["default", "fixture"],
            "parked_profiles": ["fixture-c"],
            "gateway_mode": "multiplex",
            "multiplex_standalone_reason": null,
            "gateways": [
                {"profile": "default", "ports": {"http": 1}},
                {"profile": "fixture-d", "ports": {"http": 2}},
            ],
            "hermes_home": root,
            "pid": 4242,
        }),
        home.path(),
    )
    .expect("a full reply parses");
    assert_eq!(topo.stopped, ["fixture-c"]);
    assert_eq!(
        topo.own_gateway,
        Some(vec!["default".to_owned(), "fixture-d".to_owned()])
    );
    assert_eq!(topo.gateway_mode.as_deref(), Some("multiplex"));
    assert_eq!(topo.single_reason, None);

    let topo = parse_topology(
        &json!({
            "parked_profiles": [],
            "gateway_mode": "single",
            "multiplex_standalone_reason": format!("{root}/profiles/fixture/config blocks it"),
        }),
        home.path(),
    )
    .expect("a gated reply parses");
    assert_eq!(topo.own_gateway, None, "no gateways key reads as none");
    assert_eq!(
        topo.single_reason.as_deref(),
        Some("<home>/profiles/fixture/config blocks it")
    );

    // A stopped list of the wrong shape, or no object at all, fails the parse.
    assert!(parse_topology(&json!({"parked_profiles": {"fixture-c": "x"}}), home.path()).is_err());
    assert!(parse_topology(&json!([1, 2]), home.path()).is_err());
    assert!(parse_topology(&json!(null), home.path()).is_err());

    // A missing mode reads as none, never as a match.
    let topo = parse_topology(&json!({"parked_profiles": []}), home.path()).expect("a bare reply");
    assert_eq!(topo.gateway_mode, None);

    // Through the service: a full read keeps four fields with a scrubbed reason.
    let (gateway, client, _inbound) = MockGateway::start().await;
    let _ = &gateway;
    let rest = MockRest::start().await;
    rest.response_body(
        "GET",
        "/api/status",
        br#"{"parked_profiles":["fixture-c"],"gateway_mode":"multiplex","multiplex_standalone_reason":null,"gateways":[{"profile":"default"}]}"#.to_vec(),
    )
    .await;
    let home = Home::new();
    let root = home.path().to_string_lossy().into_owned();
    let identify = json!({
        "runtime_protocol": 1, "state": "ready",
        "capabilities": ["session-authority-v1"], "authority_epoch": 3,
        "instance_id": "0123456789abcdef0123456789abcdef",
        "parked_profiles": {},
        "api_origin": rest.origin(),
        "supervisor": "none",
        "served_profiles": [{"profile_id": root, "home": root}],
    });
    let reply = json!({"ok": true, "protocol": 1, "id": 1, "result": identify}).to_string();
    let _control = serve(
        &home.path().join("gateway.sock"),
        |_| vec![ControlReply::Line(reply)],
        WsPlan::Idle,
    )
    .await;
    let service = ProfilesService::new(client, home.path().to_path_buf(), "default".to_owned());
    let input = service
        .execute(ProfilesEffect::Topology { generation: 5 })
        .await;
    match input {
        Some(ProfilesInput::Topology(5, Ok(topo))) => {
            assert_eq!(topo.stopped, ["fixture-c"]);
            assert_eq!(topo.own_gateway, Some(vec!["default".to_owned()]));
        }
        other => panic!("a full topology reads: {other:?}"),
    }

    // A 404, a slow answer and a broken body are soft failures with their words.
    for (setup, words) in [
        ("missing", "not found"),
        ("slow", "no answer within 8 s"),
        ("broken", "unreadable reply"),
    ] {
        let rest = MockRest::start().await;
        match setup {
            "missing" => rest.fail_status("GET", "/api/status", 404, None).await,
            "slow" => {
                rest.delay("GET", "/api/status", std::time::Duration::from_secs(9))
                    .await;
            }
            _ => {
                rest.response_body("GET", "/api/status", b"not json".to_vec())
                    .await;
            }
        }
        let home = Home::new();
        let root = home.path().to_string_lossy().into_owned();
        let identify = json!({
            "runtime_protocol": 1, "state": "ready",
            "capabilities": ["session-authority-v1"], "authority_epoch": 3,
            "instance_id": "0123456789abcdef0123456789abcdef",
            "parked_profiles": {},
            "api_origin": rest.origin(),
            "supervisor": "none",
            "served_profiles": [{"profile_id": root, "home": root}],
        });
        let reply = json!({"ok": true, "protocol": 1, "id": 1, "result": identify}).to_string();
        let _control = serve(
            &home.path().join("gateway.sock"),
            |_| vec![ControlReply::Line(reply)],
            WsPlan::Idle,
        )
        .await;
        let (_gateway, client, _inbound) = MockGateway::start().await;
        let service = ProfilesService::new(client, home.path().to_path_buf(), "default".into());
        let input = service
            .execute(ProfilesEffect::Topology { generation: 6 })
            .await;
        match input {
            Some(ProfilesInput::Topology(6, Err(seen))) => {
                assert_eq!(seen, words, "{setup}");
            }
            other => panic!("a {setup} topology is a soft failure: {other:?}"),
        }
    }
}

/// Opening the screen starts no gateway, even with a `hermes` on the path.
///
/// The discovery runs in a child test process: the child inherits a `PATH`
/// whose first entry holds a recording `hermes`, so any ensure would run it.
#[tokio::test]
async fn opening_the_screen_never_starts_a_gateway() {
    if let Some(home) = std::env::var_os("TALARIA_PROFILES_NEVER_ENSURE_HOME") {
        let (_gateway, client, _inbound) = MockGateway::start().await;
        let service =
            ProfilesService::new(client, std::path::PathBuf::from(home), "default".to_owned());
        let input = service
            .execute(ProfilesEffect::Discover { generation: 0 })
            .await;
        assert!(
            matches!(input, Some(ProfilesInput::Discovered(0, Err(_)))),
            "an empty home discovers no gateway: {input:?}"
        );
        return;
    }

    let home = tempfile::tempdir().expect("an empty home");
    let bin = tempfile::tempdir().expect("a bin folder");
    let marker = bin.path().join("hermes-ran");
    let script = bin.path().join("hermes");
    std::fs::write(&script, format!("#!/bin/sh\ntouch {}\n", marker.display()))
        .expect("a recording hermes");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut permissions = std::fs::metadata(&script).expect("script").permissions();
        permissions.set_mode(0o755);
        std::fs::set_permissions(&script, permissions).expect("script mode");
    }
    let mut paths = vec![bin.path().to_path_buf()];
    if let Some(path) = std::env::var_os("PATH") {
        paths.extend(std::env::split_paths(&path));
    }
    let path = std::env::join_paths(paths).expect("PATH joins");
    let binary = std::env::current_exe().expect("the test binary");
    let child = std::process::Command::new(binary)
        .arg("opening_the_screen_never_starts_a_gateway")
        .arg("--exact")
        .env("TALARIA_PROFILES_NEVER_ENSURE_HOME", home.path())
        .env("PATH", path)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .expect("the child runs");
    let (sender, receiver) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let _ = sender.send(child.wait_with_output());
    });
    let output = receiver
        .recv_timeout(std::time::Duration::from_secs(30))
        .expect("the child ends")
        .expect("the child is read");
    assert!(
        output.status.success(),
        "the child discovers without ensuring: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(!marker.exists(), "the recording hermes never ran");
    assert!(
        std::fs::read_dir(home.path())
            .expect("read home")
            .next()
            .is_none(),
        "discovery wrote nothing"
    );
}
