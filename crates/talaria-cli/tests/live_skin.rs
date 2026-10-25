//! Live skins: the theme report and a guarded skin switch.
//!
//! Ignored unless a person asks for them. Each prints `LIVE_SKIN_RESULT: PASSED`,
//! `SKIPPED (<reason>)`, or, for the switch test without its permission flag,
//! `NOT RUN (needs TALARIA_LIVE_SKIN_SWITCH=1)`; with `TALARIA_LIVE_REQUIRE=1`
//! a skip fails the test. Behavior contract: issue 227.
//!
//! Run by hand: `TALARIA_LIVE_REQUIRE=1 cargo test -p talaria-cli --test live_skin --
//! --ignored --nocapture --test-threads=1`.

mod support;

use std::path::{Path, PathBuf};
use std::time::Duration;

use hermes_client::{
    DiscoverOptions, EnsurePolicy, ProfileLookupError, default_home, discover_with,
};
use support::{Launch, Terminal};

const ROWS: u16 = 60;
const COLS: u16 = 160;
const TURN_LIMIT: Duration = Duration::from_secs(120);

fn hermes_home() -> PathBuf {
    std::env::var_os("HERMES_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(default_home)
}

fn discover() -> Result<hermes_client::Endpoint, String> {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|error| error.to_string())?;
    runtime.block_on(async {
        let options = DiscoverOptions {
            control_timeout: Duration::from_secs(5),
            ensure: EnsurePolicy::Never,
        };
        discover_with(&hermes_home(), &options)
            .await
            .map_err(|error| error.to_string())
    })
}

fn skip_reason() -> Option<String> {
    let endpoint = match discover() {
        Err(error) => {
            return Some(match error.as_str() {
                _ if error.contains("NoGateway") || error.contains("no gateway") => {
                    "no Hermes gateway is running for this home".to_owned()
                }
                _ => error,
            });
        }
        Ok(endpoint) => endpoint,
    };
    if let Err(ProfileLookupError::NeverDiscovered { name }) = endpoint.resolve_profile("fixture") {
        return Some(format!(
            "the gateway never discovered a profile named {name}"
        ));
    }
    hermes_contract::pinned::live_skip_reason(endpoint.code_sha())
}

/// True when the test may run.
fn begin() -> bool {
    if let Some(reason) = skip_reason() {
        println!("LIVE_SKIN_RESULT: SKIPPED ({reason})");
        if std::env::var("TALARIA_LIVE_REQUIRE").ok().as_deref() == Some("1") {
            panic!("live test skipped: {reason}");
        }
        return false;
    }
    true
}

fn start(user_home: &Path, project: &Path, args: &[&str]) -> Terminal {
    Launch::new(user_home)
        .hermes_home(&hermes_home())
        .cwd(project)
        .size(ROWS, COLS)
        .answer_queries()
        .args(args)
        .spawn()
}

fn type_line(terminal: &mut Terminal, text: &str) {
    terminal.type_keys(text);
    std::thread::sleep(Duration::from_millis(250));
    terminal.type_keys("\r");
}

/// The skin name after `Skin: ` in a `/skin` or `/theme` report: the first word.
fn reported_skin(text: &str) -> String {
    let line = text
        .lines()
        .find_map(|line| {
            line.trim()
                .trim_start_matches('\u{b7}')
                .trim()
                .strip_prefix("Skin: ")
        })
        .unwrap_or_else(|| panic!("no skin line in the output:\n{text}"));
    line.split([' ', '.', '('])
        .next()
        .unwrap_or_default()
        .trim()
        .to_owned()
}

/// Wait until the live region no longer says it is connecting: `/skin` needs the link.
fn wait_connected(terminal: &mut Terminal) {
    let deadline = std::time::Instant::now() + TURN_LIMIT;
    while terminal.screen_text().contains("Connecting to Hermes")
        && std::time::Instant::now() < deadline
    {
        std::thread::sleep(Duration::from_millis(50));
    }
}

fn temp_project(home: &Path) -> PathBuf {
    let project = home.join("proj");
    std::fs::create_dir(&project).unwrap();
    project
}

#[ignore = "needs a running gateway"]
#[test]
fn live_handshake_skin_resolves() {
    if !begin() {
        return;
    }
    let home = tempfile::tempdir().unwrap();
    let project = temp_project(home.path());
    let mut terminal = start(
        home.path(),
        &project,
        &["--no-ensure", "--profile", "fixture"],
    );
    terminal.wait_first_frame(Duration::from_secs(20));

    type_line(&mut terminal, "/theme");
    assert!(
        terminal.wait_for_text("Motion:", TURN_LIMIT),
        "the theme report prints"
    );
    let text = terminal.screen_text();
    for line in ["Skin:", "Background:", "Colours:", "Motion:"] {
        assert!(text.contains(line), "the report holds {line}:\n{text}");
    }
    for line in text.lines().filter(|line| {
        let trimmed = line.trim();
        trimmed.starts_with("Skin:") || trimmed.starts_with("Background:")
    }) {
        println!("live report: {line}");
    }

    type_line(&mut terminal, "/quit");
    let screen = terminal.finish(Duration::from_secs(20));
    assert_eq!(screen.exit_code, Some(0), "{}", screen.text);
    println!("LIVE_SKIN_RESULT: PASSED");
}

#[ignore = "needs a running gateway and the skin-switch permission"]
#[test]
fn live_skin_switch_round_trip() {
    if std::env::var("TALARIA_LIVE_SKIN_SWITCH").ok().as_deref() != Some("1") {
        println!("LIVE_SKIN_RESULT: NOT RUN (needs TALARIA_LIVE_SKIN_SWITCH=1)");
        return;
    }
    if !begin() {
        return;
    }
    let home = tempfile::tempdir().unwrap();
    let project = temp_project(home.path());
    let mut terminal = start(
        home.path(),
        &project,
        &["--no-ensure", "--profile", "fixture"],
    );
    terminal.wait_first_frame(Duration::from_secs(20));

    wait_connected(&mut terminal);
    // Record the current skin, then switch to ares through /skin.
    type_line(&mut terminal, "/skin");
    assert!(terminal.wait_for_text("Skin:", TURN_LIMIT));
    let recorded = reported_skin(&terminal.screen_text());
    type_line(&mut terminal, "/skin ares");
    assert!(
        terminal.wait_for_text("Skin set to \"ares\"", TURN_LIMIT),
        "the set confirms"
    );

    // The pushed skin shows within three seconds.
    let deadline = std::time::Instant::now() + Duration::from_secs(3);
    loop {
        type_line(&mut terminal, "/theme");
        if terminal.screen_text().contains("Skin: ares") || std::time::Instant::now() > deadline {
            break;
        }
    }
    assert!(
        terminal.screen_text().contains("Skin: ares"),
        "the pushed skin applies within three seconds"
    );

    // Restore the recorded skin and check it by name.
    type_line(&mut terminal, &format!("/skin {recorded}"));
    assert!(
        terminal.wait_for_text(&format!("Skin set to \"{recorded}\""), TURN_LIMIT),
        "the restore confirms"
    );
    type_line(&mut terminal, "/theme");
    assert!(
        terminal.wait_for_text("Motion:", TURN_LIMIT),
        "the report prints"
    );
    assert!(
        terminal
            .screen_text()
            .contains(&format!("Skin: {recorded}")),
        "the restored skin shows by name"
    );
    type_line(&mut terminal, "/quit");
    let screen = terminal.finish(Duration::from_secs(20));
    assert_eq!(screen.exit_code, Some(0), "{}", screen.text);
    println!("LIVE_SKIN_RESULT: PASSED");
}
