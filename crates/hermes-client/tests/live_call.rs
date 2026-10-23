//! Ignored live calls. The red run does not execute them.
//!
//! A missing gateway, a missing `fixture` profile, or a pin mismatch prints
//! `LIVE_CALL_RESULT: SKIPPED (<reason>)` and returns. Any other attach error panics.

mod support;

use hermes_client::{
    CallError, CallOptions, Client, InboundStream, Purpose, Scope, connect, handshake, mint_ticket,
};
use hermes_contract::methods::canonical::{runtime_describe, session_info, session_list};
use hermes_contract::schemas::{
    CanonicalListParams, CanonicalSessionInfoParams, RuntimeDescribeParams, Scope as ListScope,
};
use serde_json::json;
use support::{LiveEnv, live_preconditions, report_skip};

#[tokio::test]
#[ignore]
async fn live_typed_calls_roundtrip() {
    let Some((client, _inbound)) = prepare().await else {
        return;
    };
    let empty = RuntimeDescribeParams(serde_json::Map::new());
    must_ok(client.call::<runtime_describe::Marker>(empty.clone()).await);
    must_ok(
        client
            .call::<session_list::Marker>(list(ListScope::Live))
            .await,
    );
    let (describe, live, stored) = tokio::join!(
        client.call::<runtime_describe::Marker>(empty),
        client.call::<session_list::Marker>(list(ListScope::Live)),
        client.call::<session_list::Marker>(list(ListScope::Stored)),
    );
    must_ok(describe);
    must_ok(live);
    must_ok(stored);
    finish("LIVE_CALL_RESULT: PASSED");
}

#[tokio::test]
#[ignore]
async fn live_refused_method_is_method_not_found() {
    let Some((client, _inbound)) = prepare().await else {
        return;
    };
    let refused = client
        .call_raw("session.compress", json!({}), CallOptions::default())
        .await;
    let Err(CallError::Rpc(err)) = refused else {
        panic!("session.compress was not refused");
    };
    assert_eq!(err.code, -32601);
    assert_eq!(err.reason.as_deref(), Some("unknown_method"));
    assert!(err.is_method_not_found());
    must_ok(
        client
            .call::<session_list::Marker>(list(ListScope::Live))
            .await,
    );
    finish("LIVE_CALL_RESULT: PASSED");
}

#[tokio::test]
#[ignore]
async fn live_unknown_session_gives_not_found() {
    let Some((client, _inbound)) = prepare().await else {
        return;
    };
    let unknown = CanonicalSessionInfoParams {
        session_id: "0".repeat(32),
    };
    let Err(CallError::Rpc(err)) = client.call::<session_info::Marker>(unknown).await else {
        panic!("session.info was not refused");
    };
    assert_eq!(err.code, 4001);
    assert_eq!(err.reason.as_deref(), Some("not_found"));
    let line = format!(
        "session.info code={} reason={}",
        err.code,
        err.reason.as_deref().unwrap_or("")
    );
    assert_public(&line);
    println!("{line}");
    finish("LIVE_CALL_RESULT: PASSED");
}

async fn prepare() -> Option<(Client, InboundStream)> {
    match live_preconditions().await {
        Ok(env) => Some(attach(&env).await),
        Err(reason) => {
            report_skip("LIVE_CALL_RESULT", &reason);
            None
        }
    }
}

async fn attach(env: &LiveEnv) -> (Client, InboundStream) {
    let ticket = mint_ticket(
        &env.home,
        &env.endpoint,
        &env.fixture,
        Purpose::Interactive,
        Scope::profile(),
    )
    .await
    .unwrap_or_else(|err| panic!("{err} | {}", err.next_step()));
    let mut connection = connect(&env.endpoint, ticket)
        .await
        .unwrap_or_else(|err| panic!("{err} | {}", err.next_step()));
    handshake(&mut connection)
        .await
        .unwrap_or_else(|err| panic!("{err} | {}", err.next_step()));
    Client::start(connection.into_parts())
}

fn list(scope: ListScope) -> CanonicalListParams {
    CanonicalListParams {
        scope,
        limit: 5,
        ..CanonicalListParams::default()
    }
}

fn must_ok<T>(result: Result<T, CallError>) {
    if result.is_err() {
        panic!("live call failed");
    }
}

fn finish(line: &str) {
    assert_public(line);
    println!("{line}");
}

fn assert_public(text: &str) {
    assert!(
        !text.contains("hermes-gateway-ticket"),
        "ticket marker in live output"
    );
    assert!(!has_long_token(text), "live output has a long token");
}

fn has_long_token(text: &str) -> bool {
    let mut run = 0;
    for ch in text.chars() {
        if ch.is_ascii_alphanumeric() || ch == '_' || ch == '-' {
            run += 1;
            if run >= 40 {
                return true;
            }
        } else {
            run = 0;
        }
    }
    false
}
