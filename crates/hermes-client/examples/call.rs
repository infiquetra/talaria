//! Read-only calls against the local Hermes gateway.
//!
//! `cargo run -p hermes-client --example call -- --profile fixture`
//!
//! `--probe-refused` also calls `session.compress` and `session.info`.
//! The example never prints a ticket, the pinned commit, or `hermes-gateway-ticket`.

use std::process::exit;
use std::time::{Duration, Instant};

use hermes_client::{
    CallError, CallOptions, Client, DiscoverOptions, DiscoveryError, EnsurePolicy,
    ProfileLookupError, Purpose, Scope, connect, default_home, discover_with, handshake,
    mint_ticket,
};
use hermes_contract::methods::canonical::{runtime_describe, session_info, session_list};
use hermes_contract::schemas::{
    CanonicalListParams, CanonicalSessionInfoParams, RuntimeDescribeParams, Scope as ListScope,
};
use serde_json::json;

#[tokio::main]
async fn main() {
    let mut profile = "fixture".to_string();
    let mut probe_refused = false;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--profile" => {
                profile = args.next().unwrap_or_else(|| {
                    eprintln!("missing profile name");
                    exit(2);
                });
            }
            "--probe-refused" => probe_refused = true,
            _ => {
                eprintln!("unknown argument");
                exit(2);
            }
        }
    }

    let client = attach(&profile).await;
    let empty = RuntimeDescribeParams(serde_json::Map::new());
    timed_ok("call runtime.describe ok", async {
        client.call::<runtime_describe::Marker>(empty.clone()).await
    })
    .await;
    timed_ok("call session.list ok", async {
        client
            .call::<session_list::Marker>(list(ListScope::Live))
            .await
    })
    .await;

    let started = Instant::now();
    let (describe, live, stored) = tokio::join!(
        client.call::<runtime_describe::Marker>(empty),
        client.call::<session_list::Marker>(list(ListScope::Live)),
        client.call::<session_list::Marker>(list(ListScope::Stored)),
    );
    if let Err(err) = describe {
        fail_call(&err);
    }
    if let Err(err) = live {
        fail_call(&err);
    }
    if let Err(err) = stored {
        fail_call(&err);
    }
    println!("concurrent 3 calls ok {} ms", started.elapsed().as_millis());

    if probe_refused {
        match client
            .call_raw("session.compress", json!({}), CallOptions::default())
            .await
        {
            Err(CallError::Rpc(err))
                if err.code == -32601 && err.reason.as_deref() == Some("unknown_method") =>
            {
                println!("call session.compress refused code=-32601 reason=unknown_method");
            }
            Err(err) => fail_call(&err),
            Ok(_) => {
                eprintln!("session.compress returned a result");
                exit(1);
            }
        }
        let unknown = CanonicalSessionInfoParams {
            session_id: "0".repeat(32),
        };
        match client.call::<session_info::Marker>(unknown).await {
            Err(CallError::Rpc(err))
                if err.code == 4001 && err.reason.as_deref() == Some("not_found") =>
            {
                println!("call session.info refused code=4001 reason=not_found");
            }
            Err(err) => fail_call(&err),
            Ok(_) => {
                eprintln!("session.info returned a result");
                exit(1);
            }
        }
        match client
            .call::<session_list::Marker>(list(ListScope::Live))
            .await
        {
            Ok(_) => println!("after refusals: session.list ok"),
            Err(err) => fail_call(&err),
        }
    }

    client.close().await;
    println!("CALL_EXAMPLE_RESULT: PASSED");
}

fn list(scope: ListScope) -> CanonicalListParams {
    CanonicalListParams {
        scope,
        limit: 5,
        ..CanonicalListParams::default()
    }
}

async fn timed_ok<T>(label: &str, call: impl std::future::Future<Output = Result<T, CallError>>) {
    let started = Instant::now();
    if let Err(err) = call.await {
        fail_call(&err);
    }
    println!("{label} {} ms", started.elapsed().as_millis());
}

async fn attach(profile_name: &str) -> Client {
    let home = default_home();
    let options = DiscoverOptions {
        control_timeout: Duration::from_secs(5),
        ensure: EnsurePolicy::Never,
    };
    let endpoint = match discover_with(&home, &options).await {
        Ok(endpoint) => endpoint,
        Err(DiscoveryError::NoGateway) => skip("no Hermes gateway is running for this home"),
        Err(err) => panic!("{err} | {}", err.next_step()),
    };
    let profile = match endpoint.resolve_profile(profile_name) {
        Ok(profile) => profile,
        Err(ProfileLookupError::NeverDiscovered { name }) => {
            skip(&format!(
                "the gateway never discovered a profile named {name}"
            ));
        }
        Err(err) => panic!("{err} | {}", err.next_step()),
    };
    if let Some(reason) = hermes_contract::pinned::live_skip_reason(endpoint.code_sha()) {
        skip(&reason);
    }
    let ticket = mint_ticket(
        &home,
        &endpoint,
        &profile,
        Purpose::Interactive,
        Scope::profile(),
    )
    .await
    .unwrap_or_else(|err| panic!("{err} | {}", err.next_step()));
    let mut connection = connect(&endpoint, ticket)
        .await
        .unwrap_or_else(|err| panic!("{err} | {}", err.next_step()));
    handshake(&mut connection)
        .await
        .unwrap_or_else(|err| panic!("{err} | {}", err.next_step()));
    Client::start(connection.into_parts()).0
}

fn skip(reason: &str) -> ! {
    println!("CALL_EXAMPLE_RESULT: SKIPPED ({reason})");
    let code = if std::env::var("TALARIA_LIVE_REQUIRE").ok().as_deref() == Some("1") {
        1
    } else {
        2
    };
    exit(code);
}

fn fail_call(err: &CallError) -> ! {
    eprintln!("call failed: {}", outcome(err));
    exit(1);
}

fn outcome(err: &CallError) -> String {
    match err {
        CallError::NotConnected => "not connected".to_string(),
        CallError::Encode => "encode".to_string(),
        CallError::InvalidTimeout => "invalid timeout".to_string(),
        CallError::Rpc(rpc) => format!(
            "rpc code={} reason={}",
            rpc.code,
            rpc.reason.as_deref().unwrap_or("")
        ),
        CallError::BadResult { method, reason } => format!("bad result {method} {reason}"),
        CallError::TimedOut { method, after } => {
            format!("timed out {method} {} ms", after.as_millis())
        }
        CallError::Unknown { method } => format!("unknown {method}"),
    }
}
