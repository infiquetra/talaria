//! Live comforts: alerts and Herdr state against a running gateway.
//!
//! Ignored unless a person asks for it. Prints `LIVE_COMFORTS_RESULT: PASSED` or
//! `SKIPPED (<reason>)`; with `TALARIA_LIVE_REQUIRE=1` a skip fails the test.
//! Behavior contract: issue 228.
//!
//! Run by hand: `TALARIA_LIVE_REQUIRE=1 cargo test -p talaria-cli --test live_comforts --
//! --ignored --nocapture --test-threads=1`.

mod support;

use std::path::{Path, PathBuf};
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
        let endpoint = discover_with(&home, &options)
            .await
            .map_err(|error| match error {
                DiscoveryError::NoGateway => {
                    "no Hermes gateway is running for this home".to_owned()
                }
                other => other.to_string(),
            })?;
        if let Some(reason) = hermes_contract::pinned::live_skip_reason(endpoint.code_sha()) {
            return Err(reason);
        }
        let profile = endpoint
            .resolve_profile("fixture")
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
            profile: Some("fixture".to_owned()),
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

/// The id after `talaria --resume ` in the resume hint.
fn resume_id(text: &str) -> String {
    let marker = "talaria --resume ";
    let start = text
        .find(marker)
        .unwrap_or_else(|| panic!("no resume hint in the output:\n{text}"))
        + marker.len();
    text[start..]
        .split_whitespace()
        .next()
        .unwrap_or_else(|| panic!("the resume hint has no id:\n{text}"))
        .to_owned()
}

/// Complete fake-Herdr runs: argument lists split on `--end--`.
fn read_runs(log: &Path) -> Vec<Vec<String>> {
    let Ok(text) = std::fs::read_to_string(log) else {
        return Vec::new();
    };
    let mut runs = Vec::new();
    let mut current = Vec::new();
    for line in text.lines() {
        if line == "--end--" {
            runs.push(std::mem::take(&mut current));
        } else {
            current.push(line.to_owned());
        }
    }
    runs
}

fn flag(run: &[String], name: &str) -> Option<String> {
    run.iter()
        .skip_while(|arg| arg.as_str() != name)
        .nth(1)
        .cloned()
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

    let fake = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../talaria-app/tests/support/fake_herdr.sh");
    assert!(fake.is_file(), "missing {}", fake.display());

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
    let log = home.path().join("herdr.log");

    // A clean environment: no config file, history under the temporary home, no TMUX.
    let mut terminal = Launch::new(home.path())
        .hermes_home(&hermes_home())
        .cwd(&project)
        .size(60, 160)
        .answer_queries()
        .env("HERDR_ENV", "1")
        .env("HERDR_PANE_ID", "p_live")
        .env(
            "HERDR_BIN_PATH",
            fake.to_str().expect("a printable fake path"),
        )
        .env(
            "FAKE_HERDR_LOG",
            log.to_str().expect("a printable log path"),
        )
        .args(&[
            "--no-ensure",
            "--profile",
            "fixture",
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
    let text = screen.text;
    let id = resume_id(&text);
    assert!(
        project.join(".pytest_cache/x").is_file(),
        "the denied command never ran"
    );

    let runs = read_runs(&log);
    assert!(!runs.is_empty(), "Herdr saw reports");
    let states: Vec<Option<String>> = runs.iter().map(|run| flag(run, "--state")).collect();
    assert!(states.contains(&Some("idle".to_owned())), "{runs:?}");
    assert!(states.contains(&Some("working".to_owned())), "{runs:?}");
    assert!(states.contains(&Some("blocked".to_owned())), "{runs:?}");

    let seqs: Vec<u64> = runs
        .iter()
        .filter_map(|run| flag(run, "--seq"))
        .map(|seq| seq.parse().expect("a numeric seq"))
        .collect();
    assert!(
        seqs.windows(2).all(|pair| pair[0] < pair[1]),
        "the sequence rises: {seqs:?}"
    );

    let with_session: Vec<&Vec<String>> = runs
        .iter()
        .filter(|run| flag(run, "--agent-session-id").as_deref() == Some(id.as_str()))
        .collect();
    assert!(!with_session.is_empty(), "a report carries the session id");
    assert!(
        with_session.iter().any(|run| {
            let tail: Vec<&str> = run
                .iter()
                .skip_while(|arg| arg.as_str() != "--")
                .map(|arg| arg.as_str())
                .collect();
            tail == [
                "--",
                "talaria",
                "--resume",
                id.as_str(),
                "--profile",
                "fixture",
            ]
        }),
        "a session report carries the resume list: {runs:?}"
    );

    let blocked = runs
        .iter()
        .find(|run| flag(run, "--state").as_deref() == Some("blocked"))
        .expect("a blocked report");
    assert!(
        flag(blocked, "--message")
            .as_deref()
            .is_some_and(|message| message.starts_with("approval:")),
        "the blocked report carries the approval message: {blocked:?}"
    );
    let idle_after_blocked = runs
        .iter()
        .skip_while(|run| flag(run, "--state").as_deref() != Some("blocked"))
        .skip(1)
        .any(|run| flag(run, "--state").as_deref() == Some("idle"));
    assert!(idle_after_blocked, "idle follows the denial: {runs:?}");

    let releases = runs
        .iter()
        .filter(|run| run.iter().any(|arg| arg == "release-agent"))
        .count();
    assert_eq!(releases, 1, "one release on exit: {runs:?}");

    println!("LIVE_COMFORTS_RESULT: PASSED");
}
