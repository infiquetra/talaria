//! The working row through the binary: it sits directly above the composer while a turn runs
//! and leaves once the turn completes.

use crate::support;

use crate::mock_gateway;

use std::time::{Duration, Instant};

use mock_gateway::{Fixture, StandaloneGateway, fixture_event};
use serde_json::json;
use support::Launch;

fn temp_home() -> tempfile::TempDir {
    let home = tempfile::tempdir().expect("home");
    std::fs::create_dir(home.path().join(".talaria")).expect("config folder");
    std::fs::write(
        home.path().join(".talaria/config.toml"),
        "[fleet]\nenabled = false\n",
    )
    .expect("disable unrelated fleet polling");
    home
}

/// The screen's working row and the row under it, if a working row is on screen.
fn working_row_and_next(screen: &str) -> Option<(String, String)> {
    let lines: Vec<&str> = screen.lines().collect();
    let at = lines.iter().position(|line| {
        line.trim_start()
            .strip_prefix("✻ Working · ")
            .is_some_and(|rest| rest.starts_with(|c: char| c.is_ascii_digit()))
    })?;
    let next = lines.get(at + 1).copied().unwrap_or_default();
    Some((lines[at].to_owned(), next.to_owned()))
}

fn wait_for(terminal: &support::Terminal, limit: Duration, check: impl Fn(&str) -> bool) -> bool {
    let deadline = Instant::now() + limit;
    loop {
        if check(&terminal.screen_text()) {
            return true;
        }
        if Instant::now() >= deadline {
            return false;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
}

#[test]
fn working_row_sits_above_the_composer_until_the_turn_completes() {
    let home = temp_home();
    let gateway = StandaloneGateway::start(home.path());
    let id = format!("local-5ee5a11e{}", "0".repeat(56));
    let mut snapshot = mock_gateway::recorded_resume_snapshot();
    snapshot["session_id"] = json!(id);
    snapshot["stored_session_id"] = json!(id);
    snapshot["prompts"] = json!([]);
    snapshot["running"] = json!(true);
    gateway.set_resume_snapshot(snapshot);
    // Interrupting ends the turn the way Hermes does, with a completed message.
    gateway.replay_after(
        "session.interrupt",
        vec![fixture_event(Fixture::Interrupted, "message.complete")],
    );

    let mut terminal = Launch::new(home.path())
        .args(&["attach", &id, "--profile", "fixture"])
        .answer_queries()
        .spawn();
    assert!(
        terminal.wait_for_text("── Resumed: ", Duration::from_secs(10)),
        "the running session never opened"
    );
    assert!(
        wait_for(&terminal, Duration::from_secs(10), |screen| {
            working_row_and_next(screen).is_some()
        }),
        "no working row while the turn runs:\n{}",
        terminal.screen_text()
    );
    let screen = terminal.screen_text();
    let (row, next) = working_row_and_next(&screen).expect("the working row");
    assert!(
        row.contains("to interrupt"),
        "the row names the interrupt key: {row:?}"
    );
    assert!(
        next.trim_start().starts_with('╭') || next.trim_start().starts_with('┌'),
        "the composer's top border sits directly under the working row, not {next:?}:\n{screen}"
    );

    terminal.type_keys("\x1b");
    assert!(
        gateway
            .wait_request("session.interrupt", 0, Duration::from_secs(10))
            .is_some(),
        "the interrupt key sent no interrupt"
    );
    assert!(
        wait_for(&terminal, Duration::from_secs(10), |screen| {
            !screen.contains("✻ Working")
        }),
        "the working row stayed after the turn completed:\n{}",
        terminal.screen_text()
    );
    // With the turn over the screen is idle again: nothing is drawn while nothing changes.
    std::thread::sleep(Duration::from_millis(500));
    let settled = terminal.raw_output().len();
    std::thread::sleep(Duration::from_millis(1_000));
    let idle = terminal.raw_output().len() - settled;
    assert_eq!(idle, 0, "the idle screen wrote {idle} bytes in a second");

    terminal.type_keys("/quit");
    assert!(terminal.wait_for_text("/quit", Duration::from_secs(10)));
    std::thread::sleep(Duration::from_millis(250));
    terminal.type_keys("\r");
    let screen = terminal.finish(Duration::from_secs(10));
    assert_eq!(screen.exit_code, Some(0), "{}", screen.text);
}
