//! Through the binary: the `/effort` scale opened while an approval card is up stays off the
//! screen, so the card keeps its rows, and the next input closes it as Esc would.

use crate::support;

use crate::mock_gateway;

use std::time::{Duration, Instant};

use mock_gateway::StandaloneGateway;
use serde_json::json;
use support::Launch;

const COMMAND: &str = "rm -rf .build-cache";

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

fn scale_drawn(screen: &str) -> bool {
    screen.contains("Enter apply") || screen.contains("Esc cancel")
}

#[test]
fn the_effort_scale_opened_under_an_approval_card_is_not_drawn_and_closes_on_the_next_key() {
    let home = temp_home();
    let gateway = StandaloneGateway::start(home.path());
    let id = format!("local-5ee5a141{}", "0".repeat(56));
    let mut snapshot = mock_gateway::recorded_resume_snapshot();
    snapshot["session_id"] = json!(id);
    snapshot["stored_session_id"] = json!(id);
    snapshot["prompts"] = json!([]);
    snapshot["running"] = json!(false);
    let generation = snapshot["execution_generation"].as_i64().unwrap_or(1);
    gateway.set_resume_snapshot(snapshot);

    let mut terminal = Launch::new(home.path())
        .args(&["--resume", &id, "--profile", "fixture"])
        .answer_queries()
        .spawn();
    assert!(
        terminal.wait_for_text("── Resumed: ", Duration::from_secs(10)),
        "the session never opened:\n{}",
        terminal.screen_text()
    );

    // The composer holds `/effort` when the card arrives, so the composer keeps the keys and
    // Enter submits the command with the card still up.
    terminal.type_keys("/effort");
    assert!(terminal.wait_for_text("/effort", Duration::from_secs(10)));
    gateway.block_on(gateway.approval_request("prompt-9", generation, COMMAND, &["once", "deny"]));
    assert!(
        wait_for(&terminal, Duration::from_secs(10), |screen| screen
            .contains(COMMAND)),
        "the approval card never showed:\n{}",
        terminal.screen_text()
    );
    std::thread::sleep(Duration::from_millis(250));
    let reads = gateway.requests("config.get").len();
    terminal.type_keys("\r");
    assert!(
        gateway.wait_until(Duration::from_secs(10), |mock| mock
            .requests("config.get")
            .len()
            > reads),
        "the effort scale never opened:\n{}",
        terminal.screen_text()
    );
    terminal.settle();
    let screen = terminal.screen_text();
    assert!(
        screen.contains(COMMAND) && screen.contains("Enter confirm · Esc deny"),
        "the card lost its rows to the scale:\n{screen}"
    );
    assert!(
        !scale_drawn(&screen),
        "the scale was drawn under the approval card:\n{screen}"
    );

    // One more input: the scale closes first, so once the card settles nothing comes back.
    terminal.type_keys("\x1b[B");
    std::thread::sleep(Duration::from_millis(250));
    gateway.block_on(gateway.approval_settled("prompt-9", generation));
    assert!(
        wait_for(&terminal, Duration::from_secs(10), |screen| screen
            .contains("answered elsewhere")),
        "the approval card stayed after it settled:\n{}",
        terminal.screen_text()
    );
    terminal.settle();
    let screen = terminal.screen_text();
    assert!(
        !scale_drawn(&screen),
        "the scale stayed open behind the card:\n{screen}"
    );
    assert!(
        gateway.requests("config.set").is_empty(),
        "closing the scale wrote a setting"
    );
    assert!(
        gateway.requests("approval.respond").is_empty(),
        "the key answered the approval card"
    );

    terminal.type_keys("\x03");
    let _ = terminal.finish(Duration::from_secs(10));
}
