//! Live comforts: alerts against a running gateway.
//!
//! Ignored unless a person asks for it. Prints `LIVE_COMFORTS_RESULT: PASSED` or
//! `SKIPPED (<reason>)`; with `TALARIA_LIVE_REQUIRE=1` a skip fails the test.
//! Behavior contract: issue 228.
//!
//! Run by hand: `TALARIA_LIVE_REQUIRE=1 cargo test -p talaria-cli --test integration live_comforts:: --
//! --ignored --nocapture --test-threads=1`.

use crate::support;

use std::path::PathBuf;
use std::process::Command;
use std::time::{Duration, Instant};

use hermes_client::{
    Client, DiscoverOptions, DiscoveryError, EnsurePolicy, ProfileLookupError, Purpose, Scope,
    connect, default_home, discover_with, handshake, mint_ticket,
};
use hermes_contract::methods::canonical::config_get;
use hermes_contract::schemas::ConfigGetParams;
use support::{Launch, Terminal};

const READY_ASK: &str = "Reply with the single word ready.";
const RM_ASK: &str =
    "Run this exact shell command and then tell me its exit code: rm -rf .pytest_cache";
const TURN_LIMIT: Duration = Duration::from_secs(120);

fn hermes_home() -> PathBuf {
    std::env::var_os("HERMES_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(default_home)
}

/// Print the skip line, fail when a live run is required, and report not-ready.
fn skip(reason: &str) -> bool {
    println!("LIVE_COMFORTS_RESULT: SKIPPED ({reason})");
    if std::env::var("TALARIA_LIVE_REQUIRE").ok().as_deref() == Some("1") {
        panic!("live test skipped: {reason}");
    }
    false
}

/// A client on the running gateway, or the reason it cannot be reached.
fn attach() -> Result<(tokio::runtime::Runtime, Client), String> {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|error| error.to_string())?;
    let client = runtime.block_on(async {
        let home = hermes_home();
        let options = DiscoverOptions {
            control_timeout: Duration::from_secs(5),
            ensure: EnsurePolicy::Never,
        };
        let endpoint = discover_with(&home, &options).await.map_err(|error| {
            println!(
                "{}",
                hermes_contract::pinned::gateway_version_line(None, None)
            );
            match error {
                DiscoveryError::NoGateway => {
                    "no Hermes gateway is running for this home".to_owned()
                }
                other => other.to_string(),
            }
        })?;
        println!(
            "{}",
            hermes_contract::pinned::gateway_version_line(
                endpoint.code_version(),
                endpoint.code_sha()
            )
        );
        if let Some(reason) = hermes_contract::pinned::live_test_skip(
            endpoint.code_version(),
            endpoint.code_sha(),
            hermes_contract::pinned::live_exact(),
        ) {
            return Err(reason);
        }
        let profile = endpoint
            .resolve_profile(&hermes_contract::pinned::live_profile())
            .map_err(|error| match error {
                ProfileLookupError::NeverDiscovered { name } => {
                    format!("the gateway never discovered a profile named {name}")
                }
                other => other.to_string(),
            })?;
        let ticket = mint_ticket(
            &home,
            &endpoint,
            &profile,
            Purpose::Interactive,
            Scope::profile(),
        )
        .await
        .map_err(|error| error.to_string())?;
        let mut connection = connect(&endpoint, ticket)
            .await
            .map_err(|error| error.to_string())?;
        handshake(&mut connection)
            .await
            .map_err(|error| error.to_string())?;
        Ok(Client::start(connection.into_parts()).0)
    })?;
    Ok((runtime, client))
}

/// The `approvals.mode` serving `fixture`, or the reason it is unknown.
fn approvals_mode(runtime: &tokio::runtime::Runtime, client: &Client) -> Result<String, String> {
    runtime.block_on(async {
        let params = ConfigGetParams {
            key: "approvals.mode".to_owned(),
            profile: Some(hermes_contract::pinned::live_profile()),
            cwd: None,
            session_id: None,
        };
        let result = client
            .call::<config_get::Marker>(params)
            .await
            .map_err(|error| format!("config.get approvals.mode failed: {error}"))?;
        result
            .value
            .ok_or_else(|| "config.get approvals.mode failed: no value".to_owned())
    })
}

fn type_line(terminal: &mut Terminal, text: &str) {
    terminal.type_keys(text);
    std::thread::sleep(Duration::from_millis(250));
    terminal.type_keys("\r");
}

/// Wait until the status line has not shown `working` for this long.
fn wait_idle(terminal: &mut Terminal, quiet: Duration, limit: Duration) -> bool {
    let deadline = Instant::now() + limit;
    let mut idle_since: Option<Instant> = None;
    while Instant::now() < deadline {
        let screen = terminal.screen_text();
        let status = screen.lines().next_back().unwrap_or_default();
        if status
            .split('│')
            .any(|segment| segment.trim().ends_with(" working"))
        {
            idle_since = None;
        } else {
            let since = *idle_since.get_or_insert_with(Instant::now);
            if since.elapsed() >= quiet {
                return true;
            }
        }
        if terminal.exited() {
            return false;
        }
        std::thread::sleep(Duration::from_millis(200));
    }
    false
}

#[ignore = "needs a running gateway"]
#[test]
fn live_alerts_and_herdr_state() {
    if !Command::new("git")
        .arg("--version")
        .output()
        .is_ok_and(|output| output.status.success())
    {
        skip("git is not installed");
        return;
    }
    let (runtime, client) = match attach() {
        Ok(client) => client,
        Err(reason) => {
            skip(&reason);
            return;
        }
    };
    let mode = match approvals_mode(&runtime, &client) {
        Ok(mode) => mode,
        Err(reason) => {
            skip(&reason);
            return;
        }
    };
    if mode != "manual" {
        skip(&format!("approvals.mode is {mode}, not manual"));
        return;
    }

    let home = tempfile::tempdir().unwrap();
    let project = home.path().join("proj");
    std::fs::create_dir(&project).unwrap();
    assert!(
        Command::new("git")
            .args(["init", "-q"])
            .current_dir(&project)
            .status()
            .is_ok_and(|status| status.success()),
        "a temporary git project"
    );

    // A clean environment: no config file, history under the temporary home, no TMUX.
    let mut terminal = Launch::new(home.path())
        .hermes_home(&hermes_home())
        .cwd(&project)
        .size(60, 160)
        .answer_queries()
        .args(&[
            "--no-ensure",
            "--profile",
            &hermes_contract::pinned::live_profile(),
            "--provider",
            "ollama-cloud",
            "--model",
            "minimax-m3.1-flash",
        ])
        .spawn();
    assert!(
        terminal.wait_first_frame(Duration::from_secs(20)).is_some(),
        "the first frame draws"
    );

    type_line(&mut terminal, READY_ASK);
    assert!(
        terminal.wait_for_text("ready", TURN_LIMIT),
        "the first turn answers"
    );
    assert!(wait_idle(&mut terminal, Duration::from_secs(2), TURN_LIMIT));
    let first_turn_bytes = terminal.raw_output().len();
    assert!(
        !terminal.raw_output()[..first_turn_bytes]
            .windows(4)
            .any(|window| window == [0x1b, b']', b'9', b';']),
        "no desktop notification during the short first turn"
    );

    std::fs::create_dir_all(project.join(".pytest_cache")).unwrap();
    std::fs::write(project.join(".pytest_cache/x"), "keep\n").unwrap();
    type_line(&mut terminal, RM_ASK);
    assert!(
        terminal.wait_for_text("deny", TURN_LIMIT),
        "the approval card shows"
    );
    std::thread::sleep(Duration::from_millis(500));
    terminal.type_keys("\u{1b}");
    assert!(wait_idle(&mut terminal, Duration::from_secs(2), TURN_LIMIT));
    let raw = terminal.raw_output();
    let second = &raw[first_turn_bytes..];
    let bell = second.iter().position(|byte| *byte == 0x07);
    let osc = second
        .windows(4)
        .position(|window| window == [0x1b, b']', b'9', b';']);
    let (Some(bell), Some(osc)) = (bell, osc) else {
        panic!("a bell and a desktop notification follow the approval");
    };
    assert!(bell < osc, "the bell comes first");
    assert!(
        String::from_utf8_lossy(&second[osc..]).contains("Talaria: needs you: approval:"),
        "the notification names the approval"
    );

    type_line(&mut terminal, "/quit");
    let screen = terminal.finish(Duration::from_secs(20));
    assert_eq!(screen.exit_code, Some(0), "{}", screen.text);
    assert!(
        project.join(".pytest_cache/x").is_file(),
        "the denied command never ran"
    );

    println!("LIVE_COMFORTS_RESULT: PASSED");
}
