//! Through the binary: once the `/effort` scale or a streaming thinking row has closed, the idle
//! screen writes nothing. The building block's case is in `workflow_authoring.rs`.

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

/// Bytes the screen writes in a second and a half once it has settled.
fn idle_bytes(terminal: &support::Terminal) -> usize {
    std::thread::sleep(Duration::from_millis(500));
    let settled = terminal.raw_output().len();
    std::thread::sleep(Duration::from_millis(1_500));
    terminal.raw_output().len() - settled
}

fn type_line(terminal: &mut support::Terminal, text: &str) {
    terminal.type_keys(text);
    assert!(terminal.wait_for_text(text, Duration::from_secs(10)));
    std::thread::sleep(Duration::from_millis(250));
    terminal.type_keys("\r");
}

#[test]
fn the_idle_screen_writes_nothing_after_the_effort_scale_and_a_thinking_row_close() {
    let home = temp_home();
    let gateway = StandaloneGateway::start(home.path());
    let id = format!("local-5ee5a140{}", "0".repeat(56));
    let mut snapshot = mock_gateway::recorded_resume_snapshot();
    snapshot["session_id"] = json!(id);
    snapshot["stored_session_id"] = json!(id);
    snapshot["prompts"] = json!([]);
    snapshot["running"] = json!(false);
    gateway.set_resume_snapshot(snapshot);
    // The prompt starts a turn that streams its thinking and keeps running.
    let mut thinking = fixture_event(Fixture::PlainAnswer, "message.delta");
    thinking["params"]["type"] = json!("reasoning.delta");
    thinking["params"]["payload"] = json!({"text": "Reading the parser first.\nThen the tests.\n"});
    gateway.replay_after(
        "prompt.submit",
        vec![
            fixture_event(Fixture::PlainAnswer, "message.start"),
            thinking,
        ],
    );
    gateway.replay_after(
        "session.interrupt",
        vec![fixture_event(Fixture::Interrupted, "message.complete")],
    );

    let mut terminal = Launch::new(home.path())
        .args(&["--resume", &id, "--profile", "fixture"])
        .answer_queries()
        .spawn();
    assert!(
        terminal.wait_for_text("── Resumed: ", Duration::from_secs(10)),
        "the session never opened:\n{}",
        terminal.screen_text()
    );

    type_line(&mut terminal, "/effort");
    assert!(
        wait_for(&terminal, Duration::from_secs(10), |screen| {
            screen.contains("Enter apply") && screen.contains("Esc cancel")
        }),
        "the effort scale never opened:\n{}",
        terminal.screen_text()
    );
    terminal.type_keys("\x1b");
    assert!(
        wait_for(&terminal, Duration::from_secs(10), |screen| {
            !screen.contains("Esc cancel")
        }),
        "the effort scale stayed open:\n{}",
        terminal.screen_text()
    );
    let idle = idle_bytes(&terminal);
    assert_eq!(
        idle, 0,
        "the idle screen wrote {idle} bytes after /effort closed"
    );

    type_line(&mut terminal, "look at the parser");
    assert!(
        wait_for(&terminal, Duration::from_secs(10), |screen| {
            screen.contains("\u{2234} thinking\u{2026}")
        }),
        "no thinking row while the thinking streams:\n{}",
        terminal.screen_text()
    );
    // Esc asks for a second press before it interrupts.
    terminal.type_keys("\x1b");
    std::thread::sleep(Duration::from_millis(300));
    terminal.type_keys("\x1b");
    assert!(
        gateway
            .wait_request("session.interrupt", 0, Duration::from_secs(10))
            .is_some(),
        "the interrupt key sent no interrupt"
    );
    assert!(
        wait_for(&terminal, Duration::from_secs(10), |screen| {
            !screen.contains("\u{2234} thinking\u{2026}") && !screen.contains("✻ Working")
        }),
        "the thinking row or the working row stayed after the turn ended:\n{}",
        terminal.screen_text()
    );
    let idle = idle_bytes(&terminal);
    assert_eq!(
        idle, 0,
        "the idle screen wrote {idle} bytes after the thinking row closed"
    );

    terminal.type_keys("\x03");
    let _ = terminal.finish(Duration::from_secs(10));
}
