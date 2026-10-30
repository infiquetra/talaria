//! Remote socket, routing, replay and recording tests.

// Avoid formatting token-bearing values when an assertion fails.
#![allow(clippy::err_expect, clippy::ok_expect)]

#[path = "support/mock_gated.rs"]
mod mock_gated;
#[path = "remote_round_trip.rs"]
mod remote_round_trip;

use hermes_client::liveness::{Dial, DialFailure, Link, LinkConfig, LinkEvent, LinkState};
use hermes_client::remote::{
    PROFILE_METHODS, RemoteConfig, RemoteDial, RemoteName, TokenStore, WsTicket, capture, connect,
    profile_injector, ws_ticket,
};
use hermes_client::rpc::CallOptions;
use mock_gated::{MockGateway, Options};
use serde_json::{Value, json};
use std::collections::BTreeSet;
use std::os::unix::fs::PermissionsExt;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use url::Url;

fn cfg(base: Url) -> RemoteConfig {
    RemoteConfig {
        name: RemoteName::for_test("work"),
        base_url: base,
        provider: Some("basic".into()),
    }
}
fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64
}
fn store(home: &std::path::Path, expiry: i64) -> TokenStore {
    let store = TokenStore::new(home, RemoteName::for_test("work"));
    let path = store.path();
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(&path,json!({"version":1,"access_token":"SENTINEL-access","refresh_token":"SENTINEL-refresh","expires_at":expiry,"provider":"basic","user_id":"fixture","saved_at":1}).to_string()).unwrap();
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600)).unwrap();
    store
}
async fn ticket(gateway: &MockGateway) -> WsTicket {
    let mut url = gateway.base();
    url.set_path("/hermes/api/auth/ws-ticket");
    let mut socket = tokio::net::TcpStream::connect((url.host_str().unwrap(), url.port().unwrap()))
        .await
        .unwrap();
    socket.write_all(b"POST /hermes/api/auth/ws-ticket HTTP/1.1\r\nHost: 127.0.0.1\r\nAuthorization: Bearer SENTINEL-access\r\nContent-Length: 0\r\nConnection: close\r\n\r\n").await.unwrap();
    let mut bytes = Vec::new();
    socket.read_to_end(&mut bytes).await.unwrap();
    let text = String::from_utf8(bytes).unwrap();
    let (headers, body) = text.split_once("\r\n\r\n").unwrap();
    assert!(headers.starts_with("HTTP/1.1 200"));
    let response: Value = serde_json::from_str(body).unwrap();
    WsTicket::for_test(response["ticket"].as_str().unwrap())
}
async fn authorize_code(gateway: &MockGateway) -> String {
    use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
    use sha2::{Digest, Sha256};
    let verifier = "SENTINEL-verifier";
    let challenge = URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()));
    let mut url = gateway.base();
    url.set_path("/hermes/auth/native/authorize");
    url.query_pairs_mut()
        .append_pair("code_challenge", &challenge)
        .append_pair("code_challenge_method", "S256")
        .append_pair("redirect_uri", "http://127.0.0.1:1/callback")
        .append_pair("state", "SENTINEL-state");
    let mut socket = tokio::net::TcpStream::connect((url.host_str().unwrap(), url.port().unwrap()))
        .await
        .unwrap();
    let path = format!("{}?{}", url.path(), url.query().unwrap());
    socket
        .write_all(
            format!("GET {path} HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n")
                .as_bytes(),
        )
        .await
        .unwrap();
    let mut raw = Vec::new();
    socket.read_to_end(&mut raw).await.unwrap();
    let text = String::from_utf8(raw).unwrap();
    let location = text
        .lines()
        .find_map(|line| line.strip_prefix("Location: "))
        .expect("redirect");
    Url::parse(location)
        .unwrap()
        .query_pairs()
        .find(|(key, _)| key == "code")
        .unwrap()
        .1
        .into_owned()
}

fn call_opts() -> CallOptions {
    CallOptions {
        timeout: Some(Duration::from_secs(3)),
    }
}
async fn wait_for<F: Fn() -> bool>(f: F) {
    tokio::time::timeout(Duration::from_secs(5), async {
        while !f() {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .expect("gateway condition");
}

#[tokio::test]
async fn ws_ticket_retries_once_after_refresh() {
    let home = tempfile::tempdir().unwrap();
    let store = store(home.path(), now() + 1000);
    let g = MockGateway::start(Options {
        ticket_unauthorized: 1,
        ..Options::default()
    })
    .await;
    assert!(
        ws_ticket::mint_ws_ticket(&cfg(g.base()), &store)
            .await
            .is_ok()
    );
    assert_eq!(g.counts().tickets, 2);
    assert_eq!(g.counts().refresh, 1);
    g.set_ticket_unauthorized(2);
    let error = ws_ticket::mint_ws_ticket(&cfg(g.base()), &store)
        .await
        .err()
        .expect("second refusal");
    assert!(store.path().exists());
    assert_eq!(
        error.to_string(),
        "the gateway refused the sign-in for work after a refresh."
    );
    assert_eq!(g.counts().tickets, 4);
    assert_eq!(g.counts().refresh, 2);
}

#[tokio::test]
async fn dial_uses_query_ticket_only() {
    let g = MockGateway::start(Options::default()).await;
    let one = ticket(&g).await;
    let replay = one.clone();
    let _conn = connect::connect_remote(&cfg(g.base()), one)
        .await
        .ok()
        .expect("upgrade");
    let c = g.counts();
    assert_eq!(c.upgrades, 1);
    assert!(
        c.log
            .iter()
            .any(|line| line == "GET /api/ws?ticket=[redacted]")
    );
    assert_eq!(c.protocols, ["hermes-gateway-v1"]);
    assert!(
        !c.upgrade_headers
            .iter()
            .any(|h| h == "origin" || h == "authorization")
    );
    assert!(
        !c.protocols
            .iter()
            .any(|p| p.starts_with("hermes-gateway-ticket."))
    );
    assert!(
        connect::connect_remote(&cfg(g.base()), replay)
            .await
            .is_err(),
        "ticket must be single use"
    );
}

#[tokio::test]
async fn dial_accepts_missing_subprotocol_echo() {
    let g = MockGateway::start(Options::default()).await;
    assert!(
        connect::connect_remote(&cfg(g.base()), ticket(&g).await)
            .await
            .is_ok()
    );
    g.set_echo(Some("hermes-gateway-v1"));
    assert!(
        connect::connect_remote(&cfg(g.base()), ticket(&g).await)
            .await
            .is_ok()
    );
    g.set_echo(Some("wrong-protocol"));
    assert!(matches!(
        connect::connect_remote(&cfg(g.base()), ticket(&g).await).await,
        Err(hermes_client::ConnectError::SubprotocolMismatch)
    ));
}

#[tokio::test]
async fn tls_connection_runs_through_client_and_link() {
    let g = MockGateway::start(Options {
        tls: true,
        ..Options::default()
    })
    .await;
    let home = tempfile::tempdir().unwrap();
    assert!(g.tls_files().join("root.der").exists());
    assert!(g.tls_files().join("server.der").exists());
    let root = std::fs::read(g.tls_files().join("root.der")).unwrap();
    let dial = RemoteDial::new(
        cfg(g.base()),
        store(home.path(), now() + 1000),
        "fixture".into(),
    )
    .with_root_certificate(&root);
    let (link, _events) = Link::start(dial, LinkConfig::default())
        .await
        .ok()
        .expect("trusted TLS link");
    let reply = link
        .client()
        .call_raw("runtime.describe", json!({}), call_opts())
        .await;
    assert!(reply.is_ok());
    link.close().await;
    let mut untrusted = RemoteDial::new(
        cfg(g.base()),
        store(home.path(), now() + 1000),
        "fixture".into(),
    );
    let error = untrusted
        .dial(1)
        .await
        .err()
        .expect("untrusted root refused");
    assert!(error.reason().to_ascii_lowercase().contains("certificate"));
}

#[tokio::test]
async fn upgrade_rejections_map_to_messages() {
    let g = MockGateway::start(Options::default()).await;
    for (code, expected) in [
        (
            403,
            "the gateway refused the WebSocket upgrade (expired ticket, host not accepted by dashboard.public_url, or chat disabled)",
        ),
        (404, "no /api/ws at"),
        (502, "the tunnel or gateway is unavailable (HTTP 502)"),
        (503, "the tunnel or gateway is unavailable (HTTP 503)"),
        (504, "the tunnel or gateway is unavailable (HTTP 504)"),
    ] {
        g.set_refuse(code);
        let result = connect::connect_remote(&cfg(g.base()), ticket(&g).await).await;
        assert!(
            matches!(result,Err(hermes_client::ConnectError::UpgradeRejected { status:Some(n) }) if n==code)
        );
        let message = connect::upgrade_error(&g.base(), code);
        assert!(message.contains(expected));
    }
}

#[tokio::test]
async fn unknown_identity_adopted_then_restart_detected() {
    use hermes_client::replay::{Cursor, GapReason, Out, ReplayTracker};
    let g = MockGateway::start(Options::default()).await;
    let home = tempfile::tempdir().unwrap();
    let mut dial = RemoteDial::new(
        cfg(g.base()),
        store(home.path(), now() + 1000),
        "fixture".into(),
    );
    let first = dial.dial(1).await.ok().expect("first dial");
    assert_eq!(first.parts.instance_id, "mock-instance-1");
    assert_eq!(
        first.parts.profile_id.as_ref().map(|p| p.as_ref()),
        Some("fixture")
    );
    let mut tracker = ReplayTracker::<()>::new(64);
    tracker.on_new_connection(&first.process_epoch);
    tracker.track(
        "session",
        Cursor {
            replay_epoch: first.process_epoch.clone(),
            last_sequence: 0,
        },
    );
    g.restart();
    let second = dial.dial(2).await.ok().expect("second dial");
    assert_eq!(second.parts.instance_id, "mock-instance-2");
    assert!(
        tracker
            .on_new_connection(&second.process_epoch)
            .iter()
            .any(|event| matches!(event,Out::Gap(gap) if gap.reason==GapReason::BackendRestarted))
    );
}

#[tokio::test]
async fn missing_session_authority_is_refused() {
    let g = MockGateway::start(Options {
        no_authority: true,
        ..Options::default()
    })
    .await;
    let home = tempfile::tempdir().unwrap();
    let mut dial = RemoteDial::new(
        cfg(g.base()),
        store(home.path(), now() + 1000),
        "fixture".into(),
    );
    let error = dial.dial(1).await.err().expect("authority required");
    assert!(matches!(error, DialFailure::GiveUp { .. }));
    assert_eq!(
        error.reason(),
        "the remote socket has no session authority; point the tunnel at the gateway's own listener"
    );
}

#[tokio::test]
async fn profile_injected_for_declared_methods_only() {
    use hermes_client::replay::Cursor;
    let g = MockGateway::start(Options {
        profiles: vec!["default".into(), "fixture".into()],
        ..Options::default()
    })
    .await;
    let home = tempfile::tempdir().unwrap();
    let dial = RemoteDial::new(
        cfg(g.base()),
        store(home.path(), now() + 1000),
        "fixture".into(),
    );
    let (link, _events) = Link::start(dial, LinkConfig::scaled_for_tests())
        .await
        .ok()
        .expect("remote link");
    let client = link.client();
    let created = client
        .call_raw(
            "session.create",
            json!({"request_id":"request-1","source":"oneshot"}),
            call_opts(),
        )
        .await
        .unwrap();
    let sid = created["session_id"].as_str().unwrap();
    let _ = client
        .call_raw(
            "prompt.submit",
            json!({"session_id":sid,"text":"hello","submission_id":"submission-1","finite":true}),
            call_opts(),
        )
        .await
        .unwrap();
    let _ = client
        .call_raw("session.resume", json!({"session_id":sid}), call_opts())
        .await
        .unwrap();
    let _ = client
        .call_raw("gateway.ping", json!({}), call_opts())
        .await
        .unwrap();
    link.track(
        sid,
        Cursor {
            replay_epoch: "mock-epoch-1".into(),
            last_sequence: 0,
        },
    );
    tokio::time::sleep(Duration::from_millis(20)).await;
    g.drop_connections();
    wait_for(|| {
        g.counts()
            .per_profile
            .get("fixture")
            .is_some_and(|rows| rows.iter().any(|m| m == "session.events.since"))
    })
    .await;
    let counts = g.counts();
    let fixture = counts.per_profile.get("fixture").unwrap();
    for name in [
        "session.create",
        "prompt.submit",
        "session.resume",
        "session.events.since",
    ] {
        assert!(
            fixture.iter().any(|m| m == name),
            "missing routed method {name}"
        );
    }
    let default = counts
        .per_profile
        .get("default")
        .cloned()
        .unwrap_or_default();
    assert!(!default.iter().any(|m| {
        [
            "session.create",
            "prompt.submit",
            "session.resume",
            "session.events.since",
        ]
        .contains(&m.as_str())
    }));
    assert!(default.iter().any(|m| m == "runtime.describe"));
    assert!(default.iter().any(|m| m == "gateway.ping"));
    assert!(
        !fixture
            .iter()
            .any(|m| m == "runtime.describe" || m == "gateway.ping")
    );
    let mut explicit = json!({"profile":"default"});
    profile_injector("fixture".into())("session.create", &mut explicit);
    assert_eq!(explicit["profile"], "default");
    let mut params = json!({});
    profile_injector("fixture".into())("runtime.describe", &mut params);
    assert!(params.get("profile").is_none());
    profile_injector("fixture".into())("gateway.ping", &mut params);
    assert!(params.get("profile").is_none());
    link.close().await;
}

#[test]
fn profile_methods_match_vendored_contract() {
    fn has_profile(root: &Value, schema: &Value, depth: usize) -> bool {
        if depth > 32 {
            return false;
        }
        if schema
            .get("properties")
            .and_then(|p| p.get("profile"))
            .is_some()
        {
            return true;
        }
        if let Some(reference) = schema.get("$ref").and_then(Value::as_str) {
            return reference
                .strip_prefix('#')
                .and_then(|pointer| root.pointer(pointer))
                .is_some_and(|target| has_profile(root, target, depth + 1));
        }
        ["allOf", "anyOf", "oneOf"].iter().any(|key| {
            schema
                .get(*key)
                .and_then(Value::as_array)
                .is_some_and(|parts| parts.iter().any(|part| has_profile(root, part, depth + 1)))
        })
    }
    let root: Value = serde_json::from_str(include_str!(
        "../../hermes-contract/vendor/gateway-contract.openrpc.json"
    ))
    .unwrap();
    let mut expected = BTreeSet::new();
    for group in ["methods", "x-canonical-methods"] {
        for method in root[group].as_array().unwrap() {
            if method["params"]
                .as_array()
                .is_some_and(|params| params.iter().any(|p| has_profile(&root, &p["schema"], 0)))
            {
                expected.insert(method["name"].as_str().unwrap().to_owned());
            }
        }
    }
    assert!(!expected.is_empty());
    let actual: BTreeSet<_> = PROFILE_METHODS.iter().map(|s| (*s).to_owned()).collect();
    assert_eq!(actual, expected);
}

#[tokio::test]
async fn unserved_profile_fails_start() {
    let g = MockGateway::start(Options {
        profiles: vec!["default".into()],
        ..Options::default()
    })
    .await;
    let home = tempfile::tempdir().unwrap();
    let mut dial = RemoteDial::new(
        cfg(g.base()),
        store(home.path(), now() + 1000),
        "fixture".into(),
    );
    let error = dial.dial(1).await.err().expect("unserved profile");
    assert!(matches!(error, DialFailure::GiveUp { .. }));
    assert_eq!(
        error.reason(),
        "the remote gateway does not serve the profile fixture"
    );
}

#[tokio::test]
async fn reconnect_mints_fresh_ticket_each_attempt() {
    let g = MockGateway::start(Options::default()).await;
    let home = tempfile::tempdir().unwrap();
    let store = store(home.path(), now() + 1000);
    let dial = RemoteDial::new(cfg(g.base()), store.clone(), "fixture".into());
    let (link, mut events) = Link::start(dial, LinkConfig::scaled_for_tests())
        .await
        .ok()
        .expect("first link");
    let initial = g.counts().tickets;
    for n in 1..=3 {
        g.drop_connections();
        tokio::time::timeout(Duration::from_secs(5), async {
            loop {
                if let Some(LinkEvent::State(LinkState::Connected { generation })) =
                    events.recv().await
                    && generation == n as u64 + 1
                {
                    break;
                }
            }
        })
        .await
        .expect("reconnected");
        assert_eq!(g.counts().tickets, initial + n);
    }
    assert_eq!(g.counts().upgrades, initial + 3);
    let _ = std::fs::remove_file(store.path());
    let _ = self::store(home.path(), now() - 1);
    g.set_refresh_expired(true);
    g.drop_connections();
    let failed = tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            if let Some(LinkEvent::State(LinkState::Failed { reason, .. })) = events.recv().await {
                break reason;
            }
        }
    })
    .await
    .unwrap();
    assert!(failed.contains("expired"));
    tokio::time::sleep(Duration::from_millis(50)).await;
    assert_eq!(g.counts().refresh, 1);
    while let Ok(event) = events.try_recv() {
        assert!(
            !matches!(event, LinkEvent::State(LinkState::Failed { .. })),
            "terminal failure repeated"
        );
    }
    link.close().await;
}

#[tokio::test]
async fn recording_and_capture_hold_no_sentinel() {
    let root = tempfile::tempdir().unwrap();
    let output = root.path().join("recording.jsonl");
    let gateway = MockGateway::start(Options {
        auto_approve: true,
        ..Options::default()
    })
    .await;
    let store = store(root.path(), now() - 1);
    let recorder = hermes_client::Recorder::create(
        &output,
        hermes_client::RecorderOptions {
            endpoint: "wss://gateway.example.com/api/ws?ticket=SENTINEL-ticket".into(),
            ..Default::default()
        },
    )
    .unwrap();
    let config = cfg(gateway.base());
    let code = authorize_code(&gateway).await;
    let signed_in =
        hermes_client::remote::native_auth::exchange(&config, &code, "SENTINEL-verifier")
            .await
            .expect("exchange flow");
    store.save(&signed_in).expect("save sign-in");
    let _ = hermes_client::remote::native_auth::refresh(&config, &store)
        .await
        .expect("refresh flow");
    let dial = RemoteDial::new(config, store, "fixture".into()).with_recorder(recorder.clone());
    let (link, _events) = Link::start(dial, LinkConfig::default())
        .await
        .ok()
        .expect("recorded remote link");
    let client = link.client();
    let created = client
        .call_raw(
            "session.create",
            json!({"request_id":"recording-check","source":"oneshot"}),
            call_opts(),
        )
        .await
        .unwrap();
    let _=client.call_raw("prompt.submit",json!({"session_id":created["session_id"],"text":"hello","submission_id":"recording-1","finite":true}),call_opts()).await.unwrap();
    link.close().await;
    recorder.finish().unwrap();
    let recording = std::fs::read_to_string(output).unwrap();
    assert!(
        !recording.contains("SENTINEL-"),
        "recording leaked a sentinel"
    );
    assert!(!recording.contains("state=SENTINEL-"));
    let cases = [
        (
            "GET",
            "https://relay.example.com/auth/native/authorize?state=SENTINEL-state&code_challenge=SENTINEL-challenge",
            json!({}),
        ),
        (
            "POST",
            "https://gateway.example.com/auth/native/token",
            json!({"code":"SENTINEL-code","code_verifier":"SENTINEL-verifier","access_token":"SENTINEL-access","user_id":"SENTINEL-user","email":"SENTINEL-email","display_name":"SENTINEL-name"}),
        ),
        (
            "POST",
            "https://gateway.example.com/auth/native/refresh",
            json!({"refresh_token":"SENTINEL-refresh"}),
        ),
        (
            "POST",
            "https://gateway.example.com/api/auth/ws-ticket",
            json!({"ticket":"SENTINEL-ticket"}),
        ),
        (
            "GET",
            "wss://gateway.example.com/api/ws?ticket=SENTINEL-ticket",
            json!({}),
        ),
    ];
    for (method, url, body) in cases {
        let captured = capture::sanitize(
            method,
            url,
            &[
                ("Authorization".into(), "Bearer SENTINEL-access".into()),
                ("Cookie".into(), "SENTINEL-cookie".into()),
            ],
            &body,
        );
        let text = captured.to_string();
        assert!(!text.contains("SENTINEL-"), "capture leaked a sentinel");
        assert!(!text.contains("state=SENTINEL-"));
        assert!(
            !text.to_ascii_lowercase().contains("authorization")
                && !text.to_ascii_lowercase().contains("cookie")
        );
        if url.contains("relay.example.com") {
            assert!(!text.contains("relay.example.com") && text.contains("gateway.example.com"));
        }
    }
}

#[tokio::test]
async fn mock_request_log_hides_query_values() {
    let g = MockGateway::start(Options::default()).await;
    let one = ticket(&g).await;
    let _ = connect::connect_remote(&cfg(g.base()), one).await;
    let logs = g.counts().log.join("\n");
    assert!(logs.contains("/api/ws?ticket=[redacted]"));
    assert!(!logs.contains("SENTINEL-"), "mock log leaked a sentinel");
}

#[test]
fn turn_outcome_requires_status_complete() {
    let good = json!({"session_id":"session-1","admission_id":"admission-1","type":"message.complete","payload":{"status":"complete","text":"ok"}});
    assert!(remote_round_trip::matches_complete(
        &good,
        "session-1",
        "admission-1",
        "ok"
    ));
    for (key, value) in [
        ("status", json!("error")),
        ("status", json!("interrupted")),
        ("status", Value::Null),
        ("text", json!("wrong")),
    ] {
        let mut bad = good.clone();
        bad["payload"][key] = value;
        assert!(!remote_round_trip::matches_complete(
            &bad,
            "session-1",
            "admission-1",
            "ok"
        ));
    }
    let mut missing = good.clone();
    missing["payload"].as_object_mut().unwrap().remove("status");
    assert!(!remote_round_trip::matches_complete(
        &missing,
        "session-1",
        "admission-1",
        "ok"
    ));
    let mut error = good.clone();
    error["payload"]["error"] = json!("failure");
    assert!(!remote_round_trip::matches_complete(
        &error,
        "session-1",
        "admission-1",
        "ok"
    ));
    let mut other = good.clone();
    other["admission_id"] = json!("other");
    assert!(!remote_round_trip::matches_complete(
        &other,
        "session-1",
        "admission-1",
        "ok"
    ));
    let mut other = good;
    other["session_id"] = json!("other");
    assert!(!remote_round_trip::matches_complete(
        &other,
        "session-1",
        "admission-1",
        "ok"
    ));
}
