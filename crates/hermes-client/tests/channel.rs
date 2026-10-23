//! Channel behaviour against a loopback peer. Tests 9 to 31.

mod support;

use std::time::Duration;

use hermes_client::{
    CallError, CallOptions, Client, ClientClosed, CloseReason, ClosedConnection, Generation,
    Inbound, InboundEvent, InboundStream, MalformedFrame, MalformedKind, RespondError,
    ServerRequest,
};
use hermes_contract::events::{EventFrame, Notification};
use hermes_contract::methods::canonical::{approval_respond, runtime_describe, session_info};
use hermes_contract::schemas::{
    CanonicalApprovalParams, CanonicalSessionInfoParams, RuntimeDescribeParams,
};
use serde::Serialize;
use serde::ser::Error as _;
use serde_json::{Value, json};
use support::peer::{Peer, recorded_event, recorded_lines, recorded_result};

const SECRET: &str = "TALARIA-TEST-SECRET-ANSWER";

async fn guard<T>(work: impl std::future::Future<Output = T>) -> T {
    match tokio::time::timeout(Duration::from_secs(5), work).await {
        Ok(value) => value,
        Err(_) => panic!("5s guard"),
    }
}

#[tokio::test]
async fn concurrent_calls_with_out_of_order_replies() {
    guard(async {
        let (parts, mut peer) = Peer::pair().await;
        let (client, _inbound) = Client::start(parts);
        let methods = [
            ("runtime.describe", 101u64),
            ("session.create", 102),
            ("approval.respond", 105),
            ("session.detach", 106),
        ];
        let mut handles = Vec::new();
        for (method, _) in methods {
            let caller = client.clone();
            let method = method.to_string();
            handles.push(tokio::spawn(async move {
                caller
                    .call_raw(&method, json!({}), CallOptions::default())
                    .await
            }));
        }
        let mut requests = Vec::new();
        for _ in 0..4 {
            requests.push(peer.recv_json().await);
        }
        let mut ids = requests
            .iter()
            .map(|frame| frame["id"].as_u64().expect("integer id"))
            .collect::<Vec<_>>();
        ids.sort_unstable();
        assert_eq!(ids, vec![3, 4, 5, 6]);
        for method in [
            "approval.respond",
            "runtime.describe",
            "session.detach",
            "session.create",
        ] {
            let request = requests
                .iter()
                .find(|frame| frame["method"].as_str() == Some(method))
                .expect("request");
            let recorded = match method {
                "runtime.describe" => 101,
                "session.create" => 102,
                "approval.respond" => 105,
                "session.detach" => 106,
                _ => panic!("unexpected method"),
            };
            peer.reply_ok(request["id"].clone(), recorded_result(recorded))
                .await;
        }
        for ((method, recorded), handle) in methods.into_iter().zip(handles) {
            let result = match handle.await.expect("call task ended") {
                Ok(value) => value,
                Err(_) => panic!("{method} failed"),
            };
            assert_eq!(result, recorded_result(recorded));
        }
    })
    .await;
}

#[tokio::test]
async fn error_and_unknown_method_replies_surface_code_and_reason() {
    guard(async {
        let (parts, mut peer) = Peer::pair().await;
        let (client, _inbound) = Client::start(parts);
        let caller = client.clone();
        let info = tokio::spawn(async move {
            caller
                .call::<session_info::Marker>(CanonicalSessionInfoParams {
                    session_id: "0".repeat(32),
                })
                .await
        });
        let request = peer.recv_json().await;
        assert_eq!(request["method"].as_str(), Some("session.info"));
        peer.reply_err(request["id"].clone(), 4001, "not_found", Some("not_found"))
            .await;
        let err = call_err(info.await.expect("call task ended"));
        match &err {
            CallError::Rpc(rpc) => {
                assert_eq!(rpc.code, 4001);
                assert_eq!(rpc.reason.as_deref(), Some("not_found"));
            }
            _ => panic!("expected an rpc error"),
        }

        let caller = client.clone();
        let compress = tokio::spawn(async move {
            caller
                .call_raw("session.compress", json!({}), CallOptions::default())
                .await
        });
        let request = peer.recv_json().await;
        peer.reply_err(
            request["id"].clone(),
            -32601,
            "unknown method: session.compress",
            Some("unknown_method"),
        )
        .await;
        let err = call_err(compress.await.expect("call task ended"));
        let CallError::Rpc(rpc) = &err else {
            panic!("expected an rpc error");
        };
        assert!(rpc.is_method_not_found());

        let caller = client.clone();
        let ping = tokio::spawn(async move {
            caller
                .call_raw("gateway.ping", json!({}), CallOptions::default())
                .await
        });
        let request = peer.recv_json().await;
        peer.reply_ok(request["id"].clone(), json!({"ok": true}))
            .await;
        match ping.await.expect("call task ended") {
            Ok(value) => assert_eq!(value, json!({"ok": true})),
            Err(_) => panic!("gateway.ping failed"),
        }

        let caller = client.clone();
        let again = tokio::spawn(async move {
            caller
                .call_raw("gateway.ping", json!({}), CallOptions::default())
                .await
        });
        let request = peer.recv_json().await;
        peer.reply_ok(request["id"].clone(), json!({"ok": true}))
            .await;
        assert!(again.await.expect("call task ended").is_ok());
    })
    .await;
}

#[tokio::test]
async fn unmatched_and_undecodable_replies_are_reported() {
    guard(async {
        let (parts, mut peer) = Peer::pair().await;
        let (client, mut inbound) = Client::start(parts);
        peer.reply_ok(json!(99), json!({})).await;
        peer.reply_ok(json!("hb-1"), json!({"ok": true})).await;
        wait_stats(&client, |stats| {
            stats.unmatched_replies == 2 && stats.pending_calls == 0
        })
        .await;
        assert_quiet(&mut inbound).await;

        for body in [json!(7), Value::Null] {
            let caller = client.clone();
            let describe = tokio::spawn(async move {
                caller
                    .call::<runtime_describe::Marker>(RuntimeDescribeParams(serde_json::Map::new()))
                    .await
            });
            let request = peer.recv_json().await;
            peer.reply_ok(request["id"].clone(), body).await;
            let err = call_err(describe.await.expect("call task ended"));
            match err {
                CallError::BadResult { method, reason } => {
                    assert_eq!(method, "runtime.describe");
                    assert_eq!(reason, "result did not decode");
                }
                _ => panic!("expected a bad result"),
            }
        }

        let caller = client.clone();
        let both = tokio::spawn(async move {
            caller
                .call_raw("runtime.describe", json!({}), CallOptions::default())
                .await
        });
        let request = peer.recv_json().await;
        let id = request["id"].clone();
        peer.send_text(
            &json!({
                "jsonrpc": "2.0",
                "id": id,
                "result": {"ok": true},
                "error": {"code": 1, "message": "both"}
            })
            .to_string(),
        )
        .await;
        let err = call_err(both.await.expect("call task ended"));
        assert!(matches!(err, CallError::Rpc(_)));
    })
    .await;
}

#[tokio::test]
async fn caller_drop_removes_the_entry() {
    guard(async {
        let (parts, mut peer) = Peer::pair().await;
        let (client, _inbound) = Client::start(parts);
        let mut call =
            Box::pin(client.call_raw("runtime.describe", json!({}), CallOptions::default()));
        tokio::select! {
            biased;
            _ = &mut call => panic!("call finished before drop"),
            _ = std::future::ready(()) => {}
        }
        let request = peer.recv_json().await;
        drop(call);
        assert_eq!(client.stats().await.pending_calls, 0);
        let before = client.stats().await.unmatched_replies;
        peer.reply_ok(request["id"].clone(), json!({})).await;
        wait_stats(&client, |stats| stats.unmatched_replies == before + 1).await;
        assert_eq!(client.stats().await.pending_calls, 0);
    })
    .await;
}

#[tokio::test(flavor = "current_thread")]
async fn call_times_out_and_removes_pending() {
    let (parts, mut peer) = Peer::pair().await;
    let (client, _inbound) = Client::start(parts);
    tokio::time::pause();
    let caller = client.clone();
    let call = tokio::spawn(async move {
        caller
            .call_raw("runtime.describe", json!({}), CallOptions::default())
            .await
    });
    wait_stats(&client, |stats| stats.pending_calls == 1).await;
    tokio::time::advance(Duration::from_secs(119)).await;
    assert!(!call.is_finished());
    assert_eq!(client.stats().await.pending_calls, 1);
    tokio::time::advance(Duration::from_secs(2)).await;
    for _ in 0..50 {
        if call.is_finished() {
            break;
        }
        tokio::task::yield_now().await;
    }
    assert!(call.is_finished());
    let err = call_err(call.await.expect("call task ended"));
    assert!(matches!(
        err,
        CallError::TimedOut { ref method, after }
            if method == "runtime.describe" && after >= Duration::from_secs(120)
    ));
    assert!(err.outcome_unknown());
    assert_eq!(client.stats().await.pending_calls, 0);
    let request = peer.recv_json().await;
    peer.reply_ok(request["id"].clone(), json!({})).await;
    for _ in 0..50 {
        if client.stats().await.unmatched_replies == 1 {
            break;
        }
        tokio::task::yield_now().await;
    }
    assert_eq!(client.stats().await.unmatched_replies, 1);
    assert_eq!(client.stats().await.pending_calls, 0);
}

#[tokio::test(flavor = "current_thread")]
async fn approval_respond_uses_300_second_timeout() {
    // Placeholder values. The generated type also requires `session_id`;
    // the timing check does not depend on it.
    silent_timeout(Duration::from_secs(119), Duration::from_secs(1), |client| {
        Box::pin(async move {
            client
                .call::<runtime_describe::Marker>(RuntimeDescribeParams(serde_json::Map::new()))
                .await
                .map(|_| ())
        })
    })
    .await;
    let approval = CanonicalApprovalParams {
        choice: "deny".to_string(),
        execution_generation: 1,
        profile: None,
        prompt_id: "p-test".to_string(),
        session_id: "s-test".to_string(),
    };
    let (parts, _peer) = Peer::pair().await;
    let (client, _inbound) = Client::start(parts);
    tokio::time::pause();
    let caller = client.clone();
    let call = tokio::spawn(async move {
        caller
            .call::<approval_respond::Marker>(approval)
            .await
            .map(|_| ())
    });
    wait_stats(&client, |stats| stats.pending_calls == 1).await;
    tokio::time::advance(Duration::from_secs(299)).await;
    assert!(!call.is_finished());
    assert_eq!(client.stats().await.pending_calls, 1);
    tokio::time::advance(Duration::from_secs(2)).await;
    let err = call_err_unit(call.await.expect("call task ended"));
    assert!(matches!(
        err,
        CallError::TimedOut { ref method, after }
            if method == "approval.respond" && after >= Duration::from_secs(300)
    ));
    assert_eq!(client.stats().await.pending_calls, 0);
}

#[tokio::test]
async fn dropped_connection_yields_unknown_for_every_pending_call() {
    guard(async {
        let (parts, mut peer) = Peer::pair().await;
        let (client, mut inbound) = Client::start(parts);
        let mut handles = Vec::new();
        for _ in 0..3 {
            let caller = client.clone();
            handles.push(tokio::spawn(async move {
                caller
                    .call_raw("runtime.describe", json!({}), CallOptions::default())
                    .await
            }));
        }
        for _ in 0..3 {
            let _request = peer.recv_json().await;
        }
        assert_eq!(client.stats().await.pending_calls, 3);
        let sent = peer.json_frames();
        peer.kill().await;
        let mut labels = Vec::new();
        for handle in handles {
            let err = call_err(handle.await.expect("call task ended"));
            assert!(err.outcome_unknown());
            labels.push(outcome_label(&err));
        }
        match inbound.next().await {
            Some(Inbound::Closed(ClosedConnection { reason, .. })) => {
                assert!(matches!(reason, CloseReason::Error(_)));
            }
            _ => panic!("expected the close"),
        }
        assert_quiet(&mut inbound).await;
        let after = client
            .call::<runtime_describe::Marker>(RuntimeDescribeParams(serde_json::Map::new()))
            .await;
        let after_err = call_err(after);
        let after_label = outcome_label(&after_err);
        let delta = peer.json_frames().saturating_sub(sent);
        assert_eq!(labels, ["Unknown", "Unknown", "Unknown"]);
        assert_eq!(after_label, "NotConnected");
        assert_eq!(delta, 0);
        println!(concat!(
            "UAT",
            "-3 pending=3 outcomes=Unknown,Unknown,Unknown after_close=NotConnected frames_sent_after_close=0"
        ));
    })
    .await;
}

#[tokio::test]
async fn close_reasons_are_reported() {
    guard(async {
        let (parts, peer) = Peer::pair().await;
        let (client, mut inbound) = Client::start(parts);
        peer.close(1000).await;
        let closed = take_socket_close(&mut inbound).await;
        assert_eq!(
            closed.reason,
            CloseReason::RemoteClosed { code: Some(1000) }
        );
        assert_not_connected(&client).await;

        let (parts, peer) = Peer::pair().await;
        match client.replace_connection(parts).await {
            Ok(_) => {}
            Err(_) => panic!("replace failed"),
        }
        peer.close(1011).await;
        let closed = take_socket_close(&mut inbound).await;
        assert_eq!(closed.reason, CloseReason::Overloaded);
        assert_not_connected(&client).await;

        let (parts, peer) = Peer::pair().await;
        match client.replace_connection(parts).await {
            Ok(_) => {}
            Err(_) => panic!("replace failed"),
        }
        peer.kill().await;
        let closed = take_socket_close(&mut inbound).await;
        match closed.reason {
            CloseReason::Error(kind) => assert!(
                matches!(
                    kind,
                    std::io::ErrorKind::UnexpectedEof | std::io::ErrorKind::ConnectionReset
                ),
                "close kind {kind:?}"
            ),
            _ => panic!("expected an io close"),
        }
    })
    .await;
}

#[tokio::test]
async fn reply_after_replace_is_discarded() {
    guard(async {
        let (parts, mut peer) = Peer::pair().await;
        let (client, mut inbound) = Client::start(parts);
        let caller = client.clone();
        let pending = tokio::spawn(async move {
            caller
                .call_raw("runtime.describe", json!({}), CallOptions::default())
                .await
        });
        let request = peer.recv_json().await;
        peer.hold();
        let (parts, mut peer2) = Peer::pair().await;
        match client.replace_connection(parts).await {
            Ok(generation) => assert_eq!(generation, Generation(2)),
            Err(_) => panic!("replace failed"),
        }
        peer.reply_ok(request["id"].clone(), json!({"ok": true}))
            .await;
        peer.event(
            "message.delta",
            Some("s1"),
            Some(1),
            json!({"text": "late"}),
        )
        .await;
        peer.close(1000).await;
        peer.release().await;
        match inbound.next().await {
            Some(Inbound::Closed(closed)) => {
                assert_eq!(closed.generation, Generation(1));
                assert_eq!(closed.reason, CloseReason::Replaced);
            }
            _ => panic!("expected the replaced close"),
        }
        assert_quiet(&mut inbound).await;
        assert_eq!(client.generation(), Generation(2));
        let caller = client.clone();
        let again = tokio::spawn(async move {
            caller
                .call_raw("runtime.describe", json!({}), CallOptions::default())
                .await
        });
        let request = peer2.recv_json().await;
        peer2
            .reply_ok(request["id"].clone(), json!({"ok": true}))
            .await;
        assert!(again.await.expect("call task ended").is_ok());
        let old = call_err(pending.await.expect("call task ended"));
        assert!(matches!(old, CallError::Unknown { .. }));
        assert_eq!(client.generation(), Generation(2));
    })
    .await;
}

#[tokio::test]
async fn replace_connection_bumps_generation_and_fails_pending() {
    guard(async {
        let (parts, mut peer) = Peer::pair().await;
        let (client, mut inbound) = Client::start(parts);
        let mut pending = Vec::new();
        for _ in 0..2 {
            let caller = client.clone();
            pending.push(tokio::spawn(async move {
                caller
                    .call_raw("runtime.describe", json!({}), CallOptions::default())
                    .await
            }));
        }
        let _first = peer.recv_json().await;
        let _second = peer.recv_json().await;
        let sent = peer.json_frames();
        let (parts, mut peer2) = Peer::pair().await;
        let answerer = tokio::spawn(async move {
            let request = peer2.recv_json().await;
            let id = request
                .get("id")
                .cloned()
                .filter(|id| id.as_u64() == Some(5))
                .unwrap_or_else(|| panic!("request id missing"));
            peer2.reply_ok(id.clone(), json!({"ok": true})).await;
            id
        });
        let caller = client.clone();
        let during = async {
            tokio::task::yield_now().await;
            caller
                .call_raw("runtime.describe", json!({}), CallOptions::default())
                .await
        };
        let (replaced, during, first) =
            tokio::join!(client.replace_connection(parts), during, inbound.next(),);
        match replaced {
            Ok(generation) => assert_eq!(generation, Generation(2)),
            Err(_) => panic!("replace failed"),
        }
        assert_eq!(client.generation(), Generation(2));
        for handle in pending {
            let err = call_err(handle.await.expect("call task ended"));
            assert!(matches!(err, CallError::Unknown { .. }));
        }
        match first {
            Some(Inbound::Closed(closed)) => {
                assert_eq!(closed.generation, Generation(1));
                assert_eq!(closed.reason, CloseReason::Replaced);
            }
            _ => panic!("expected the replaced close"),
        }
        assert!(during.is_ok());
        let id = answerer.await.expect("answerer ended");
        assert_eq!(id, json!(5));
        assert_eq!(peer.json_frames(), sent);
    })
    .await;
}

#[tokio::test]
async fn client_close_cancels_everything() {
    guard(async {
        let (parts, mut peer) = Peer::pair().await;
        let (client, mut inbound) = Client::start(parts);
        let mut pending = Vec::new();
        for _ in 0..2 {
            let caller = client.clone();
            pending.push(tokio::spawn(async move {
                caller
                    .call_raw("runtime.describe", json!({}), CallOptions::default())
                    .await
            }));
        }
        for _ in 0..2 {
            let _request = peer.recv_json().await;
        }
        let _id = peer
            .server_request("sudo", "s1", json!({"command": "apt list"}))
            .await;
        let request = take_server(&mut inbound).await;
        client.close().await;
        for handle in pending {
            let err = call_err(handle.await.expect("call task ended"));
            assert!(matches!(err, CallError::Unknown { .. }));
        }
        let err = request.responder.respond(&json!({"value": "late"})).await;
        assert!(matches!(err, Err(RespondError::Cancelled)));
        match inbound.next().await {
            Some(Inbound::Closed(closed)) => {
                assert_eq!(closed.reason, CloseReason::ClientClosed);
            }
            _ => panic!("expected the client close"),
        }
        assert!(inbound.next().await.is_none());
        let (parts, _peer) = Peer::pair().await;
        match client.replace_connection(parts).await {
            Err(ClientClosed) => {}
            Ok(_) => panic!("replace succeeded after close"),
        }
    })
    .await;
}

#[tokio::test]
async fn events_carry_generation_and_envelope_fields() {
    guard(async {
        let frames = delta_and_two_more();
        let (parts, peer) = Peer::pair().await;
        let (client, mut inbound) = Client::start(parts);
        let _ = client;
        for frame in &frames {
            peer.send_text(&serde_json::to_string(frame).expect("frame encodes"))
                .await;
        }
        for frame in &frames {
            let params = frame.get("params").cloned().expect("params");
            let event = take_event(&mut inbound).await;
            assert_eq!(event.generation, Generation(1));
            assert_eq!(event.raw, params);
            let decoded = EventFrame::from_params(&params).expect("recorded event");
            assert_eq!(event.frame, decoded);
        }
    })
    .await;
}

#[tokio::test]
async fn unknown_and_hand_written_events_are_typed() {
    guard(async {
        let (parts, peer) = Peer::pair().await;
        let (_client, mut inbound) = Client::start(parts);
        let params = json!({"type": "not.in.contract", "payload": {"keep": true}});
        peer.send_text(&json!({"jsonrpc": "2.0", "method": "event", "params": params}).to_string())
            .await;
        let event = take_event(&mut inbound).await;
        assert_eq!(event.raw, params);
        assert!(matches!(
            event.frame.notification,
            Notification::Unknown(ref value) if value == &params
        ));

        for type_name in ["approval.request", "approval.settled"] {
            let frame = recorded_event(type_name, 0);
            let params = frame.get("params").cloned().expect("params");
            peer.send_text(&serde_json::to_string(&frame).expect("frame encodes"))
                .await;
            let event = take_event(&mut inbound).await;
            assert_eq!(event.raw, params);
            assert_eq!(
                event.frame,
                EventFrame::from_params(&params).expect("event")
            );
            match type_name {
                "approval.request" => {
                    assert!(matches!(
                        event.frame.notification,
                        Notification::ApprovalRequest(_)
                    ));
                }
                "approval.settled" => {
                    assert!(matches!(
                        event.frame.notification,
                        Notification::ApprovalSettled(_)
                    ));
                }
                _ => panic!("unexpected event"),
            }
        }

        let bad = json!({
            "jsonrpc": "2.0",
            "method": "event",
            "params": {"type": "message.delta", "payload": 1}
        });
        let text = bad.to_string();
        peer.send_text(&text).await;
        let malformed = take_malformed(&mut inbound).await;
        assert_eq!(malformed.kind, MalformedKind::BadPayload);
        assert_eq!(malformed.label.as_deref(), Some("message.delta"));
        assert!(
            malformed
                .label
                .as_ref()
                .is_none_or(|label| label.len() <= 64)
        );
        assert_eq!(malformed.length, text.len());
    })
    .await;
}

#[tokio::test]
async fn buffered_handshake_frames_are_delivered_first_in_order() {
    guard(async {
        let (mut parts, peer) = Peer::pair().await;
        let a = event_text("A");
        let b = event_text("B");
        let c = event_text("C");
        parts.buffered = vec![a, b];
        let (client, mut inbound) = Client::start(parts);
        peer.send_text(&c).await;
        assert_session(&mut inbound, "A").await;
        assert_session(&mut inbound, "B").await;
        assert_session(&mut inbound, "C").await;

        let (mut parts, peer) = Peer::pair().await;
        let d = event_text("D");
        let e = event_text("E");
        parts.buffered = vec![d];
        match client.replace_connection(parts).await {
            Ok(_) => {}
            Err(_) => panic!("replace failed"),
        }
        peer.send_text(&e).await;
        match inbound.next().await {
            Some(Inbound::Closed(closed)) => {
                assert_eq!(closed.generation, Generation(1));
                assert_eq!(closed.reason, CloseReason::Replaced);
            }
            _ => panic!("expected the replaced close"),
        }
        assert_session(&mut inbound, "D").await;
        assert_session(&mut inbound, "E").await;
    })
    .await;
}

#[tokio::test]
async fn slow_inbound_consumer_does_not_delay_replies() {
    guard(async {
        let (parts, mut peer) = Peer::pair().await;
        let (client, mut inbound) = Client::start(parts);
        let caller = client.clone();
        let call = tokio::spawn(async move {
            caller
                .call_raw("runtime.describe", json!({}), CallOptions::default())
                .await
        });
        let request = peer.recv_json().await;
        for seq in 0..10_000u64 {
            peer.event("not.in.contract", None, Some(seq), json!({}))
                .await;
        }
        peer.reply_ok(request["id"].clone(), json!({"ok": true}))
            .await;
        assert!(call.await.expect("call task ended").is_ok());
        assert!(client.stats().await.inbound_backlog >= 10_000);
        for seq in 0..10_000u64 {
            let event = take_event(&mut inbound).await;
            assert_eq!(event.frame.seq, Some(seq));
        }
    })
    .await;
}

#[tokio::test]
async fn malformed_frame_is_reported_and_connection_survives() {
    guard(async {
        let (parts, mut peer) = Peer::pair().await;
        let (client, mut inbound) = Client::start(parts);
        let caller = client.clone();
        let call = tokio::spawn(async move {
            caller
                .call_raw("runtime.describe", json!({}), CallOptions::default())
                .await
        });
        let request = peer.recv_json().await;
        peer.send_text("not json").await;
        peer.send_text("[1,2]").await;
        peer.send_text(r#"{"id":5,"method":"x"}"#).await;
        let reply = json!({
            "jsonrpc": "2.0",
            "id": request["id"],
            "result": {"ok": true}
        })
        .to_string();
        peer.send_binary(reply.as_bytes()).await;
        assert!(call.await.expect("call task ended").is_ok());
        let mut kinds = Vec::new();
        for _ in 0..3 {
            kinds.push(take_malformed(&mut inbound).await.kind);
        }
        assert_eq!(
            kinds,
            vec![
                MalformedKind::NotJson,
                MalformedKind::NotObject,
                MalformedKind::UnexpectedRequest,
            ]
        );
        let caller = client.clone();
        let again = tokio::spawn(async move {
            caller
                .call_raw("gateway.ping", json!({}), CallOptions::default())
                .await
        });
        let request = peer.recv_json().await;
        peer.reply_ok(request["id"].clone(), json!({"ok": true}))
            .await;
        assert!(again.await.expect("call task ended").is_ok());
    })
    .await;
}

#[tokio::test]
async fn server_request_answered_through_responder() {
    guard(async {
        let (parts, mut peer) = Peer::pair().await;
        let (_client, mut inbound) = Client::start(parts);
        let id = peer
            .server_request("sudo", "s1", json!({"command": "apt list"}))
            .await;
        let request = take_server(&mut inbound).await;
        assert_eq!(request.id, id);
        assert!(matches!(
            request.request,
            hermes_contract::events::ServerRequest::Sudo(_)
        ));
        request
            .responder
            .respond(&json!({"value": SECRET}))
            .await
            .expect("respond");
        let request_debug = format!("{request:?}");
        let responder_debug = format!("{:?}", request.responder);
        let leaked = request_debug.contains(SECRET)
            || request_debug.contains("apt list")
            || responder_debug.contains(SECRET)
            || responder_debug.contains("apt list");
        assert!(
            !leaked,
            "debug lengths {} {}",
            request_debug.len(),
            responder_debug.len()
        );
        let answer = peer.recv_json().await;
        let id_echoed = answer.get("id").and_then(Value::as_str) == Some(id.as_str());
        let method_present = answer.get("method").is_some();
        let value_ok = answer.pointer("/result/value").and_then(Value::as_str) == Some(SECRET);
        assert!(
            id_echoed && value_ok && !method_present,
            "answer shape id_echoed={id_echoed} method_present={method_present} value_len={}",
            answer
                .pointer("/result/value")
                .and_then(Value::as_str)
                .map(str::len)
                .unwrap_or(0)
        );
        println!(concat!(
            "UAT",
            "-4 answered id_echoed=true method_present=false"
        ));
    })
    .await;
}

#[tokio::test]
async fn dropped_responder_answers_internal_error_and_ignore_sends_nothing() {
    guard(async {
        let (parts, mut peer) = Peer::pair().await;
        let (client, mut inbound) = Client::start(parts);
        let id1 = peer
            .server_request("sudo", "s1", json!({"command": "apt list"}))
            .await;
        let request = take_server(&mut inbound).await;
        drop(request);
        let dropped = peer.recv_json().await;
        assert_error(&dropped, &id1, -32603, "request dropped by client");
        assert_eq!(client.stats().await.dropped_without_decision, 1);

        let id2 = peer
            .server_request("sudo", "s1", json!({"command": "apt list"}))
            .await;
        let request = take_server(&mut inbound).await;
        request.responder.ignore();

        let id3 = peer
            .server_request("sudo", "s1", json!({"command": "apt list"}))
            .await;
        let request = take_server(&mut inbound).await;
        request
            .responder
            .fail(-32602, "test message")
            .await
            .expect("fail");
        let failed = peer.recv_json().await;
        assert_error(&failed, &id3, -32602, "test message");
        assert_ne!(failed.get("id").and_then(Value::as_str), Some(id2.as_str()));
    })
    .await;
}

#[tokio::test]
async fn unknown_server_request_method_is_answered_method_not_found() {
    guard(async {
        let (parts, mut peer) = Peer::pair().await;
        let (_client, mut inbound) = Client::start(parts);
        let id1 = peer.server_request("future.kind", "s1", json!({})).await;
        let first = peer.recv_json().await;
        assert_error(
            &first,
            &id1,
            -32601,
            "no handler for server request: future.kind",
        );
        let malformed = take_malformed(&mut inbound).await;
        assert_eq!(malformed.kind, MalformedKind::UnknownServerRequest);

        let id2 = peer.server_request("future.kind", "s1", json!({})).await;
        let second = peer.recv_json().await;
        assert_error(
            &second,
            &id2,
            -32601,
            "no handler for server request: future.kind",
        );
        assert_quiet(&mut inbound).await;

        let id3 = peer
            .server_request("sudo", "s1", json!({"command": 1}))
            .await;
        let third = peer.recv_json().await;
        assert_error(
            &third,
            &id3,
            -32603,
            "server request params could not be read: sudo",
        );
        let malformed = take_malformed(&mut inbound).await;
        assert_eq!(malformed.kind, MalformedKind::BadPayload);
        assert_eq!(malformed.label.as_deref(), Some("sudo"));
    })
    .await;
}

#[tokio::test]
async fn request_cancel_event_marks_responder_cancelled() {
    guard(async {
        let (parts, mut peer) = Peer::pair().await;
        let (_client, mut inbound) = Client::start(parts);
        let id1 = peer
            .server_request("sudo", "s1", json!({"command": "apt list"}))
            .await;
        let first = take_server(&mut inbound).await;
        peer.event(
            "request.cancel",
            Some("s1"),
            None,
            json!({"id": id1, "method": "sudo", "reason": "timeout"}),
        )
        .await;
        let cancel = take_event(&mut inbound).await;
        assert!(matches!(
            cancel.frame.notification,
            Notification::RequestCancel(_)
        ));
        let id2 = peer
            .server_request("sudo", "s1", json!({"command": "apt list"}))
            .await;
        let second = take_server(&mut inbound).await;
        peer.event(
            "request.cancel",
            Some("s1"),
            None,
            json!({"id": id2, "method": "sudo", "reason": "made-up"}),
        )
        .await;
        let cancel = take_event(&mut inbound).await;
        assert!(matches!(
            cancel.frame.notification,
            Notification::RequestCancel(_)
        ));
        let err = first.responder.respond(&json!({"value": "x"})).await;
        assert!(matches!(err, Err(RespondError::Cancelled)));
        let err = second.responder.respond(&json!({"value": "x"})).await;
        assert!(matches!(err, Err(RespondError::Cancelled)));
        assert_no_json(&mut peer).await;
    })
    .await;
}

#[tokio::test]
async fn unanswerable_responder_survives_replace() {
    guard(async {
        let (parts, peer) = Peer::pair().await;
        let (client, mut inbound) = Client::start(parts);
        let id = peer
            .server_request("sudo", "s1", json!({"command": "apt list"}))
            .await;
        let request = take_server(&mut inbound).await;
        peer.kill().await;
        let closed = take_socket_close(&mut inbound).await;
        assert!(matches!(closed.reason, CloseReason::Error(_)));
        let err = request.responder.respond(&json!({"value": "later"})).await;
        assert!(matches!(err, Err(RespondError::NotConnected)));
        assert_eq!(client.stats().await.open_server_requests, 1);

        let (parts, mut peer) = Peer::pair().await;
        match client.replace_connection(parts).await {
            Ok(_) => {}
            Err(_) => panic!("replace failed"),
        }
        request
            .responder
            .respond(&json!({"value": "later"}))
            .await
            .expect("respond after replace");
        let answer = peer.recv_json().await;
        assert_eq!(answer.get("id").and_then(Value::as_str), Some(id.as_str()));
        assert_eq!(
            answer.pointer("/result/value").and_then(Value::as_str),
            Some("later")
        );

        let before_drop = client.stats().await.dropped_without_decision;
        let id2 = peer
            .server_request("sudo", "s1", json!({"command": "apt list"}))
            .await;
        let second = take_server(&mut inbound).await;
        let _ = id2;
        peer.kill().await;
        let _closed = take_socket_close(&mut inbound).await;
        let sent = peer.json_frames();
        drop(second);
        assert_eq!(peer.json_frames(), sent);
        let stats = client.stats().await;
        assert_eq!(stats.open_server_requests, 0);
        assert_eq!(stats.dropped_without_decision, before_drop + 1);
    })
    .await;
}

#[tokio::test]
async fn dropped_inbound_stream_answers_requests_method_not_found() {
    guard(async {
        let (parts, mut peer) = Peer::pair().await;
        let (client, inbound) = Client::start(parts);
        drop(inbound);
        let id = peer
            .server_request("sudo", "s1", json!({"command": "apt list"}))
            .await;
        let answer = peer.recv_json().await;
        assert_eq!(answer.get("id").and_then(Value::as_str), Some(id.as_str()));
        assert_eq!(
            answer.pointer("/error/code").and_then(Value::as_i64),
            Some(-32601)
        );
        let caller = client.clone();
        let call = tokio::spawn(async move {
            caller
                .call_raw("gateway.ping", json!({}), CallOptions::default())
                .await
        });
        let request = peer.recv_json().await;
        peer.reply_ok(request["id"].clone(), json!({"ok": true}))
            .await;
        assert!(call.await.expect("call task ended").is_ok());
    })
    .await;
}

#[tokio::test]
async fn unsendable_calls_send_nothing() {
    guard(async {
        let (parts, peer) = Peer::pair().await;
        let (client, _inbound) = Client::start(parts);
        let err = call_err(
            client
                .call_with::<runtime_describe::Marker>(
                    RuntimeDescribeParams(serde_json::Map::new()),
                    CallOptions {
                        timeout: Some(Duration::ZERO),
                    },
                )
                .await
                .map(|_| ()),
        );
        assert!(matches!(err, CallError::InvalidTimeout));
        assert_eq!(peer.json_frames(), 0);
        let err = call_err(
            client
                .call_raw("runtime.describe", json!([1]), CallOptions::default())
                .await,
        );
        assert!(matches!(err, CallError::Encode));
        assert_eq!(peer.json_frames(), 0);
        client.close().await;
        let err = call_err(
            client
                .call::<runtime_describe::Marker>(RuntimeDescribeParams(serde_json::Map::new()))
                .await
                .map(|_| ()),
        );
        assert!(matches!(err, CallError::NotConnected));
        assert_eq!(peer.json_frames(), 0);

        let (parts, mut peer) = Peer::pair().await;
        let (_client, mut inbound) = Client::start(parts);
        let _id = peer
            .server_request("sudo", "s1", json!({"command": "apt list"}))
            .await;
        let request = take_server(&mut inbound).await;
        let sent = peer.json_frames();
        let err = request.responder.respond(&Refuse).await;
        assert!(matches!(err, Err(RespondError::Encode)));
        assert_eq!(peer.json_frames(), sent);
        request
            .responder
            .respond(&json!({"value": "ok"}))
            .await
            .expect("later respond");
        let answer = peer.recv_json().await;
        assert_eq!(
            answer.pointer("/result/value").and_then(Value::as_str),
            Some("ok")
        );
    })
    .await;
}

async fn silent_timeout<F, Fut>(pending_for: Duration, rest: Duration, launch: F)
where
    F: FnOnce(Client) -> std::pin::Pin<Box<Fut>>,
    Fut: std::future::Future<Output = Result<(), CallError>> + Send + 'static,
{
    let (parts, _peer) = Peer::pair().await;
    let (client, _inbound) = Client::start(parts);
    tokio::time::pause();
    let call = tokio::spawn(launch(client.clone()));
    wait_stats(&client, |stats| stats.pending_calls == 1).await;
    tokio::time::advance(pending_for).await;
    assert!(!call.is_finished());
    assert_eq!(client.stats().await.pending_calls, 1);
    tokio::time::advance(rest).await;
    let err = call_err_unit(call.await.expect("call task ended"));
    assert!(matches!(
        err,
        CallError::TimedOut { ref method, .. } if method == "runtime.describe"
    ));
    assert_eq!(client.stats().await.pending_calls, 0);
    tokio::time::resume();
}

fn delta_and_two_more() -> Vec<Value> {
    let lines = recorded_lines();
    let start = lines
        .iter()
        .position(|frame| {
            frame.pointer("/params/type").and_then(Value::as_str) == Some("message.delta")
        })
        .expect("recorded event is missing");
    lines[start..start + 3].to_vec()
}

fn event_text(label: &str) -> String {
    json!({
        "jsonrpc": "2.0",
        "method": "event",
        "params": {"type": "not.in.contract", "session_id": label, "payload": {}}
    })
    .to_string()
}

async fn wait_stats(client: &Client, mut ready: impl FnMut(&hermes_client::ChannelStats) -> bool) {
    let started = tokio::time::Instant::now();
    loop {
        let stats = client.stats().await;
        if ready(&stats) {
            return;
        }
        if started.elapsed() > Duration::from_secs(5) {
            panic!("stats did not reach the expected state");
        }
        tokio::task::yield_now().await;
    }
}

async fn assert_quiet(inbound: &mut InboundStream) {
    match tokio::time::timeout(Duration::from_millis(200), inbound.next()).await {
        Err(_) => {}
        Ok(Some(item)) => panic!("unexpected {}", kind_name(&item)),
        Ok(None) => panic!("stream ended"),
    }
}

async fn assert_no_json(peer: &mut Peer) {
    let arrived = tokio::time::timeout(Duration::from_millis(200), peer.recv_json())
        .await
        .is_ok();
    assert!(!arrived, "a frame was sent");
}

async fn assert_not_connected(client: &Client) {
    let err = call_err(
        client
            .call::<runtime_describe::Marker>(RuntimeDescribeParams(serde_json::Map::new()))
            .await
            .map(|_| ()),
    );
    assert!(matches!(err, CallError::NotConnected));
}

async fn take_event(inbound: &mut InboundStream) -> InboundEvent {
    match inbound.next().await {
        Some(Inbound::Event(event)) => event,
        Some(item) => panic!("expected an event, got {}", kind_name(&item)),
        None => panic!("stream ended"),
    }
}

async fn assert_session(inbound: &mut InboundStream, label: &str) {
    let event = take_event(inbound).await;
    assert_eq!(
        event.raw.get("session_id").and_then(Value::as_str),
        Some(label)
    );
}

async fn take_server(inbound: &mut InboundStream) -> ServerRequest {
    match inbound.next().await {
        Some(Inbound::ServerRequest(request)) => request,
        Some(item) => panic!("expected a server request, got {}", kind_name(&item)),
        None => panic!("stream ended"),
    }
}

async fn take_malformed(inbound: &mut InboundStream) -> MalformedFrame {
    match inbound.next().await {
        Some(Inbound::Malformed(frame)) => frame,
        Some(item) => panic!("expected a malformed frame, got {}", kind_name(&item)),
        None => panic!("stream ended"),
    }
}

async fn take_socket_close(inbound: &mut InboundStream) -> ClosedConnection {
    loop {
        match inbound.next().await {
            Some(Inbound::Closed(closed)) if closed.reason == CloseReason::Replaced => continue,
            Some(Inbound::Closed(closed)) => return closed,
            Some(item) => panic!("expected a close, got {}", kind_name(&item)),
            None => panic!("stream ended"),
        }
    }
}

fn kind_name(item: &Inbound) -> &'static str {
    match item {
        Inbound::Event(_) => "event",
        Inbound::ServerRequest(_) => "server request",
        Inbound::Malformed(_) => "malformed",
        Inbound::Closed(_) => "closed",
    }
}

fn call_err<T>(result: Result<T, CallError>) -> CallError {
    match result {
        Err(err) => err,
        Ok(_) => panic!("call returned a result"),
    }
}

fn call_err_unit(result: Result<(), CallError>) -> CallError {
    call_err(result)
}

fn outcome_label(err: &CallError) -> &'static str {
    match err {
        CallError::NotConnected => "NotConnected",
        CallError::Encode => "Encode",
        CallError::InvalidTimeout => "InvalidTimeout",
        CallError::Rpc(_) => "Rpc",
        CallError::BadResult { .. } => "BadResult",
        CallError::TimedOut { .. } => "TimedOut",
        CallError::Unknown { .. } => "Unknown",
    }
}

fn assert_error(frame: &Value, id: &str, code: i64, message: &str) {
    let id_ok = frame.get("id").and_then(Value::as_str) == Some(id);
    let code_ok = frame.pointer("/error/code").and_then(Value::as_i64) == Some(code);
    let message_ok = frame.pointer("/error/message").and_then(Value::as_str) == Some(message);
    let method_absent = frame.get("method").is_none();
    assert!(
        id_ok && code_ok && message_ok && method_absent,
        "error shape id_ok={id_ok} code_ok={code_ok} message_ok={message_ok} method_absent={method_absent}"
    );
}

struct Refuse;

impl Serialize for Refuse {
    fn serialize<S: serde::Serializer>(&self, _serializer: S) -> Result<S::Ok, S::Error> {
        Err(S::Error::custom("refuse"))
    }
}
