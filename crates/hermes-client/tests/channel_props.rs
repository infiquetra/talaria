//! Property tests for one outcome per call, and for a total classifier.

mod support;

use std::time::Duration;

use hermes_client::{CallError, CallOptions, Client, FrameClass, Inbound, classify};
use proptest::prelude::*;
use proptest::test_runner::{Config, TestRunner};
use serde_json::{Value, json};
use support::peer::Peer;
use tokio::task::JoinHandle;

#[derive(Clone, Copy, Debug)]
enum Fate {
    OkReply,
    ErrorReply,
    Hang,
    Drop,
}

#[derive(Clone, Copy, Debug)]
enum Shock {
    Kill,
    Replace,
}

struct Link {
    peer: Peer,
    live: bool,
}

#[test]
fn every_call_resolves_exactly_once() {
    let mut config = Config::with_cases(32);
    config.failure_persistence = None;
    let mut runner = TestRunner::new(config);
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("runtime");
    let strategy = (1usize..=20).prop_flat_map(|count| {
        (
            prop::collection::vec(fate_strategy(), count),
            prop::option::of((0..count, shock_strategy())),
        )
    });
    runner
        .run(&strategy, |(fates, shock)| {
            runtime.block_on(async {
                match tokio::time::timeout(Duration::from_secs(5), drive(fates, shock)).await {
                    Ok(()) => {}
                    Err(_) => panic!("5s guard"),
                }
            });
            Ok(())
        })
        .expect("property");
}

#[test]
fn arbitrary_text_never_panics_the_classifier() {
    let mut config = Config::with_cases(32);
    config.failure_persistence = None;
    let mut runner = TestRunner::new(config);
    runner
        .run(&(any::<String>(), any::<Vec<u8>>()), |(text, bytes)| {
            let _ = classify(text.as_bytes());
            let _ = classify(&bytes);
            let _: FrameClass = classify(text.as_bytes());
            Ok(())
        })
        .expect("property");
}

fn fate_strategy() -> impl Strategy<Value = Fate> {
    prop_oneof![
        Just(Fate::OkReply),
        Just(Fate::ErrorReply),
        Just(Fate::Hang),
        Just(Fate::Drop),
    ]
}

fn shock_strategy() -> impl Strategy<Value = Shock> {
    prop_oneof![Just(Shock::Kill), Just(Shock::Replace)]
}

async fn drive(fates: Vec<Fate>, shock: Option<(usize, Shock)>) {
    let (parts, peer) = Peer::pair().await;
    let (client, mut inbound) = Client::start(parts);
    let mut link = Link { peer, live: true };
    let mut hanging: Vec<JoinHandle<Result<Value, CallError>>> = Vec::new();

    for (index, fate) in fates.into_iter().enumerate() {
        if link.live {
            match fate {
                Fate::OkReply => {
                    let handle = spawn_call(&client);
                    let request = link.peer.recv_json().await;
                    link.peer
                        .reply_ok(request_id(&request), json!({"ok": true}))
                        .await;
                    assert!(handle.await.expect("call task ended").is_ok());
                }
                Fate::ErrorReply => {
                    let handle = spawn_call(&client);
                    let request = link.peer.recv_json().await;
                    link.peer
                        .reply_err(request_id(&request), 1, "no", Some("no"))
                        .await;
                    let err = unwrap_err(handle.await.expect("call task ended"));
                    assert!(matches!(err, CallError::Rpc(_)));
                    assert!(!err.outcome_unknown());
                }
                Fate::Hang => {
                    let handle = spawn_call(&client);
                    let _request = link.peer.recv_json().await;
                    hanging.push(handle);
                }
                Fate::Drop => {
                    let mut call = Box::pin(client.call_raw(
                        "runtime.describe",
                        json!({}),
                        CallOptions::default(),
                    ));
                    tokio::select! {
                        biased;
                        _ = &mut call => panic!("dropped call finished"),
                        _ = std::future::ready(()) => {}
                    }
                    let _request = link.peer.recv_json().await;
                    drop(call);
                }
            }
            assert_eq!(client.stats().await.pending_calls, hanging.len());
        } else {
            let before = link.peer.json_frames();
            let err = unwrap_err(
                client
                    .call_raw("runtime.describe", json!({}), CallOptions::default())
                    .await,
            );
            assert!(matches!(err, CallError::NotConnected));
            assert!(!err.outcome_unknown());
            assert_eq!(link.peer.json_frames(), before);
        }

        if let Some((at, kind)) = shock
            && at == index
        {
            match kind {
                Shock::Kill => {
                    link.peer.kill().await;
                    assert!(matches!(inbound.next().await, Some(Inbound::Closed(_))));
                    link.live = false;
                    finish_hangs(&mut hanging).await;
                }
                Shock::Replace => {
                    let (parts, peer) = Peer::pair().await;
                    match client.replace_connection(parts).await {
                        Ok(_) => {}
                        Err(_) => panic!("replace failed"),
                    }
                    finish_hangs(&mut hanging).await;
                    link.peer.kill().await;
                    link = Link { peer, live: true };
                }
            }
            assert_eq!(client.stats().await.pending_calls, 0);
        }
    }

    let waiting = std::mem::take(&mut hanging);
    client.close().await;
    for handle in waiting {
        let err = unwrap_err(handle.await.expect("call task ended"));
        assert!(matches!(err, CallError::Unknown { .. }));
        assert!(err.outcome_unknown());
    }
    assert_eq!(client.stats().await.pending_calls, 0);
}

fn spawn_call(client: &Client) -> JoinHandle<Result<Value, CallError>> {
    let client = client.clone();
    tokio::spawn(async move {
        client
            .call_raw("runtime.describe", json!({}), CallOptions::default())
            .await
    })
}

async fn finish_hangs(hanging: &mut Vec<JoinHandle<Result<Value, CallError>>>) {
    for handle in hanging.drain(..) {
        let err = unwrap_err(handle.await.expect("call task ended"));
        assert!(matches!(err, CallError::Unknown { .. }));
        assert!(err.outcome_unknown());
    }
}

fn request_id(frame: &Value) -> Value {
    frame
        .get("id")
        .cloned()
        .filter(|id| id.is_number())
        .unwrap_or_else(|| panic!("request id missing"))
}

fn unwrap_err(result: Result<Value, CallError>) -> CallError {
    match result {
        Err(err) => err,
        Ok(_) => panic!("call returned a result"),
    }
}
