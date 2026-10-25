//! Live bypass and move against a running Hermes gateway. Ignored unless a person asks
//! for it. A skip is not a pass: with `TALARIA_LIVE_REQUIRE=1` a skip fails the test.
//!
//! Run by hand, one test at a time, with the gateway of the home named by `HERMES_HOME` and
//! profile `fixture` at approval mode `manual`:
//! `TALARIA_LIVE_REQUIRE=1 cargo test --release -p talaria-cli --test live_permissions -- --ignored
//! --nocapture --test-threads=1`

use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use hermes_client::{
    CallOptions, DiscoverOptions, EnsurePolicy, Inbound, Purpose, Scope, default_home,
    discover_with,
};
use serde_json::{Map, Value, json};
use talaria_app::launch::{NewSession, SessionRequest};
use talaria_app::mutation::MutationClient;
use talaria_app::permissions::PermissionsService;
use talaria_app::sessions::SessionService;
use talaria_app::workspace::WorkspaceService;
use talaria_core::mutation::{MutationOutcome, MutationRequest};
use talaria_core::permissions::{ApprovalMode, PermissionsEffect, PermissionsInput};
use talaria_core::workspace::{CdEffect, CdInput, FolderCheck};

fn hermes_home() -> PathBuf {
    std::env::var_os("HERMES_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(default_home)
}

/// The commit the running gateway reports, read through discovery that never starts a gateway.
async fn gateway_code_sha(home: &Path) -> Result<Option<String>, String> {
    let options = DiscoverOptions {
        control_timeout: Duration::from_secs(5),
        ensure: EnsurePolicy::Never,
    };
    discover_with(home, &options)
        .await
        .map(|endpoint| endpoint.code_sha().map(str::to_owned))
        .map_err(|error| error.to_string())
}

fn skip(reason: String) {
    println!("LIVE_PERMISSIONS_RESULT: SKIPPED ({reason})");
    if std::env::var("TALARIA_LIVE_REQUIRE").ok().as_deref() == Some("1") {
        panic!("live test skipped: {reason}");
    }
}

fn git_init(folder: &Path) {
    let status = Command::new("git")
        .arg("init")
        .arg("--quiet")
        .arg(folder)
        .status()
        .expect("git init runs");
    assert!(status.success(), "git init in {}", folder.display());
}

fn now_nanos() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|span| span.as_nanos())
        .unwrap_or(0)
}

fn unique_request_id() -> String {
    format!("live{}-cd-m1", now_nanos())
}

/// Hermes keeps every `session.create` request id with its request, and refuses the same id
/// with another folder, so each run uses its own launch id.
fn unique_launch_id() -> String {
    format!("live-permissions-{}", now_nanos())
}

/// The same folder, as Hermes reports it or after resolving symbolic links: macOS keeps
/// temporary folders behind a symbolic link.
fn same_folder(reported: &str, expected: &str) -> bool {
    reported == expected
        || matches!(
            (std::fs::canonicalize(reported), std::fs::canonicalize(expected)),
            (Ok(reported), Ok(expected)) if reported == expected
        )
}

#[tokio::test]
#[ignore]
async fn live_session_bypass_and_move() {
    let home = hermes_home();
    match gateway_code_sha(&home).await {
        Err(reason) => {
            skip(reason);
            return;
        }
        Ok(sha) => {
            if let Some(reason) = hermes_contract::pinned::live_skip_reason(sha.as_deref()) {
                skip(reason);
                return;
            }
        }
    }
    let options = DiscoverOptions {
        ensure: EnsurePolicy::Never,
        ..DiscoverOptions::default()
    };
    let endpoint = discover_with(&home, &options)
        .await
        .expect("the operator gateway answers");
    let profile = endpoint
        .resolve_profile("fixture")
        .expect("profile fixture exists");
    let ticket = hermes_client::mint_ticket(
        &home,
        &endpoint,
        &profile,
        Purpose::Interactive,
        Scope::profile(),
    )
    .await
    .expect("a ticket");
    let mut connection = hermes_client::connect(&endpoint, ticket)
        .await
        .expect("a connection");
    hermes_client::handshake(&mut connection)
        .await
        .expect("a handshake");
    let (client, mut inbound) = hermes_client::Client::start(connection.into_parts());

    // The operator precondition: profile fixture reads manual.
    let mode = client
        .call_raw(
            "config.get",
            json!({"key": "approvals.mode"}),
            CallOptions { timeout: None },
        )
        .await
        .expect("the mode reads");
    if mode.get("value") != Some(&Value::String("manual".to_owned())) {
        client.close().await;
        skip(format!(
            "profile fixture reads {}, want manual",
            mode.get("value").unwrap_or(&Value::Null)
        ));
        return;
    }

    let launch = unique_launch_id();
    let sessions = SessionService::with_client(&launch, client.clone());
    let permissions = PermissionsService::new(std::sync::Arc::new(client.clone()));
    let workspace = WorkspaceService::new(MutationClient::new(client.clone()));

    let dir_a = tempfile::TempDir::new().expect("a folder");
    let dir_b = tempfile::TempDir::new().expect("a folder");
    git_init(dir_a.path());
    git_init(dir_b.path());
    let folder_b = dir_b.path().to_string_lossy().into_owned();

    let opened = sessions
        .open(SessionRequest::New(NewSession {
            request_id: format!("{launch}-create"),
            source: "tui".to_owned(),
            cwd: dir_a.path().to_path_buf(),
            model: None,
            provider: None,
            reasoning: None,
        }))
        .await
        .expect("a session");
    let session_id = opened.session_id.clone();

    let body = async {
        // The bypass starts off.
        let input = permissions
            .execute(PermissionsEffect::ReadSessionOff {
                session_id: session_id.clone(),
                generation: 0,
            })
            .await;
        let PermissionsInput::SessionOffRead { result, .. } = input.expect("a bypass read") else {
            panic!("a bypass read feeds back a bypass read");
        };
        assert_eq!(result, Ok(false), "the bypass starts off");

        // Both settings read.
        let input = permissions
            .execute(PermissionsEffect::ReadProfileMode { generation: 1 })
            .await;
        assert!(
            matches!(
                input,
                Some(PermissionsInput::ProfileModeRead {
                    result: Ok(ApprovalMode::Manual),
                    ..
                })
            ),
            "the profile reads manual"
        );

        // The bypass switches on and back off.
        for (off, generation) in [(true, 2), (false, 3)] {
            let input = permissions
                .execute(PermissionsEffect::SetSessionOff {
                    session_id: session_id.clone(),
                    generation,
                    off,
                })
                .await;
            let PermissionsInput::SessionOffSet { result, .. } = input.expect("a bypass write")
            else {
                panic!("a bypass write feeds back a bypass write");
            };
            assert_eq!(result, Ok(off), "the bypass switches");
            let input = permissions
                .execute(PermissionsEffect::ReadSessionOff {
                    session_id: session_id.clone(),
                    generation: generation + 10,
                })
                .await;
            let PermissionsInput::SessionOffRead { result, .. } = input.expect("a reread") else {
                panic!("a reread feeds back a reread");
            };
            assert_eq!(result, Ok(off), "the bypass rereads");
        }

        // The session moves to the second folder.
        let input = workspace
            .execute(CdEffect::CheckFolder {
                session_id: session_id.clone(),
                target: folder_b.clone().into(),
            })
            .await;
        assert!(
            matches!(
                input,
                Some(CdInput::Checked {
                    check: FolderCheck::Directory,
                    ..
                })
            ),
            "the target is a folder"
        );
        let snapshot = client
            .call_raw(
                "session.resume",
                json!({"session_id": session_id}),
                CallOptions { timeout: None },
            )
            .await
            .expect("a snapshot");
        let mut payload = Map::new();
        payload.insert("cwd".to_owned(), Value::String(folder_b.clone()));
        let request = MutationRequest {
            session_id: session_id.clone(),
            request_id: unique_request_id(),
            expected_revision: snapshot["revision"].as_i64().expect("a revision"),
            expected_generation: snapshot["execution_generation"].as_i64(),
            operation: "workspace".to_owned(),
            payload,
        };
        let input = workspace.execute(CdEffect::Send(request)).await;
        let CdInput::Outcome { outcome, .. } = input.expect("a move outcome") else {
            panic!("a send feeds back an outcome");
        };
        let MutationOutcome::Committed(result) = outcome else {
            panic!("the move commits, got {outcome:?}");
        };
        let moved_to = result["cwd"].as_str().unwrap_or_default();
        assert!(
            same_folder(moved_to, &folder_b),
            "the move reports {moved_to}, want {folder_b}"
        );
        // Hermes publishes the move as `session.updated` (its result) and, when it owns the
        // session's projection, as `session.info`. An earlier event may still be buffered from
        // the open, and heartbeats keep the stream busy, so wait for this session's new folder
        // in either under one deadline.
        let deadline = tokio::time::Instant::now() + Duration::from_secs(30);
        let mut seen = Vec::new();
        loop {
            let Ok(item) = tokio::time::timeout_at(deadline, inbound.next()).await else {
                panic!("no session event with {folder_b} within 30 s; saw {seen:?}");
            };
            let item = item.expect("the stream stays open");
            if let Inbound::Event(event) = item
                && (event.raw["type"] == "session.info" || event.raw["type"] == "session.updated")
                && event.raw["session_id"] == session_id
            {
                let cwd = event.raw["payload"]["cwd"].as_str().unwrap_or_default();
                if same_folder(cwd, &folder_b) {
                    break;
                }
                seen.push(cwd.to_owned());
            }
        }
        Ok::<(), String>(())
    };
    let held = body.await;

    // Whatever happened, the bypass is restored, then the session detaches and the
    // connection closes; the session stays in Hermes.
    let input = permissions
        .execute(PermissionsEffect::SetSessionOff {
            session_id: session_id.clone(),
            generation: 100,
            off: false,
        })
        .await;
    let PermissionsInput::SessionOffSet { result, .. } = input.expect("a restore") else {
        panic!("a restore feeds back a restore");
    };
    assert_eq!(result, Ok(false), "the bypass is restored");
    let _ = sessions.detach(&opened).await;
    client.close().await;
    held.expect("the body passes");

    // The profile still reads manual afterwards.
    let mut connection = hermes_client::connect(
        &endpoint,
        hermes_client::mint_ticket(
            &home,
            &endpoint,
            &profile,
            Purpose::Interactive,
            Scope::profile(),
        )
        .await
        .expect("a ticket"),
    )
    .await
    .expect("a connection");
    hermes_client::handshake(&mut connection)
        .await
        .expect("a handshake");
    let (client, _) = hermes_client::Client::start(connection.into_parts());
    let mode = client
        .call_raw(
            "config.get",
            json!({"key": "approvals.mode"}),
            CallOptions { timeout: None },
        )
        .await
        .expect("the mode rereads");
    let value = mode.get("value").cloned().unwrap_or(Value::Null);
    println!("LIVE_PERMISSIONS_PROFILE_MODE: {value}");
    assert_eq!(value, Value::String("manual".to_owned()));
    client.close().await;

    println!("LIVE_PERMISSIONS_RESULT: PASSED");
}
