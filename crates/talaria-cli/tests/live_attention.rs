//! Live fleet attention against a running gateway.
//!
//! Ignored unless someone asks for it. Prints `LIVE_ATTENTION_RESULT: PASSED` or
//! `SKIPPED (<reason>)`. With `TALARIA_LIVE_REQUIRE=1` a skip fails the test.
//!
//! Run by hand: `TALARIA_LIVE_REQUIRE=1 cargo test -p talaria-cli --test live_attention --
//! --ignored --nocapture --test-threads=1`.

mod support;

use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

use hermes_client::{
    CallOptions, Client, DiscoverOptions, DiscoveryError, EnsurePolicy, ProfileLookupError,
    Purpose, Scope, connect, default_home, discover_with, handshake, mint_ticket,
};
use hermes_contract::methods::canonical::config_get;
use hermes_contract::schemas::ConfigGetParams;
use serde_json::{Value, json};
use support::{Launch, Terminal};

const PROVIDER: &str = "ollama-cloud";
const MODEL: &str = "minimax-m3.1-flash";
const APPROVAL_ASK: &str = "Run this exact shell command: rm -rf ./talaria-attention-probe";
const DONE_ASK: &str = "Run this exact shell command: sleep 30 && echo attention-done";
const ALERT_MARK: &[u8] = &[0x07, 0x1b, b']', b'9', b';'];

fn hermes_home() -> PathBuf {
    std::env::var_os("HERMES_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(default_home)
}

fn scrub(reason: &str) -> String {
    reason
        .split_whitespace()
        .map(|word| {
            if word.contains('/') || word.contains('\\') {
                "<path>"
            } else {
                word
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn skip(reason: &str) -> bool {
    println!("LIVE_ATTENTION_RESULT: SKIPPED ({})", scrub(reason));
    if std::env::var("TALARIA_LIVE_REQUIRE").ok().as_deref() == Some("1") {
        panic!("live test skipped: {}", scrub(reason));
    }
    false
}

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

fn catalogue_has_model(runtime: &tokio::runtime::Runtime, client: &Client) -> Result<(), String> {
    runtime.block_on(async {
        let listed = client
            .call_raw(
                "model.options",
                json!({"explicit_only": true}),
                CallOptions::default(),
            )
            .await
            .map_err(|error| format!("model.options failed: {error}"))?;
        let providers = listed
            .get("providers")
            .and_then(Value::as_array)
            .ok_or_else(|| "model.options returned no providers".to_owned())?;
        let row = providers
            .iter()
            .find(|row| row.get("slug").and_then(Value::as_str) == Some(PROVIDER))
            .ok_or_else(|| format!("{PROVIDER} is absent from the catalogue"))?;
        let models = row
            .get("models")
            .and_then(Value::as_array)
            .ok_or_else(|| format!("{PROVIDER} has no model list"))?;
        if models.iter().any(|model| model.as_str() == Some(MODEL)) {
            Ok(())
        } else {
            Err(format!("{MODEL} is absent from {PROVIDER}"))
        }
    })
}

fn launch(home: &Path, project: &Path) -> Terminal {
    Launch::new(home)
        .hermes_home(&hermes_home())
        .cwd(project)
        .size(48, 160)
        .answer_queries()
        .args(&[
            "--no-ensure",
            "--profile",
            "fixture",
            "--provider",
            PROVIDER,
            "--model",
            MODEL,
        ])
        .spawn()
}

fn type_line(terminal: &mut Terminal, text: &str) {
    terminal.type_keys(text);
    std::thread::sleep(Duration::from_millis(250));
    terminal.type_keys("\r");
}

fn status_count(screen: &str, mark: char) -> u32 {
    let status = screen.lines().next_back().unwrap_or("");
    for piece in status.split('\u{2502}') {
        let piece = piece.trim();
        let Some(rest) = piece.strip_prefix(mark) else {
            continue;
        };
        if let Some(count) = rest
            .split_whitespace()
            .next()
            .and_then(|word| word.parse().ok())
        {
            return count;
        }
    }
    0
}

fn alert_hits(raw: &[u8]) -> Vec<usize> {
    raw.windows(ALERT_MARK.len())
        .enumerate()
        .filter_map(|(index, window)| (window == ALERT_MARK).then_some(index))
        .collect()
}

fn git_project(root: &Path) -> PathBuf {
    let project = root.join("proj");
    std::fs::create_dir(&project).expect("project folder");
    let status = Command::new("git")
        .args(["init", "-q"])
        .current_dir(&project)
        .status();
    assert!(
        status.is_ok_and(|status| status.success()),
        "a temporary git project"
    );
    std::fs::create_dir(project.join("talaria-attention-probe")).expect("probe folder");
    project
}

fn ready(terminal: &mut Terminal) -> bool {
    terminal.wait_for_text(MODEL, Duration::from_secs(30))
}

fn preconditions() -> Option<(tokio::runtime::Runtime, Client)> {
    if Command::new("git").arg("--version").output().is_err() {
        skip("git is not available");
        return None;
    }
    let (runtime, client) = match attach() {
        Ok(pair) => pair,
        Err(reason) => {
            skip(&reason);
            return None;
        }
    };
    if let Err(reason) = catalogue_has_model(&runtime, &client) {
        skip(&reason);
        return None;
    }
    match approvals_mode(&runtime, &client) {
        Ok(mode) if mode == "manual" => Some((runtime, client)),
        Ok(mode) => {
            skip(&format!("approvals.mode is {mode}, not manual"));
            None
        }
        Err(reason) => {
            skip(&reason);
            None
        }
    }
}

#[test]
#[ignore = "needs a running gateway"]
fn live_background_approval_rings_once_and_counts() {
    let Some((_runtime, _client)) = preconditions() else {
        return;
    };
    let home = tempfile::tempdir().expect("temporary home");
    let project = git_project(home.path());
    let mut first = launch(home.path(), &project);
    assert!(ready(&mut first), "the first composer draws");
    std::thread::sleep(Duration::from_secs(16));
    let baseline = status_count(&first.screen_text(), '\u{2691}');

    let mut second = launch(home.path(), &project);
    assert!(ready(&mut second), "the second composer draws");
    type_line(&mut second, APPROVAL_ASK);
    assert!(
        second.wait_for_text("deny", Duration::from_secs(120)),
        "the approval card shows"
    );
    let card_at = Instant::now();
    type_line(&mut second, "/quit");
    let screen = second.finish(Duration::from_secs(20));
    let resume = resume_id(&screen.text);

    let deadline = Instant::now() + Duration::from_secs(30);
    let mut matched = false;
    while Instant::now() < deadline {
        let raw = first.raw_output();
        let hits = alert_hits(&raw);
        if hits.len() == 1 {
            let body = String::from_utf8_lossy(&raw[hits[0]..]);
            if body.contains("Talaria: ")
                && body.contains("needs you: approval: rm -rf ./talaria-attention-probe")
            {
                matched = true;
                break;
            }
        }
        if status_count(&first.screen_text(), '\u{2691}') == baseline + 1 && hits.len() == 1 {
            matched = true;
            break;
        }
        std::thread::sleep(Duration::from_millis(200));
    }
    let raw = first.raw_output();
    let hits = alert_hits(&raw);
    assert_eq!(hits.len(), 1, "one desktop alert");
    let body = String::from_utf8_lossy(&raw[hits[0]..]);
    assert!(body.contains("Talaria: "), "{body}");
    assert!(
        body.contains("needs you: approval: rm -rf ./talaria-attention-probe"),
        "{body}"
    );
    assert!(matched);
    assert_eq!(status_count(&first.screen_text(), '\u{2691}'), baseline + 1);
    println!(
        "latency_ms={}",
        Instant::now()
            .saturating_duration_since(card_at)
            .as_millis()
    );

    let mut third = Launch::new(home.path())
        .hermes_home(&hermes_home())
        .cwd(&project)
        .size(48, 160)
        .answer_queries()
        .args(&["--no-ensure", "--profile", "fixture", "--resume", &resume])
        .spawn();
    assert!(
        third.wait_for_text("deny", Duration::from_secs(60)),
        "the resumed approval card shows"
    );
    third.type_keys("\u{1b}");
    type_line(&mut third, "/quit");
    let _ = third.finish(Duration::from_secs(20));

    let back = Instant::now() + Duration::from_secs(10);
    while Instant::now() < back && status_count(&first.screen_text(), '\u{2691}') != baseline {
        std::thread::sleep(Duration::from_millis(200));
    }
    assert_eq!(status_count(&first.screen_text(), '\u{2691}'), baseline);
    assert!(
        project.join("talaria-attention-probe").is_dir(),
        "the probe folder remains"
    );
    let _ = first.finish(Duration::from_secs(20));
    println!("LIVE_ATTENTION_RESULT: PASSED");
}

#[test]
#[ignore = "needs a running gateway"]
fn live_unwatched_done_counts_without_alert() {
    let Some((_runtime, _client)) = preconditions() else {
        return;
    };
    let home = tempfile::tempdir().expect("temporary home");
    let project = git_project(home.path());
    let mut first = launch(home.path(), &project);
    assert!(ready(&mut first), "the first composer draws");
    std::thread::sleep(Duration::from_secs(16));
    let baseline = status_count(&first.screen_text(), '\u{2713}');
    let before = first.raw_output().len();

    let mut second = launch(home.path(), &project);
    assert!(ready(&mut second), "the second composer draws");
    type_line(&mut second, DONE_ASK);
    assert!(
        second.wait_for_text("working", Duration::from_secs(60)),
        "the turn starts"
    );
    type_line(&mut second, "/quit");
    let _ = second.finish(Duration::from_secs(20));

    let deadline = Instant::now() + Duration::from_secs(90);
    while Instant::now() < deadline && status_count(&first.screen_text(), '\u{2713}') < baseline + 1
    {
        std::thread::sleep(Duration::from_millis(200));
    }
    assert_eq!(status_count(&first.screen_text(), '\u{2713}'), baseline + 1);
    let raw = first.raw_output();
    let added = String::from_utf8_lossy(&raw[before.min(raw.len())..]);
    let notified_finish =
        added.contains("Talaria: ") && (added.contains(": done") || added.contains(": finished"));
    assert!(!notified_finish, "an unwatched finish does not notify");
    let _ = first.finish(Duration::from_secs(20));
    println!("LIVE_ATTENTION_RESULT: PASSED");
}

fn resume_id(text: &str) -> String {
    let marker = "talaria --resume ";
    let start = text
        .find(marker)
        .unwrap_or_else(|| panic!("no resume hint in the output"))
        + marker.len();
    text[start..]
        .split_whitespace()
        .next()
        .unwrap_or_else(|| panic!("the resume hint has no id"))
        .to_owned()
}
