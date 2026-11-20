//! Open server-request bookkeeping stays bounded, and a refusal is one JSON-RPC frame.

use std::time::Duration;

use hermes_client::{Client, Inbound, InboundStream, RespondError, ServerRequest};
use serde_json::{Value, json};

use crate::support::peer::Peer;

const OPEN_CAP: usize = 64;

async fn guard<T>(work: impl std::future::Future<Output = T>) -> T {
    match tokio::time::timeout(Duration::from_secs(8), work).await {
        Ok(value) => value,
        Err(_) => panic!("8s guard"),
    }
}

async fn drain_server_requests(inbound: &mut InboundStream) -> Vec<ServerRequest> {
    let mut delivered = Vec::new();
    loop {
        match tokio::time::timeout(Duration::from_millis(400), inbound.next()).await {
            Ok(Some(Inbound::ServerRequest(request))) => delivered.push(request),
            Ok(Some(other)) => panic!("unexpected inbound {}", kind(other)),
            Ok(None) | Err(_) => break,
        }
    }
    delivered
}

fn kind(item: Inbound) -> &'static str {
    match item {
        Inbound::Event(_) => "event",
        Inbound::ServerRequest(_) => "server request",
        Inbound::Malformed(_) => "malformed",
        Inbound::Closed(_) => "closed",
    }
}

async fn read_json_until_quiet(peer: &mut Peer) -> Vec<Value> {
    let mut frames = Vec::new();
    while let Ok(value) = tokio::time::timeout(Duration::from_millis(200), peer.recv_json()).await {
        frames.push(value);
    }
    frames
}

#[tokio::test]
async fn open_server_request_flood_stays_bounded_and_never_evicts() {
    guard(async {
        let (parts, mut peer) = Peer::pair().await;
        let (client, mut inbound) = Client::start(parts);
        let mut ids = Vec::new();
        for _ in 0..70 {
            ids.push(
                peer.server_request("sudo", "s1", json!({"command": "true"}))
                    .await,
            );
        }
        let delivered = drain_server_requests(&mut inbound).await;
        assert_eq!(
            delivered.len(),
            OPEN_CAP,
            "open server requests grew past {OPEN_CAP}"
        );
        assert_eq!(
            delivered[0].id, ids[0],
            "the first open request was evicted"
        );
        assert!(
            delivered
                .iter()
                .all(|request| ids[..OPEN_CAP].contains(&request.id)),
            "a later request replaced an earlier open one"
        );
        assert_eq!(client.stats().await.open_server_requests, OPEN_CAP);

        let refusals = read_json_until_quiet(&mut peer).await;
        assert_eq!(refusals.len(), 70 - OPEN_CAP);
        for (frame, id) in refusals.iter().zip(ids[OPEN_CAP..].iter()) {
            assert_eq!(frame["jsonrpc"], "2.0");
            assert_eq!(frame["id"], id.as_str());
            assert!(frame.get("result").is_none());
            assert!(frame.get("method").is_none());
            assert_eq!(frame["error"]["code"], -32000);
            assert_eq!(frame["error"]["message"], "too many open server requests");
        }

        delivered[0]
            .responder
            .respond(&json!({"value": "kept"}))
            .await
            .expect("the first open request is still answerable");
        let answer = peer.recv_json().await;
        assert_eq!(answer["id"], ids[0]);
        assert_eq!(
            answer.pointer("/result/value").and_then(Value::as_str),
            Some("kept")
        );
        assert!(answer.get("error").is_none());

        let again = peer
            .server_request("sudo", "s1", json!({"command": "true"}))
            .await;
        let accepted = tokio::time::timeout(Duration::from_secs(2), inbound.next())
            .await
            .expect("answering one request frees a slot")
            .expect("stream open");
        let Inbound::ServerRequest(request) = accepted else {
            panic!("the freed slot did not deliver a server request");
        };
        assert_eq!(request.id, again);
        assert_eq!(client.stats().await.open_server_requests, OPEN_CAP);
    })
    .await;
}

#[tokio::test]
async fn duplicate_open_server_request_cancels_the_original_slot() {
    guard(async {
        let (parts, peer) = Peer::pair().await;
        let (_client, mut inbound) = Client::start(parts);
        let id = peer
            .server_request("sudo", "s1", json!({"command": "true"}))
            .await;
        let request = match inbound.next().await {
            Some(Inbound::ServerRequest(request)) => request,
            _ => panic!("the first request was not delivered"),
        };
        let before = peer.json_frames();
        peer.send_text(
            &json!({
                "jsonrpc": "2.0",
                "id": id,
                "method": "sudo",
                "params": {"session_id": "s1", "command": "other"}
            })
            .to_string(),
        )
        .await;
        assert!(matches!(inbound.next().await, Some(Inbound::Malformed(_))));
        assert_eq!(peer.json_frames(), before, "a duplicate id wrote a frame");
        assert!(matches!(
            request.responder.respond(&json!({"value": "later"})).await,
            Err(RespondError::Cancelled)
        ));
        assert_eq!(peer.json_frames(), before);
    })
    .await;
}

#[tokio::test]
async fn unknown_method_echo_is_truncated_on_the_wire() {
    guard(async {
        let (parts, mut peer) = Peer::pair().await;
        let (_client, mut inbound) = Client::start(parts);
        let method = format!("custom.{}", "m".repeat(200));
        let id = peer.server_request(&method, "s1", json!({})).await;
        let frame = peer.recv_json().await;
        assert_eq!(frame["id"].as_str(), Some(id.as_str()));
        assert_eq!(frame["error"]["code"], -32601);
        let message = frame["error"]["message"].as_str().expect("message");
        let echoed = message
            .strip_prefix("no handler for server request: ")
            .expect("prefix");
        assert_eq!(echoed.chars().count(), 128);
        assert!(method.starts_with(echoed));
        assert!(!message.contains(&"m".repeat(180)));
        match inbound.next().await {
            Some(Inbound::Malformed(_)) => {}
            _ => panic!("the unknown method was not reported"),
        }
    })
    .await;
}
