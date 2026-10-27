//! Connectors service: socket parameters, replies, secrets, and a dropped write.

#![allow(clippy::too_many_lines)]

#[path = "support/mock_gateway.rs"]
mod mock_gateway;
#[path = "../../hermes-client/tests/support/mock_rest.rs"]
mod mock_rest;

use std::collections::BTreeMap;
use std::net::TcpListener;
use std::sync::Arc;
use std::time::Duration;

use hermes_client::record::RecorderOptions;
use hermes_client::rest::{RestClient, RestOptions};
use hermes_client::{Recorder, scan_literals};
use mock_gateway::MockGateway;
use mock_rest::{CountingMinter, MockRest};
use serde_json::json;
use talaria_app::connectors::{ConnectorsConsent, ConnectorsService};
use talaria_core::connectors::{
    AccountAddress, ConnectorsEffect, ConnectorsFailure, ConnectorsInput,
};
use zeroize::Zeroizing;

fn rest_at(origin: &str) -> RestClient {
    RestClient::new(
        origin,
        Arc::new(CountingMinter::default()),
        RestOptions::default(),
    )
    .expect("loopback client")
}

fn closed_origin() -> String {
    let listener = TcpListener::bind("127.0.0.1:0").expect("loopback");
    let address = listener.local_addr().expect("address");
    drop(listener);
    format!("http://{address}")
}

/// Pinned connect result: `seq` plus a target that carries `kind`, `action`, and `state`.
fn open_connect_result(deadline_at: f64, connect_url: &str) -> serde_json::Value {
    json!({
        "op_id": "op-test-1",
        "seq": 1,
        "deadline_at": deadline_at,
        "settled": false,
        "status": "initiated",
        "note": null,
        "targets": [{
            "name": "github",
            "kind": "connector",
            "action": "connect",
            "state": "initiated",
            "connect_url": connect_url
        }]
    })
}

#[tokio::test]
async fn socket_params_are_exact() {
    let server = MockRest::start().await;
    let (gateway, client, _inbound) = MockGateway::start().await;
    let consent = ConnectorsConsent::default();
    assert!(!consent.granted("fixture"));
    let service = ConnectorsService::new(
        client,
        rest_at(&server.origin()),
        "fixture".into(),
        consent.clone(),
    );
    let refused = service
        .execute(ConnectorsEffect::ListConnectors, None)
        .await;
    assert!(matches!(
        refused.first(),
        Some(ConnectorsInput::ListReply(Err(ConnectorsFailure::NotSent(reason))))
            if reason == "consent not given"
    ));
    assert!(gateway.requests("connectors.list").is_empty());

    consent.grant("fixture");
    let _ = service
        .execute(ConnectorsEffect::ListConnectors, None)
        .await;
    let seen = gateway.requests("connectors.list");
    assert_eq!(seen.len(), 1);
    assert_eq!(seen[0].params, json!({"owner": {"type": "account"}}));
    assert!(seen[0].params.get("profile").is_none());
    assert!(seen[0].params.get("session_id").is_none());

    let catalog = ConnectorsEffect::LoadCatalog;
    let _ = service.execute(catalog, None).await;
    assert_eq!(gateway.requests("connectors.catalog")[0].params, json!({}));
    let connect = ConnectorsEffect::Connect {
        slug: "github".into(),
        address: AccountAddress::Alias("work".into()),
    };
    let _ = service.execute(connect, None).await;
    assert_eq!(
        gateway.requests("connectors.connect")[0].params,
        json!({
            "owner": {"type": "account"},
            "connectors": ["github"],
            "alias": "work",
            "reconnect": false
        })
    );
}

#[tokio::test]
async fn replies_decode_v0224_shapes() {
    let server = MockRest::start().await;
    let (gateway, client, _inbound) = MockGateway::start().await;
    let consent = ConnectorsConsent::default();
    consent.grant("fixture");
    let service =
        ConnectorsService::new(client, rest_at(&server.origin()), "fixture".into(), consent);
    gateway.push_result(open_connect_result(
        1_800_000_300.9,
        "https://example.invalid/connect/test",
    ));
    let inputs = service
        .execute(
            ConnectorsEffect::Connect {
                slug: "github".into(),
                address: AccountAddress::Alias("work".into()),
            },
            None,
        )
        .await;
    let ConnectorsInput::ConnectReply(Ok(operation)) = &inputs[0] else {
        panic!("connect reply");
    };
    assert_eq!(operation.op_id, "op-test-1");
    assert!(operation.deadline <= std::time::Instant::now() + Duration::from_secs(300));
    let _ = gateway;
}

#[tokio::test]
async fn secrets_reach_only_their_frames() {
    let directory = tempfile::tempdir().expect("temp");
    let recording = directory.path().join("frames.jsonl");
    let recorder = Recorder::create(&recording, RecorderOptions::default()).expect("recorder");
    let (gateway, client, _inbound) =
        MockGateway::start_with_recorder(mock_gateway::SESSION_ID, Some(recorder)).await;
    let server = MockRest::start()
        .await
        .allow_own_profile_selector("fixture");
    let consent = ConnectorsConsent::default();
    consent.grant("fixture");
    let service =
        ConnectorsService::new(client, rest_at(&server.origin()), "fixture".into(), consent);
    let mut secrets = BTreeMap::new();
    secrets.insert(
        "SIGNAL_HTTP_URL".into(),
        Zeroizing::new("sv-sentinel-7c1e".into()),
    );
    let _ = service
        .execute(
            ConnectorsEffect::UpdatePlatform {
                id: "signal".into(),
                enabled: None,
                typed_keys: vec!["SIGNAL_HTTP_URL".into()],
                lists: BTreeMap::new(),
                clear_env: Vec::new(),
            },
            Some(secrets),
        )
        .await;
    gateway.push_result(open_connect_result(
        1_800_000_300.0,
        "https://example.invalid/connect/ls-sentinel-3b9d",
    ));
    let _ = service
        .execute(
            ConnectorsEffect::Connect {
                slug: "github".into(),
                address: AccountAddress::Alias("work".into()),
            },
            None,
        )
        .await;
    let put = server
        .requests()
        .await
        .into_iter()
        .find(|request| request.method == "PUT")
        .expect("put");
    let body = String::from_utf8(put.body).expect("utf8");
    assert!(body.contains("sv-sentinel-7c1e"));
    let recorded = std::fs::read_to_string(&recording).unwrap_or_default();
    let forbidden = ["sv-sentinel-7c1e".to_owned(), "ls-sentinel-3b9d".to_owned()];
    assert!(scan_literals(&recorded, &forbidden).is_empty());
    assert!(recorded.contains("[redacted]"));
    assert!(!format!("{:?}", service_debug_inputs()).contains("sv-sentinel-7c1e"));
}

fn service_debug_inputs() -> ConnectorsInput {
    ConnectorsInput::SecretEdited {
        key: "SIGNAL_HTTP_URL".into(),
        empty: false,
    }
}

#[tokio::test]
async fn dropped_write_is_unknown() {
    let effects = [
        ConnectorsEffect::Connect {
            slug: "github".into(),
            address: AccountAddress::Alias("work".into()),
        },
        ConnectorsEffect::RenameAccount {
            connection_id: "conn-test-1".into(),
            alias: "work".into(),
        },
        ConnectorsEffect::RemoveAccount {
            connection_id: "conn-test-1".into(),
        },
        ConnectorsEffect::SetConnector {
            slug: "github".into(),
            enabled: false,
            expected_revision: "01J00000000000000000000000".into(),
        },
        ConnectorsEffect::SetTools {
            slug: "github".into(),
            disabled_tools: vec!["issues".into()],
            expected_revision: "01J00000000000000000000000".into(),
        },
        ConnectorsEffect::UpdatePlatform {
            id: "signal".into(),
            enabled: Some(false),
            typed_keys: Vec::new(),
            lists: BTreeMap::new(),
            clear_env: Vec::new(),
        },
        ConnectorsEffect::ApprovePairing {
            platform: "telegram".into(),
            request_id: "req-test-1".into(),
        },
        ConnectorsEffect::RevokePairing {
            platform: "telegram".into(),
            user_id: "user-test-2".into(),
        },
    ];
    for effect in effects {
        let on_socket = socket_method(&effect);
        let on_rest = rest_write(&effect);
        assert!(
            on_socket.is_some() ^ on_rest.is_some(),
            "each write uses one transport"
        );
        let server = MockRest::start()
            .await
            .allow_own_profile_selector("fixture");
        if let Some((method, path)) = on_rest {
            server.drop_after_request(method, path).await;
        }
        let (gateway, client, _inbound) = MockGateway::start().await;
        let socket = on_socket.map(|_| client.clone());
        let consent = ConnectorsConsent::default();
        consent.grant("fixture");
        let service = ConnectorsService::new(
            client,
            rest_at(&server.origin()),
            "fixture".into(),
            consent.clone(),
        );
        let inputs = if let Some(method) = on_socket {
            drop_socket_after_send(&service, &gateway, method, effect.clone()).await
        } else {
            service.execute(effect.clone(), None).await
        };
        assert!(
            matches!(reply_failure(&effect, &inputs), ConnectorsFailure::Unknown),
            "maybe sent"
        );
        let not_sent = if let Some(socket) = socket {
            socket.close().await;
            service.execute(effect.clone(), None).await
        } else {
            let (_closed_gateway, closed_client, _closed_inbound) = MockGateway::start().await;
            let closed = ConnectorsService::new(
                closed_client,
                rest_at(&closed_origin()),
                "fixture".into(),
                consent,
            );
            closed.execute(effect.clone(), None).await
        };
        assert!(
            matches!(
                reply_failure(&effect, &not_sent),
                ConnectorsFailure::NotSent(_)
            ),
            "never sent"
        );
    }
}

fn socket_method(effect: &ConnectorsEffect) -> Option<&'static str> {
    match effect {
        ConnectorsEffect::Connect { .. } => Some("connectors.connect"),
        ConnectorsEffect::RenameAccount { .. } => Some("connectors.accounts.rename"),
        ConnectorsEffect::RemoveAccount { .. } => Some("connectors.accounts.remove"),
        ConnectorsEffect::SetConnector { .. } | ConnectorsEffect::SetTools { .. } => {
            Some("connectors.policy.set")
        }
        _ => None,
    }
}

fn rest_write(effect: &ConnectorsEffect) -> Option<(&'static str, &'static str)> {
    match effect {
        ConnectorsEffect::UpdatePlatform { .. } => {
            Some(("PUT", "/api/messaging/platforms/signal?profile=fixture"))
        }
        ConnectorsEffect::ApprovePairing { .. } => Some(("POST", "/api/pairing/approve")),
        ConnectorsEffect::RevokePairing { .. } => Some(("POST", "/api/pairing/revoke")),
        _ => None,
    }
}

fn reply_failure<'a>(
    effect: &ConnectorsEffect,
    inputs: &'a [ConnectorsInput],
) -> &'a ConnectorsFailure {
    match (effect, inputs.first()) {
        (ConnectorsEffect::Connect { .. }, Some(ConnectorsInput::ConnectReply(Err(failure)))) => {
            failure
        }
        (
            ConnectorsEffect::RenameAccount { .. },
            Some(ConnectorsInput::RenameReply(Err(failure))),
        ) => failure,
        (
            ConnectorsEffect::RemoveAccount { .. },
            Some(ConnectorsInput::RemoveReply(Err(failure))),
        ) => failure,
        (
            ConnectorsEffect::SetConnector { .. } | ConnectorsEffect::SetTools { .. },
            Some(ConnectorsInput::PolicySetReply(Err(failure))),
        ) => failure,
        (
            ConnectorsEffect::UpdatePlatform { .. },
            Some(ConnectorsInput::UpdateReply {
                reply: Err(failure),
                ..
            }),
        ) => failure,
        (
            ConnectorsEffect::ApprovePairing { .. },
            Some(ConnectorsInput::ApproveReply(Err(failure))),
        ) => failure,
        (
            ConnectorsEffect::RevokePairing { .. },
            Some(ConnectorsInput::RevokeReply(Err(failure))),
        ) => failure,
        _ => panic!("write reply"),
    }
}

/// Silence the method so the mock does not answer, then drop the socket once the frame is recorded.
async fn drop_socket_after_send(
    service: &ConnectorsService,
    gateway: &MockGateway,
    method: &str,
    effect: ConnectorsEffect,
) -> Vec<ConnectorsInput> {
    gateway.set_silent(method, true);
    let execute = service.execute(effect, None);
    tokio::pin!(execute);
    tokio::select! {
        biased;
        inputs = &mut execute => inputs,
        () = until_seen(gateway, method) => {
            gateway.kill_connection().await;
            execute.await
        }
    }
}

async fn until_seen(gateway: &MockGateway, method: &str) {
    while gateway.requests(method).is_empty() {
        tokio::task::yield_now().await;
    }
}
