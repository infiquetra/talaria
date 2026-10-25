//! Live rewind against a running Hermes gateway. Ignored unless a person asks for it.
//!
//! A skip is not a pass. With `TALARIA_LIVE_REQUIRE=1` a skip fails the test.
//! The first call is `rewind_turns`, so an ignored run fails in the stub until that function
//! is implemented. Issue 233. Checkpoint rewind and diff.

use std::process::Command;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use hermes_client::{
    CallOptions, Client, DiscoverOptions, DiscoveryError, EnsurePolicy, ProfileLookupError,
    Purpose, Scope, connect, default_home, discover_with, mint_ticket,
};
use serde_json::json;
use talaria_app::launch::{NewSession, SessionRequest};
use talaria_app::rewind::RewindService;
use talaria_app::sessions::SessionService;
use talaria_core::rewind::{
    RewindEffect, RewindInput, RewindKey, RewindScreen, RewindState, rewind_turns, update,
};
use talaria_core::session::{Effect, SessionId};

const PROMPT_1: &str = "Create a file named notes.txt containing exactly the line alpha. Use your file writing tool, not the terminal. Reply with the word done.";
const PROMPT_2: &str = "In notes.txt replace alpha with beta using your patch or file tool, not the terminal. Reply with the word done.";

fn report_skip(reason: &str) {
    println!("LIVE_REWIND_RESULT: SKIPPED ({reason})");
    if std::env::var("TALARIA_LIVE_REQUIRE").ok().as_deref() == Some("1") {
        panic!("live test skipped: {reason}");
    }
}

fn passed() {
    println!("LIVE_REWIND_RESULT: PASSED");
}

#[tokio::test]
#[ignore]
async fn live_rewind_restore_and_diff() {
    let _ = rewind_turns(&[]);
    let home = default_home();
    let options = DiscoverOptions {
        control_timeout: Duration::from_secs(5),
        ensure: EnsurePolicy::Never,
    };
    let endpoint = match discover_with(&home, &options).await {
        Ok(endpoint) => endpoint,
        Err(DiscoveryError::NoGateway) => {
            report_skip("no gateway");
            return;
        }
        Err(error) => panic!("discovery failed: {error}"),
    };
    let profile = match endpoint.resolve_profile("fixture") {
        Ok(profile) => profile,
        Err(ProfileLookupError::NeverDiscovered { .. }) => {
            report_skip("fixture not served");
            return;
        }
        Err(error) => panic!("fixture lookup failed: {error}"),
    };
    if let Some(reason) = hermes_contract::pinned::live_skip_reason(endpoint.code_sha()) {
        report_skip(&reason);
        return;
    }
    let ticket = mint_ticket(
        &home,
        &endpoint,
        &profile,
        Purpose::Interactive,
        Scope::profile(),
    )
    .await
    .unwrap_or_else(|error| panic!("ticket failed: {error}"));
    let connection = connect(&endpoint, ticket)
        .await
        .unwrap_or_else(|error| panic!("connect failed: {error}"));
    let (client, _inbound) = Client::start(connection.into_parts());
    let folder = tempfile::tempdir().expect("folder");
    let init = Command::new("git")
        .arg("init")
        .current_dir(folder.path())
        .status()
        .expect("git init");
    assert!(init.success(), "git init failed");
    std::fs::write(folder.path().join("README.md"), "seed\n").expect("seed file");
    let service = SessionService::with_client("live-rewind", client.clone());
    let opened = service
        .open(SessionRequest::New(NewSession {
            request_id: "0123456789abcdef0123456789abcdef".to_string(),
            source: "tui".to_string(),
            cwd: folder.path().to_path_buf(),
            model: Some("minimax-m3.1-flash".to_string()),
            provider: Some("ollama-cloud".to_string()),
            reasoning: None,
        }))
        .await
        .unwrap_or_else(|error| panic!("open failed: {error:?}"));
    send_prompt(&service, &opened.session_id, PROMPT_1, "1").await;
    wait_idle(&client, &opened.session_id).await;
    send_prompt(&service, &opened.session_id, PROMPT_2, "2").await;
    wait_idle(&client, &opened.session_id).await;
    let notes = std::fs::read_to_string(folder.path().join("notes.txt")).unwrap_or_default();
    assert_eq!(
        notes.trim(),
        "beta",
        "model did not write the file: {notes}"
    );
    let listed = client
        .call_raw(
            "rollback.list",
            json!({"session_id": opened.session_id}),
            CallOptions {
                timeout: Some(Duration::from_secs(120)),
            },
        )
        .await
        .unwrap_or_else(|error| panic!("rollback.list failed: {error}"));
    if listed.get("enabled") == Some(&serde_json::Value::Bool(false)) {
        report_skip("checkpoints off");
        return;
    }
    let rows = listed["checkpoints"].as_array().map_or(0, Vec::len);
    assert!(
        rows >= 2,
        "rollback.list holds {rows} checkpoints, expected at least two"
    );
    let session_diff = client
        .call_raw(
            "rollback.session_diff",
            json!({"session_id": opened.session_id}),
            CallOptions {
                timeout: Some(Duration::from_secs(240)),
            },
        )
        .await
        .unwrap_or_else(|error| panic!("rollback.session_diff failed: {error}"));
    let mentioned = ["stat", "diff"].iter().any(|key| {
        session_diff[*key]
            .as_str()
            .is_some_and(|text| text.contains("notes.txt"))
    });
    assert!(mentioned, "the session diff does not mention notes.txt");

    // Drive the core directly: open, the newest prompt (prompt 2) is selected, f, Enter.
    let rewind = RewindService::new(client.clone(), opened.session_id.clone());
    let mut state = RewindState::default();
    let opened_overlay = update(
        &mut state,
        RewindInput::Open {
            turn_running: false,
        },
        now_ms(),
    );
    let mut effects = drive(&rewind, &mut state, opened_overlay).await;
    assert_eq!(
        state.screen(),
        RewindScreen::Turns,
        "the prompt list did not open"
    );
    for (key, screen) in [
        (RewindKey::Enter, RewindScreen::Actions),
        (RewindKey::F, RewindScreen::Confirm),
        (RewindKey::Enter, RewindScreen::Closed),
    ] {
        let produced = update(&mut state, RewindInput::Key(key), now_ms());
        effects.extend(drive(&rewind, &mut state, produced).await);
        assert_eq!(
            state.screen(),
            screen,
            "after {key:?}: {:?}",
            state.view(now_ms()).body
        );
    }
    let notes = std::fs::read_to_string(folder.path().join("notes.txt")).unwrap_or_default();
    assert_eq!(
        notes.trim(),
        "alpha",
        "the restore did not put notes.txt back: {notes}"
    );
    let snapshot = client
        .call_raw(
            "session.resume",
            json!({"session_id": opened.session_id, "display_projection": true}),
            CallOptions {
                timeout: Some(Duration::from_secs(120)),
            },
        )
        .await
        .unwrap_or_else(|error| panic!("resume failed: {error}"));
    let messages = snapshot["messages"].as_array().cloned().unwrap_or_default();
    assert_eq!(
        rewind_turns(&messages).len(),
        1,
        "a fresh read should hold only prompt 1"
    );
    let returned = effects.iter().find_map(|effect| match effect {
        RewindEffect::ReturnPrompt { text, .. } => Some(text.as_str()),
        _ => None,
    });
    assert_eq!(
        returned,
        Some(PROMPT_2),
        "the rewound prompt was not returned"
    );
    passed();
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |elapsed| {
            u64::try_from(elapsed.as_millis()).unwrap_or(u64::MAX)
        })
}

/// Run each call the state asks for through the service and feed the reply back, until no
/// call is left. Returns every effect, in order.
async fn drive(
    service: &RewindService,
    state: &mut RewindState,
    first: Vec<RewindEffect>,
) -> Vec<RewindEffect> {
    let mut seen = Vec::new();
    let mut pending = first;
    loop {
        let next = pending.iter().find_map(|effect| match effect {
            RewindEffect::Call { ticket, call } => Some((*ticket, call.clone())),
            _ => None,
        });
        seen.append(&mut pending);
        let Some((ticket, call)) = next else {
            return seen;
        };
        let reply = service.execute(ticket, call).await;
        pending = update(state, reply, now_ms());
    }
}

async fn send_prompt(service: &SessionService, session_id: &str, text: &str, n: &str) {
    let outcome = service
        .execute(Effect::SendPrompt {
            session_id: SessionId::new(session_id),
            text: text.to_string(),
            submission_id: format!("talaria-live-rewind-{n}"),
        })
        .await;
    assert!(
        matches!(outcome, talaria_app::sessions::ExecuteOutcome::Sent),
        "{outcome:?}"
    );
}

async fn wait_idle(client: &Client, session_id: &str) {
    let deadline = Instant::now() + Duration::from_secs(180);
    loop {
        let snapshot = client
            .call_raw(
                "session.resume",
                json!({"session_id": session_id, "display_projection": true}),
                CallOptions {
                    timeout: Some(Duration::from_secs(120)),
                },
            )
            .await
            .unwrap_or_else(|error| panic!("resume failed: {error}"));
        let running = snapshot.get("running") == Some(&serde_json::Value::Bool(true));
        let pending = snapshot
            .get("pending")
            .and_then(|value| value.as_array())
            .map(|rows| rows.len())
            .unwrap_or(0);
        if !running && pending == 0 {
            return;
        }
        if Instant::now() >= deadline {
            panic!(
                "the turn did not end within 180 s (running={running}, pending={pending}). Check for an approval request."
            );
        }
        tokio::time::sleep(Duration::from_secs(1)).await;
    }
}
