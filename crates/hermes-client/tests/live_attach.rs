//! Live attach checks. Ignored until a gateway is actually running.
//!
//! `live_preconditions` discovers first, so a stub fails there.

mod support;

use std::time::Instant;

use hermes_client::{
    ConnectError, DiscoverOptions, EnsurePolicy, Purpose, Scope, Ticket, TicketError, connect,
    discover_with, handshake, mint_ticket,
};
use support::{live_preconditions, report_skip};

const RESULT: &str = "LIVE_ATTACH_RESULT";

fn never() -> DiscoverOptions {
    DiscoverOptions {
        control_timeout: std::time::Duration::from_secs(5),
        ensure: EnsurePolicy::Never,
    }
}

fn skip_or_env(result: Result<support::LiveEnv, String>) -> Option<support::LiveEnv> {
    match result {
        Ok(env) => Some(env),
        Err(reason) => {
            report_skip(RESULT, &reason);
            None
        }
    }
}

fn token_run(text: &str) -> bool {
    let mut run = 0usize;
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

#[tokio::test]
#[ignore]
async fn live_attach_prints_timings_and_never_the_ticket() {
    let Some(env) = skip_or_env(live_preconditions().await) else {
        return;
    };
    let mut lines = Vec::new();
    let started = Instant::now();
    let endpoint = discover_with(&env.home, &never())
        .await
        .unwrap_or_else(|err| {
            panic!("identify failed: {err} | {}", err.next_step());
        });
    lines.push(format!(
        "step identify ok {} ms",
        started.elapsed().as_millis()
    ));

    let profile = endpoint.resolve_profile("fixture").unwrap_or_else(|err| {
        panic!("fixture lookup failed: {err} | {}", err.next_step());
    });
    let started = Instant::now();
    let ticket = mint_ticket(
        &env.home,
        &endpoint,
        &profile,
        Purpose::Interactive,
        Scope::profile(),
    )
    .await
    .unwrap_or_else(|err| panic!("ticket failed: {err} | {}", err.next_step()));
    lines.push(format!(
        "step ticket ok {} ms",
        started.elapsed().as_millis()
    ));

    let started = Instant::now();
    let mut connection = connect(&endpoint, ticket)
        .await
        .unwrap_or_else(|err| panic!("upgrade failed: {err} | {}", err.next_step()));
    lines.push(format!(
        "step upgrade ok {} ms",
        started.elapsed().as_millis()
    ));

    let done = handshake(&mut connection)
        .await
        .unwrap_or_else(|err| panic!("handshake failed: {err} | {}", err.next_step()));
    lines.push(format!(
        "step gateway.ready ok {} ms",
        done.timings.ready.as_millis()
    ));
    if done.server_requests.is_none() {
        let failed = format!(
            "step client.capabilities failed {} ms",
            done.timings.capabilities.as_millis()
        );
        println!("{failed}");
        panic!("client.capabilities failed");
    }
    lines.push(format!(
        "step client.capabilities ok {} ms",
        done.timings.capabilities.as_millis()
    ));
    lines.push(format!(
        "step runtime.describe ok {} ms",
        done.timings.describe.as_millis()
    ));
    for line in &lines {
        println!("{line}");
    }
    println!("{RESULT}: PASSED");
    let mut transcript = lines.join("\n");
    transcript.push_str(&format!("\n{RESULT}: PASSED"));
    if transcript.contains("hermes-gateway-ticket") || token_run(&transcript) {
        panic!("live output held a ticket");
    }
}

#[tokio::test]
#[ignore]
async fn live_attach_stale_instance_is_refused() {
    let Some(env) = skip_or_env(live_preconditions().await) else {
        return;
    };
    let stale = env.endpoint.with_instance_id(&"0".repeat(32));
    match mint_ticket(
        &env.home,
        &stale,
        &env.fixture,
        Purpose::Interactive,
        Scope::profile(),
    )
    .await
    {
        Err(TicketError::StaleInstance) => println!("{RESULT}: PASSED"),
        Err(err) => panic!("stale instance: {err} | {}", err.next_step()),
        Ok(_) => panic!("stale instance was accepted"),
    }
}

#[tokio::test]
#[ignore]
async fn live_attach_unknown_ticket_is_refused_at_upgrade() {
    let Some(env) = skip_or_env(live_preconditions().await) else {
        return;
    };
    let ticket = Ticket::parse("n".repeat(43)).unwrap_or_else(|err| {
        panic!("ticket parse failed: {err} | {}", err.next_step());
    });
    match connect(&env.endpoint, ticket).await {
        Err(ConnectError::UpgradeRejected { status: Some(403) }) => {
            println!("status=403");
            println!("{RESULT}: PASSED");
        }
        Err(ConnectError::UpgradeRejected { status }) => {
            println!("status={status:?}");
            panic!("unexpected upgrade status");
        }
        Err(err) => panic!("upgrade failed: {err} | {}", err.next_step()),
        Ok(_) => panic!("unknown ticket was accepted"),
    }
}
