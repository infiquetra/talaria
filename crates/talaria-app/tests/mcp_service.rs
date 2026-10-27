#[path = "support/mock_gateway.rs"]
mod mock_gateway;
#[path = "../../hermes-client/tests/support/mock_rest.rs"]
mod mock_rest;

use std::io::Write;
use std::sync::Arc;
use std::time::{Duration, Instant};

use hermes_client::{
    Recorder, RecorderOptions,
    rest::{RestClient, RestError, RestOptions},
};
use mock_gateway::MockGateway;
use mock_rest::{CountingMinter, MockRest};
use serde_json::{Value, json};
use talaria_app::mcp::{McpService, failure_from_rest, timeout_for};
use talaria_core::mcp::{
    self, Failure, McpEffect, McpInput, McpOpen, McpState, McpTab, SecretSlot,
};
use zeroize::Zeroizing;

fn fixture(name: &str) -> Value {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../talaria-core/tests/fixtures/mcp")
        .join(format!("{name}.json"));
    serde_json::from_slice(&std::fs::read(path).expect("MCP fixture")).expect("valid fixture")
}

fn rest_client(mock: &MockRest) -> RestClient {
    RestClient::new(
        &mock.origin(),
        Arc::new(CountingMinter::default()),
        RestOptions::default(),
    )
    .expect("mock loopback REST client")
}

async fn service() -> (
    MockGateway,
    MockRest,
    McpService,
    hermes_client::InboundStream,
) {
    let (mock, client, inbound) = MockGateway::start().await;
    let rest = MockRest::start().await;
    let service = McpService::new(client, rest_client(&rest), "fixture".to_owned());
    (mock, rest, service, inbound)
}

fn remote(name: &str) -> Value {
    let mut server = fixture("list")["servers"][0].clone();
    server["name"] = json!(name);
    server
}

fn config(value: Value) -> Option<serde_json::Map<String, Value>> {
    Some(value.as_object().expect("config object").clone())
}

struct WireCase {
    effect: McpEffect,
    method: &'static str,
    params: Value,
    timeout: u64,
    secret: Option<&'static str>,
    reply: &'static str,
}

#[tokio::test]
async fn socket_params_and_timeouts_are_exact() {
    let cases = vec![
        WireCase {
            effect: McpEffect::LoadCatalog,
            method: "mcp.catalog",
            params: json!({}),
            timeout: 15,
            secret: None,
            reply: "catalog",
        },
        WireCase {
            effect: McpEffect::LoadList,
            method: "mcp.servers.list",
            params: json!({}),
            timeout: 15,
            secret: None,
            reply: "list",
        },
        WireCase {
            effect: McpEffect::LoadStatus,
            method: "mcp.servers.status",
            params: json!({}),
            timeout: 15,
            secret: None,
            reply: "status",
        },
        WireCase {
            effect: McpEffect::AddServer {
                name: "deepwiki".into(),
                preset: Some("deepwiki".into()),
                config: None,
                token: false,
            },
            method: "mcp.servers.add",
            params: json!({"name":"deepwiki", "preset":"deepwiki"}),
            timeout: 30,
            secret: None,
            reply: "add",
        },
        WireCase {
            effect: McpEffect::AddServer {
                name: "my-remote".into(),
                preset: None,
                config: config(json!({"url":"https://example.invalid/mcp"})),
                token: true,
            },
            method: "mcp.servers.add",
            params: json!({"name":"my-remote", "config":{"url":"https://example.invalid/mcp"}, "bearer_token":"test-token"}),
            timeout: 30,
            secret: Some("test-token"),
            reply: "add",
        },
        WireCase {
            effect: McpEffect::AddServer {
                name: "my-oauth".into(),
                preset: None,
                config: config(json!({"url":"https://example.invalid/mcp", "auth":"oauth"})),
                token: false,
            },
            method: "mcp.servers.add",
            params: json!({"name":"my-oauth", "config":{"url":"https://example.invalid/mcp", "auth":"oauth"}}),
            timeout: 30,
            secret: None,
            reply: "add",
        },
        WireCase {
            effect: McpEffect::AddServer {
                name: "my-local".into(),
                preset: None,
                config: config(json!({"command":"npx", "args":["-y", "some-server"]})),
                token: false,
            },
            method: "mcp.servers.add",
            params: json!({"name":"my-local", "config":{"command":"npx", "args":["-y", "some-server"]}}),
            timeout: 30,
            secret: None,
            reply: "add",
        },
        WireCase {
            effect: McpEffect::SetKey {
                name: "my-remote".into(),
                env_var: None,
                slot: SecretSlot::KeyValue,
            },
            method: "mcp.servers.set_api_key",
            params: json!({"name":"my-remote", "value":"test-key"}),
            timeout: 30,
            secret: Some("test-key"),
            reply: "set-key",
        },
        WireCase {
            effect: McpEffect::SetKey {
                name: "my-local".into(),
                env_var: Some("SOME_TOKEN".into()),
                slot: SecretSlot::EnvValue(0),
            },
            method: "mcp.servers.set_api_key",
            params: json!({"name":"my-local", "value":"test-env", "env_var":"SOME_TOKEN"}),
            timeout: 30,
            secret: Some("test-env"),
            reply: "set-key",
        },
        WireCase {
            effect: McpEffect::Test {
                name: "deepwiki".into(),
            },
            method: "mcp.servers.test",
            params: json!({"name":"deepwiki"}),
            timeout: 60,
            secret: None,
            reply: "test",
        },
        WireCase {
            effect: McpEffect::Remove {
                name: "my-local".into(),
            },
            method: "mcp.servers.remove",
            params: json!({"name":"my-local"}),
            timeout: 30,
            secret: None,
            reply: "remove",
        },
        WireCase {
            effect: McpEffect::StartSignIn {
                name: "my-oauth".into(),
            },
            method: "mcp.servers.oauth.start",
            params: json!({"name":"my-oauth"}),
            timeout: 45,
            secret: None,
            reply: "oauth-start",
        },
        WireCase {
            effect: McpEffect::Poll {
                name: "my-oauth".into(),
                flow: "flow-example".into(),
            },
            method: "mcp.servers.oauth.poll",
            params: json!({"name":"my-oauth", "session_id":"flow-example"}),
            timeout: 10,
            secret: None,
            reply: "oauth-poll",
        },
        WireCase {
            effect: McpEffect::CancelSignIn {
                name: "my-oauth".into(),
                flow: "flow-example".into(),
            },
            method: "mcp.servers.oauth.cancel",
            params: json!({"name":"my-oauth", "session_id":"flow-example"}),
            timeout: 5,
            secret: None,
            reply: "oauth-cancel",
        },
        WireCase {
            effect: McpEffect::RelayCallback {
                name: "my-oauth".into(),
                flow: "flow-example".into(),
            },
            method: "mcp.servers.oauth.callback",
            params: json!({"name":"my-oauth", "session_id":"flow-example", "code":"test-code", "state":"test-state"}),
            timeout: 10,
            secret: Some("http://127.0.0.1:43123/callback?code=test-code&state=test-state"),
            reply: "oauth-callback",
        },
        WireCase {
            effect: McpEffect::RelayCallback {
                name: "my-oauth".into(),
                flow: "flow-example".into(),
            },
            method: "mcp.servers.oauth.callback",
            params: json!({"name":"my-oauth", "session_id":"flow-example", "state":"test-state", "error":"access_denied", "iss":"https://issuer.invalid"}),
            timeout: 10,
            secret: Some(
                "http://127.0.0.1:43123/callback?error=access_denied&state=test-state&iss=https%3A%2F%2Fissuer.invalid&extra=ignored#ignored",
            ),
            reply: "oauth-callback",
        },
        WireCase {
            effect: McpEffect::Reload {
                session_id: Some("foreground".into()),
                confirm: false,
            },
            method: "reload.mcp",
            params: json!({"session_id":"foreground"}),
            timeout: 120,
            secret: None,
            reply: "reload",
        },
        WireCase {
            effect: McpEffect::Reload {
                session_id: Some("foreground".into()),
                confirm: true,
            },
            method: "reload.mcp",
            params: json!({"session_id":"foreground", "confirm":true}),
            timeout: 120,
            secret: None,
            reply: "reload",
        },
        WireCase {
            effect: McpEffect::Reload {
                session_id: None,
                confirm: false,
            },
            method: "reload.mcp",
            params: json!({}),
            timeout: 120,
            secret: None,
            reply: "reload",
        },
        WireCase {
            effect: McpEffect::Reload {
                session_id: None,
                confirm: true,
            },
            method: "reload.mcp",
            params: json!({"confirm":true}),
            timeout: 120,
            secret: None,
            reply: "reload",
        },
    ];
    let (mock, _rest, service, _inbound) = service().await;
    let expected_frames = cases.len();
    for case in cases {
        assert_eq!(
            timeout_for(&case.effect),
            Some(Duration::from_secs(case.timeout)),
            "{} timeout",
            case.method
        );
        mock.push_result(fixture(case.reply));
        let before = mock.requests(case.method).len();
        service
            .execute(
                case.effect,
                case.secret.map(|text| Zeroizing::new(text.to_owned())),
            )
            .await;
        let frames = mock.requests(case.method);
        assert_eq!(
            frames.len(),
            before + 1,
            "{} must send exactly one request",
            case.method
        );
        assert_eq!(frames[before].params, case.params, "{} params", case.method);
    }
    assert_eq!(
        timeout_for(&McpEffect::SetEnabled {
            name: "deepwiki".into(),
            enabled: false
        }),
        None
    );
    assert_eq!(
        mock.mcp_reload_count(),
        0,
        "scripted replies have no reload side effect"
    );
    assert_eq!(
        mock.frame_count(),
        expected_frames,
        "no implicit extra calls"
    );
}

fn with_unknown(mut value: Value) -> Value {
    value
        .as_object_mut()
        .unwrap()
        .insert("future_field".into(), json!({"arbitrary":true}));
    if let Some(rows) = value.get_mut("servers").and_then(Value::as_array_mut) {
        for row in rows {
            row["future_row_field"] = json!(42);
        }
    }
    value
}

#[tokio::test]
async fn replies_decode_v0224_shapes() {
    let (mock, _rest, service, _inbound) = service().await;
    mock.push_result(with_unknown(fixture("list")));
    let input = service.execute(McpEffect::LoadList, None).await;
    let [McpInput::ListLoaded(Ok(rows))] = input.as_slice() else {
        panic!("list reply was not decoded");
    };
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].name, "deepwiki");
    assert!(rows[0].enabled);
    assert!(
        rows[0].command.is_none()
            && rows[0].auth.is_none()
            && rows[0].tools.is_none()
            && rows[0].plugin.is_none()
            && rows[0].oauth_tokens_present.is_none()
    );

    let mut nulls = fixture("list");
    nulls["servers"][0]["url"] = Value::Null;
    mock.push_result(with_unknown(nulls));
    let input = service.execute(McpEffect::LoadList, None).await;
    assert!(
        matches!(input.as_slice(), [McpInput::ListLoaded(Ok(rows))] if rows.len() == 1 && rows[0].url.is_none())
    );

    mock.push_result(with_unknown(fixture("status")));
    let input = service.execute(McpEffect::LoadStatus, None).await;
    let [McpInput::StatusLoaded(Ok(rows))] = input.as_slice() else {
        panic!("status reply was not decoded");
    };
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].name, "deepwiki");
    assert_eq!(rows[0].tools, 3);
    assert!(rows[0].connected && !rows[0].disabled && rows[0].plugin.is_none());

    mock.push_result(with_unknown(fixture("catalog")));
    let input = service.execute(McpEffect::LoadCatalog, None).await;
    let [McpInput::CatalogLoaded(Ok(rows))] = input.as_slice() else {
        panic!("catalogue reply was not decoded");
    };
    assert_eq!(rows.len(), 1);
    assert!(rows[0].connector_slug.is_none());
    assert!(rows[0].transport.starts_with("TransportSpec(type='http'"));

    mock.push_result(with_unknown(fixture("add")));
    let input = service
        .execute(
            McpEffect::AddServer {
                name: "deepwiki".into(),
                preset: Some("deepwiki".into()),
                config: None,
                token: false,
            },
            None,
        )
        .await;
    assert!(
        matches!(input.as_slice(), [McpInput::Added { name, result: Ok(row) }] if name == "deepwiki" && row.name == "deepwiki" && row.enabled)
    );

    mock.push_result(with_unknown(fixture("set-key")));
    let input = service
        .execute(
            McpEffect::SetKey {
                name: "deepwiki".into(),
                env_var: Some("EXAMPLE_KEY".into()),
                slot: SecretSlot::KeyValue,
            },
            Some(Zeroizing::new("test-key".into())),
        )
        .await;
    assert!(
        matches!(input.as_slice(), [McpInput::KeySaved { name, env_var: Some(variable), result: Ok(saved) }] if name == "deepwiki" && variable == "EXAMPLE_KEY" && saved == "EXAMPLE_KEY")
    );

    mock.push_result(with_unknown(fixture("test")));
    let input = service
        .execute(
            McpEffect::Test {
                name: "deepwiki".into(),
            },
            None,
        )
        .await;
    let [
        McpInput::Tested {
            name,
            result: Ok(result),
        },
    ] = input.as_slice()
    else {
        panic!("test reply was not decoded");
    };
    assert_eq!(name, "deepwiki");
    assert!(result.ok && !result.oauth_needed && result.oauth_tokens_present.is_none());
    assert_eq!(result.tools[0].name, "example");
    assert_eq!((result.prompts, result.resources), (Some(0), Some(0)));

    mock.push_result(json!({"ok":false,"error":"Sign in first","tools":[],"oauth_needed":true,"oauth_tokens_present":false,"future_field":null}));
    let input = service
        .execute(
            McpEffect::Test {
                name: "deepwiki".into(),
            },
            None,
        )
        .await;
    assert!(
        matches!(input.as_slice(), [McpInput::Tested { result: Ok(reply), .. }] if !reply.ok && reply.oauth_needed && reply.error.as_deref() == Some("Sign in first"))
    );

    mock.push_result(with_unknown(fixture("remove")));
    let input = service
        .execute(
            McpEffect::Remove {
                name: "deepwiki".into(),
            },
            None,
        )
        .await;
    assert!(
        matches!(input.as_slice(), [McpInput::Removed { name, result: Ok(()) }] if name == "deepwiki")
    );

    mock.push_result(with_unknown(fixture("oauth-start")));
    let input = service
        .execute(
            McpEffect::StartSignIn {
                name: "deepwiki".into(),
            },
            None,
        )
        .await;
    let expected_auth_url = fixture("oauth-start")["auth_url"]
        .as_str()
        .expect("oauth-start auth_url")
        .to_owned();
    assert!(
        matches!(input.as_slice(), [McpInput::SignInStarted { name, result: Ok((flow, url)) }] if name == "deepwiki" && flow == "flow-example" && url == &expected_auth_url)
    );
    for reply in [
        with_unknown(fixture("oauth-poll")),
        json!({"ok":true,"status":"approved","session_id":"flow-example","error_message":null,"auth_url":null,"tools":[{"name":"tool","description":"A tool"}],"future_field":true}),
        json!({"ok":true,"status":"error","error_message":"OAuth session not found or expired","future_field":true}),
    ] {
        let expected: hermes_contract::schemas::McpOauthPollResult =
            serde_json::from_value(reply.clone()).unwrap();
        mock.push_result(reply);
        let input = service
            .execute(
                McpEffect::Poll {
                    name: "deepwiki".into(),
                    flow: "flow-example".into(),
                },
                None,
            )
            .await;
        let [
            McpInput::Polled {
                name,
                result: Ok(actual),
            },
        ] = input.as_slice()
        else {
            panic!("poll reply was not decoded");
        };
        assert_eq!(name, "deepwiki");
        assert_eq!(actual, &expected);
    }

    mock.push_result(with_unknown(fixture("oauth-callback")));
    let input = service
        .execute(
            McpEffect::RelayCallback {
                name: "deepwiki".into(),
                flow: "flow-example".into(),
            },
            Some(Zeroizing::new(
                "http://127.0.0.1:43123/callback?code=test-code&state=test-state".into(),
            )),
        )
        .await;
    assert!(
        matches!(input.as_slice(), [McpInput::CallbackRelayed { name, result: Ok(()) }] if name == "deepwiki")
    );
    mock.push_result(json!({"ok":false,"error_message":"OAuth callback state mismatch"}));
    let input = service
        .execute(
            McpEffect::RelayCallback {
                name: "deepwiki".into(),
                flow: "flow-example".into(),
            },
            Some(Zeroizing::new(
                "http://127.0.0.1:43123/callback?code=test-code&state=test-state".into(),
            )),
        )
        .await;
    assert!(matches!(
        input.as_slice(),
        [McpInput::CallbackRelayed { result: Err(_), .. }]
    ));

    let mut cancel_start = with_unknown(fixture("oauth-start"));
    cancel_start["session_id"] = json!("flow-cancel");
    mock.push_result(cancel_start);
    let _ = service
        .execute(
            McpEffect::StartSignIn {
                name: "deepwiki".into(),
            },
            None,
        )
        .await;
    mock.push_result(with_unknown(fixture("oauth-cancel")));
    assert!(
        service
            .execute(
                McpEffect::CancelSignIn {
                    name: "deepwiki".into(),
                    flow: "flow-cancel".into()
                },
                None
            )
            .await
            .is_empty()
    );

    for reply in [
        with_unknown(fixture("reload")),
        json!({"status":"reloaded","message":"free text","session":"applied","session_servers":["deepwiki"],"unavailable":[],"loaded_rev":null,"future_field":true}),
    ] {
        let expected: hermes_contract::schemas::ReloadMcpResult =
            serde_json::from_value(reply.clone()).unwrap();
        mock.push_result(reply);
        let input = service
            .execute(
                McpEffect::Reload {
                    session_id: None,
                    confirm: false,
                },
                None,
            )
            .await;
        let [McpInput::Reloaded(Ok(actual))] = input.as_slice() else {
            panic!("reload reply was not decoded");
        };
        assert_eq!(actual, &expected);
    }

    mock.mcp_error(
        "mcp.servers.test",
        json!({"code":4064,"message":"server 'gone' not found"}),
    );
    let input = service
        .execute(
            McpEffect::Test {
                name: "gone".into(),
            },
            None,
        )
        .await;
    assert!(
        matches!(input.as_slice(), [McpInput::Tested { result: Err(Failure::Rpc { code: 4064, message, reason: None, detail: None }), .. }] if message == "server 'gone' not found")
    );
    mock.mcp_error("reload.mcp", json!({"code":4001,"message":"mcp_servers_unavailable","data":{"reason":"mcp_servers_unavailable","field":"mcp_servers","detail":"did not connect: gone"}}));
    let input = service
        .execute(
            McpEffect::Reload {
                session_id: None,
                confirm: true,
            },
            None,
        )
        .await;
    assert!(
        matches!(input.as_slice(), [McpInput::Reloaded(Err(Failure::Rpc { code: 4001, reason: Some(reason), detail: Some(detail), .. }))] if reason == "mcp_servers_unavailable" && detail == "did not connect: gone")
    );
    let known = json!({"name":"known", "transport":"http", "tools":1, "connected":true, "disabled":false, "status":"connected", "source":"config", "plugin":null});
    let mut unknown = known.clone();
    unknown["name"] = json!("unknown");
    unknown["status"] = json!("starting_up");
    let raw = json!({"servers":[unknown,known], "checked_at":1760000000000_u64});
    let rows = talaria_app::mcp::decode_mcp_runtime_rows(&raw);
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].name, "known");
    mock.push_result(raw);
    let input = service.execute(McpEffect::LoadStatus, None).await;
    assert!(
        matches!(input.as_slice(), [McpInput::StatusLoaded(Ok(rows))] if rows.len() == 1 && rows[0].name == "known")
    );
    mock_gateway::check_mcp_peer_refusals();
}

async fn execute_observed(
    service: &McpService,
    effect: McpEffect,
    secret: Option<Zeroizing<String>>,
    state: &mut McpState,
    log: &mut Vec<u8>,
) {
    writeln!(log, "effect: {effect:?}").unwrap();
    let input = service.execute(effect, secret).await;
    for event in input {
        writeln!(log, "event/error: {event:?}").unwrap();
        let effects = mcp::update(state, event, Instant::now());
        writeln!(log, "returned effects: {effects:?}").unwrap();
    }
    let view = mcp::view(state, Instant::now());
    writeln!(
        log,
        "state: {state:?}\nview: {view:?}\nvisible text and notices: {:?}",
        view.text_lines()
    )
    .unwrap();
    let area = ratatui::layout::Rect::new(0, 0, 120, 40);
    let mut buffer = ratatui::buffer::Buffer::empty(area);
    talaria_tui::mcp::draw_mcp(&view, &|_| 16, area, &mut buffer);
    for cell in &buffer.content {
        write!(log, "{}", cell.symbol()).unwrap();
    }
    writeln!(log).unwrap();
}

#[tokio::test]
async fn secrets_reach_only_their_frames() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("mcp.jsonl");
    let recorder = Recorder::create(&path, RecorderOptions::default()).unwrap();
    let (mock, client, _inbound) =
        MockGateway::start_with_recorder("local-session-1", Some(recorder.clone())).await;
    let rest = MockRest::start().await;
    let service = McpService::new(client.clone(), rest_client(&rest), "fixture".into());
    let mut state = McpState::default();
    let mut log = Vec::new();
    let effects = mcp::update(
        &mut state,
        McpInput::Open {
            open: McpOpen::Tab(McpTab::Servers),
            profile_name: "fixture".into(),
            connected: true,
            over_ssh: true,
            foreground: None,
        },
        Instant::now(),
    );
    writeln!(&mut log, "opening effects: {effects:?}").unwrap();
    let token = "tk-sentinel-8b3f";
    let env = "ev-sentinel-9a2c";
    let key = "ky-sentinel-71a0";
    let code = "cd-sentinel-9ac2";
    let flow_state = "st-sentinel-129b";
    let pasted = format!("http://127.0.0.1:43123/callback?code={code}&state={flow_state}");

    execute_observed(
        &service,
        McpEffect::AddServer {
            name: "keyed".into(),
            preset: None,
            config: config(json!({"url":"https://example.invalid/mcp"})),
            token: true,
        },
        Some(Zeroizing::new(token.into())),
        &mut state,
        &mut log,
    )
    .await;
    execute_observed(
        &service,
        McpEffect::AddServer {
            name: "local".into(),
            preset: None,
            config: config(json!({"command":"npx","args":[]})),
            token: false,
        },
        None,
        &mut state,
        &mut log,
    )
    .await;
    execute_observed(
        &service,
        McpEffect::SetKey {
            name: "local".into(),
            env_var: Some("EXAMPLE_TOKEN".into()),
            slot: SecretSlot::EnvValue(0),
        },
        Some(Zeroizing::new(env.into())),
        &mut state,
        &mut log,
    )
    .await;
    execute_observed(
        &service,
        McpEffect::SetKey {
            name: "keyed".into(),
            env_var: None,
            slot: SecretSlot::KeyValue,
        },
        Some(Zeroizing::new(key.into())),
        &mut state,
        &mut log,
    )
    .await;
    let mut oauth = remote("oauth");
    oauth["auth"] = json!("oauth");
    mock.mcp_server(oauth);
    execute_observed(
        &service,
        McpEffect::StartSignIn {
            name: "oauth".into(),
        },
        None,
        &mut state,
        &mut log,
    )
    .await;
    execute_observed(
        &service,
        McpEffect::RelayCallback {
            name: "oauth".into(),
            flow: "flow-example".into(),
        },
        Some(Zeroizing::new(pasted.clone())),
        &mut state,
        &mut log,
    )
    .await;
    execute_observed(&service, McpEffect::LoadList, None, &mut state, &mut log).await;
    // Exercise an error's public text as well as successful replies. As in prompts_secrecy,
    // this sink captures public formatting; it does not intercept process-wide log events.
    execute_observed(
        &service,
        McpEffect::Test {
            name: "missing".into(),
        },
        None,
        &mut state,
        &mut log,
    )
    .await;

    let frames = mock.seen();
    for (needle, method, field) in [
        (token, "mcp.servers.add", "bearer_token"),
        (env, "mcp.servers.set_api_key", "value"),
        (key, "mcp.servers.set_api_key", "value"),
        (code, "mcp.servers.oauth.callback", "code"),
        (flow_state, "mcp.servers.oauth.callback", "state"),
    ] {
        let containing: Vec<_> = frames
            .iter()
            .filter(|frame| frame.params.to_string().contains(needle))
            .collect();
        assert_eq!(
            containing.len(),
            1,
            "a sentinel must occur in one request only"
        );
        assert_eq!(containing[0].method.as_deref(), Some(method));
        assert!(
            containing[0].params[field].as_str() == Some(needle),
            "sentinel in its dedicated field"
        );
    }
    assert!(
        !frames
            .iter()
            .any(|frame| frame.params.to_string().contains(&pasted)),
        "full pasted address is never sent"
    );
    let added = mock.requests("mcp.servers.add");
    for frame in added {
        assert!(
            !frame.params["config"].to_string().contains("sentinel-"),
            "configuration contains no secret"
        );
    }
    client.close().await;
    let summary = recorder.finish().unwrap();
    assert!(
        summary.frames >= frames.len() && summary.dropped == 0,
        "recording includes the actual service traffic"
    );
    let needles: Vec<String> = [token, env, key, code, flow_state, pasted.as_str()]
        .into_iter()
        .map(str::to_owned)
        .collect();
    for file in std::fs::read_dir(temp.path()).unwrap() {
        let path = file.unwrap().path();
        if path.is_file() {
            let recording = std::fs::read_to_string(path).unwrap();
            assert!(
                hermes_client::redact::scan_literals(&recording, &needles).is_empty(),
                "recorder directory leaked an MCP sentinel"
            );
        }
    }
    let logs = String::from_utf8(log).unwrap();
    assert!(
        logs.contains("effect:")
            && logs.contains("event/error:")
            && logs.contains("state:")
            && logs.contains("visible text and notices:")
    );
    assert!(
        !needles.iter().any(|needle| logs.contains(needle)),
        "public output or its formatted log sink leaked an MCP sentinel"
    );
}

#[tokio::test]
async fn bad_paste_sends_nothing() {
    let (mock, _rest, service, _inbound) = service().await;
    for address in [
        "not a URL",
        "https://127.0.0.1:43123/callback?code=x&state=y",
        "http://example.invalid/callback?code=x&state=y",
        "http://127.0.0.1:43123/wrong?code=x&state=y",
        "http://127.0.0.1:43123/callback?code=x",
        "http://127.0.0.1:43123/callback?state=y",
    ] {
        let before = mock.frame_count();
        let input = service
            .execute(
                McpEffect::RelayCallback {
                    name: "oauth".into(),
                    flow: "flow-example".into(),
                },
                Some(Zeroizing::new(address.into())),
            )
            .await;
        assert!(
            matches!(
                input.as_slice(),
                [McpInput::CallbackRelayed {
                    result: Err(Failure::BadPaste(_)),
                    ..
                }]
            ),
            "bad paste must return a local validation error"
        );
        tokio::task::yield_now().await;
        assert_eq!(
            mock.frame_count(),
            before,
            "invalid redirect must not send any frame"
        );
    }
}

#[tokio::test]
async fn dropped_write_is_unknown() {
    for (effect, method) in [
        (
            McpEffect::AddServer {
                name: "added".into(),
                preset: None,
                config: config(json!({"url":"https://example.invalid/mcp"})),
                token: false,
            },
            "mcp.servers.add",
        ),
        (
            McpEffect::Remove {
                name: "removed".into(),
            },
            "mcp.servers.remove",
        ),
        (
            McpEffect::Reload {
                session_id: Some("foreground".into()),
                confirm: true,
            },
            "reload.mcp",
        ),
    ] {
        let (mock, _rest, service, _inbound) = service().await;
        mock.drop_after(method);
        let input = tokio::time::timeout(Duration::from_secs(2), service.execute(effect, None))
            .await
            .expect("dropped socket must settle promptly");
        assert!(
            matches!(
                (method, input.as_slice()),
                (
                    "mcp.servers.add",
                    [McpInput::Added {
                        result: Err(Failure::Unknown),
                        ..
                    }]
                ) | (
                    "mcp.servers.remove",
                    [McpInput::Removed {
                        result: Err(Failure::Unknown),
                        ..
                    }]
                ) | ("reload.mcp", [McpInput::Reloaded(Err(Failure::Unknown))])
            ),
            "sent write lost its reply: outcome unknown"
        );
        tokio::time::sleep(Duration::from_millis(50)).await;
        assert_eq!(
            mock.requests(method).len(),
            1,
            "write must never be retried"
        );
        assert_eq!(
            mock.frame_count(),
            1,
            "service must not send a substitute write"
        );
    }
    assert!(matches!(
        failure_from_rest(&RestError::Dropped { sent: true }),
        Failure::Unknown
    ));
    assert!(matches!(
        failure_from_rest(&RestError::Dropped { sent: false }),
        Failure::NotSent(_)
    ));
    assert!(matches!(
        failure_from_rest(&RestError::Timeout { sent: true }),
        Failure::Unknown
    ));
}

#[tokio::test]
async fn closing_cancels_a_waiting_signin() {
    let (mock, _rest, service, _inbound) = service().await;
    let mut oauth = remote("oauth");
    oauth["auth"] = json!("oauth");
    mock.mcp_server(oauth);
    let input = service
        .execute(
            McpEffect::StartSignIn {
                name: "oauth".into(),
            },
            None,
        )
        .await;
    assert!(matches!(
        input.as_slice(),
        [McpInput::SignInStarted { result: Ok(_), .. }]
    ));
    mock.set_silent("mcp.servers.oauth.poll", true);
    let service = Arc::new(service);
    let waiting = {
        let service = service.clone();
        tokio::spawn(async move {
            service
                .execute(
                    McpEffect::Poll {
                        name: "oauth".into(),
                        flow: "flow-example".into(),
                    },
                    None,
                )
                .await
        })
    };
    mock.wait_for_request("mcp.servers.oauth.poll", 0).await;
    mock.set_silent("mcp.servers.oauth.cancel", true);
    // Leave room for scheduling around the service's one-second shutdown deadline.
    tokio::time::timeout(Duration::from_millis(1100), service.close())
        .await
        .expect("closing waits at most one second");
    let cancel = tokio::time::timeout(
        Duration::from_millis(100),
        mock.wait_for_request("mcp.servers.oauth.cancel", 0),
    )
    .await
    .expect("cancel must be sent during close");
    assert_eq!(
        cancel.params,
        json!({"name":"oauth", "session_id":"flow-example"})
    );
    service.close().await;
    service
        .execute(
            McpEffect::CancelSignIn {
                name: "oauth".into(),
                flow: "flow-example".into(),
            },
            None,
        )
        .await;
    tokio::task::yield_now().await;
    assert_eq!(
        mock.requests("mcp.servers.oauth.cancel").len(),
        1,
        "close and a late cancel share the one cancellation"
    );
    waiting.abort();

    // A close during the start request cannot know the flow id yet. The late
    // answer still has to be consumed and its newly known flow cancelled once.
    let (mock, _rest, service, _inbound) = self::service().await;
    let mut oauth = remote("late-oauth");
    oauth["auth"] = json!("oauth");
    mock.mcp_server(oauth);
    mock.hold("mcp.servers.oauth.start");
    let service = Arc::new(service);
    let starting = {
        let service = service.clone();
        tokio::spawn(async move {
            service
                .execute(
                    McpEffect::StartSignIn {
                        name: "late-oauth".into(),
                    },
                    None,
                )
                .await
        })
    };
    mock.wait_for_request("mcp.servers.oauth.start", 0).await;
    tokio::time::timeout(Duration::from_millis(1100), service.close())
        .await
        .expect("close must not wait indefinitely for a flow id");
    assert!(
        mock.requests("mcp.servers.oauth.cancel").is_empty(),
        "no cancellation may guess a flow id"
    );
    mock.release("mcp.servers.oauth.start");
    tokio::time::timeout(Duration::from_secs(1), starting)
        .await
        .expect("late start must settle")
        .unwrap();
    let cancelled = tokio::time::timeout(
        Duration::from_secs(1),
        mock.wait_for_request("mcp.servers.oauth.cancel", 0),
    )
    .await
    .expect("late flow must be cancelled");
    assert_eq!(
        cancelled.params,
        json!({"name":"late-oauth", "session_id":"flow-example"})
    );
    service.close().await;
    tokio::task::yield_now().await;
    assert_eq!(
        mock.requests("mcp.servers.oauth.cancel").len(),
        1,
        "late start and repeated close must share cancellation"
    );
}
