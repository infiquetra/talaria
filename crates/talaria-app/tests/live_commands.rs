//! Read-only slash calls against a running Hermes gateway. Issue #223.
//!
//! Ignored unless someone asks for it. A skip prints `SKIPPED:` and is not a pass when
//! `TALARIA_LIVE_REQUIRE=1` is set.

use std::path::PathBuf;
use std::time::{Duration, Instant};

use hermes_client::{
    CallOptions, ConnectOptions, DiscoverOptions, EnsurePolicy, Purpose, Scope, connect_with,
    default_home, discover_with, handshake, mint_ticket,
};
use serde_json::json;
use talaria_app::commands::CommandService;
use talaria_core::commands::{CommandOutcome, CommandsInput};

fn home() -> PathBuf {
    std::env::var_os("HERMES_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(default_home)
}

fn require_or_skip(reason: &str) -> bool {
    println!("SKIPPED: {reason}");
    if std::env::var("TALARIA_LIVE_REQUIRE").ok().as_deref() == Some("1") {
        panic!("live test skipped: {reason}");
    }
    false
}

#[tokio::test]
#[ignore = "needs a Hermes gateway on the fixture profile"]
async fn live_catalog_completion_and_read_commands() {
    let options = DiscoverOptions {
        control_timeout: Duration::from_secs(5),
        ensure: EnsurePolicy::Never,
    };
    let endpoint = match discover_with(&home(), &options).await {
        Ok(endpoint) => endpoint,
        Err(_) => {
            require_or_skip("no Hermes gateway reachable");
            return;
        }
    };
    if let Some(reason) = hermes_contract::pinned::live_skip_reason(endpoint.code_sha()) {
        require_or_skip(&reason);
        return;
    }
    let profile = match endpoint.resolve_profile("fixture") {
        Ok(profile) => profile,
        Err(_) => {
            require_or_skip("the fixture profile is not served");
            return;
        }
    };
    let ticket = mint_ticket(
        &home(),
        &endpoint,
        &profile,
        Purpose::Interactive,
        Scope::profile(),
    )
    .await
    .expect("ticket");
    let mut connection = connect_with(&endpoint, ticket, &ConnectOptions::default())
        .await
        .expect("connect");
    handshake(&mut connection).await.expect("handshake");
    let (client, _inbound) = hermes_client::Client::start(connection.into_parts());
    let folder = tempfile::tempdir().expect("temporary folder");
    let created = client
        .call_raw(
            "session.create",
            json!({
                "source": "cli",
                "cwd": folder.path(),
                "model": "minimax-m3.1-flash",
                "provider": "ollama-cloud",
            }),
            CallOptions {
                timeout: Some(Duration::from_secs(30)),
            },
        )
        .await
        .expect("session.create");
    let session_id = created["session_id"].as_str().unwrap_or("").to_owned();
    let mut service = CommandService::new(client);

    let started = Instant::now();
    let catalog = service.fetch_catalog(1, Some(session_id.clone())).await;
    println!("fetch_catalog {} ms", started.elapsed().as_millis());
    assert!(matches!(catalog, CommandsInput::CatalogArrived { .. }));

    let started = Instant::now();
    let completion = service
        .complete(2, "/he".into(), Some(session_id.clone()))
        .await;
    println!("complete {} ms", started.elapsed().as_millis());
    let _ = completion;

    let started = Instant::now();
    let version = service.execute(&session_id, "/version").await;
    println!("version {} ms", started.elapsed().as_millis());
    match version {
        CommandOutcome::Output { text, .. } => assert!(!text.is_empty()),
        other => panic!("{other:?}"),
    }

    let started = Instant::now();
    let worktree = service.execute(&session_id, "/worktree").await;
    println!("worktree {} ms", started.elapsed().as_millis());
    match worktree {
        CommandOutcome::Refused { message, .. } => {
            assert!(message.contains("not available on the shared Hermes gateway"));
            assert!(message.lines().nth(1).is_some());
        }
        other => panic!("{other:?}"),
    }

    let started = Instant::now();
    let unknown = service
        .execute(&session_id, "/nonexistent-talaria-check")
        .await;
    println!("unknown {} ms", started.elapsed().as_millis());
    match unknown {
        CommandOutcome::Refused { message, .. } => assert!(message.contains("does not know")),
        other => panic!("{other:?}"),
    }
}
