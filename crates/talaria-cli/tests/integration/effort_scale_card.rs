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
fn question_pane_accepts_the_clicked_answer_and_restores_the_composer() {
    let home = temp_home();
    std::fs::write(
        home.path().join(".talaria/config.toml"),
        "[fleet]\nenabled = false\n[ui]\nmouse = true\n",
    )
    .unwrap();
    let gateway = StandaloneGateway::start(home.path());
    let id = format!("local-5ee5a149{}", "0".repeat(56));
    let mut snapshot = mock_gateway::recorded_resume_snapshot();
    snapshot["session_id"] = json!(id);
    snapshot["stored_session_id"] = json!(id);
    snapshot["prompts"] = json!([]);
    snapshot["running"] = json!(false);
    let generation = snapshot["execution_generation"].as_i64().unwrap_or(1);
    gateway.set_resume_snapshot(snapshot);

    let mut terminal = Launch::new(home.path())
        .args(&["--resume", &id, "--profile", "fixture"])
        .size(12, 60)
        .answer_queries()
        .spawn();
    let limit = Duration::from_secs(5);
    assert!(
        gateway.wait_request("session.resume", 0, limit).is_some(),
        "{}",
        terminal.screen_text()
    );
    assert!(
        terminal.wait_for_text("Type a message", limit),
        "{}",
        terminal.screen_text()
    );
    gateway.block_on(gateway.clarify_request(
        "choose",
        generation,
        "Which colour?",
        &["Red", "Blue"],
        false,
    ));
    assert!(terminal.wait_for_text("Question · Choose one", limit));
    let screen = terminal.screen_text();
    assert!(
        screen.contains("Which colour?") && !screen.contains("Type a message"),
        "{screen}"
    );
    let blue_row = screen
        .lines()
        .position(|line| line.contains("2. Blue"))
        .expect("Blue is visible")
        + 1;
    // Ignore input for a quarter second after a new card appears, as in a real session.
    std::thread::sleep(Duration::from_millis(260));
    terminal.type_keys(&format!("\x1b[<0;6;{blue_row}M\x1b[<0;6;{blue_row}m"));
    assert!(
        terminal.wait_for_text("› 2. Blue", limit),
        "{}",
        terminal.screen_text()
    );
    terminal.type_keys("\x1bOR"); // F3 opens help for the selected choice.
    assert!(terminal.wait_for_text("Prompt card keys", limit));
    assert!(terminal.screen_text().contains("Enter or 1–9 choose"));
    terminal.type_keys("\x1b");
    assert!(terminal.wait_for_text("› 2. Blue", limit));
    assert!(gateway.requests("clarify.respond").is_empty());
    terminal.type_keys("\r");
    let answer = gateway
        .wait_request("clarify.respond", 0, limit)
        .expect("answer");
    assert_eq!(answer.params["answer"], "Blue");
    gateway.block_on(gateway.clarify_settled("choose", generation));
    assert!(terminal.wait_for_text("Type a message", limit));
    assert!(!terminal.screen_text().contains("Question · Choose one"));
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
