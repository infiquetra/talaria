//! Through the binary: while Hivecode builds a workflow, a block above the composer says which
//! step the agent is on. An interrupted build leaves one line that says it stopped, and the idle
//! screen afterwards draws nothing.

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

#[test]
fn building_block_shows_each_step_and_leaves_one_line_when_stopped() {
    let home = temp_home();
    let gateway = StandaloneGateway::start(home.path());
    let id = format!("local-5ee5a130{}", "0".repeat(56));
    let mut snapshot = mock_gateway::recorded_resume_snapshot();
    snapshot["session_id"] = json!(id);
    snapshot["stored_session_id"] = json!(id);
    snapshot["prompts"] = json!([]);
    snapshot["running"] = json!(false);
    gateway.set_resume_snapshot(snapshot);
    // The routed prompt starts a turn that writes a workflow script and keeps running.
    let mut write = fixture_event(Fixture::Approval, "tool.start");
    write["params"]["payload"]["name"] = json!("write_file");
    write["params"]["payload"]["tool_name"] = json!("write_file");
    write["params"]["payload"]["context"] = json!("fixture.workflow");
    write["params"]["payload"]["args"] = json!({
        "path": "/home/fixture/.talaria/drafts/fixture.workflow",
        "content": "export const meta = {name:'fixture', description:'fixture'};"
    });
    gateway.replay_after(
        "prompt.submit",
        vec![fixture_event(Fixture::Approval, "message.start"), write],
    );
    gateway.replay_after(
        "session.interrupt",
        vec![fixture_event(Fixture::Interrupted, "message.complete")],
    );

    let mut terminal = Launch::new(home.path())
        .args(&["--hivecode", "--resume", &id, "--profile", "fixture"])
        .answer_queries()
        .spawn();
    assert!(
        terminal.wait_for_text("── Resumed: ", Duration::from_secs(10)),
        "the session never opened:\n{}",
        terminal.screen_text()
    );
    terminal.type_keys("fix the parser");
    assert!(terminal.wait_for_text("fix the parser", Duration::from_secs(10)));
    std::thread::sleep(Duration::from_millis(250));
    terminal.type_keys("\r");
    assert!(
        wait_for(&terminal, Duration::from_secs(10), |screen| {
            screen.contains("⚙ Hivecode · building a workflow · ")
                && screen.contains("Step 2 of 5: writing the workflow script · fixture.workflow")
        }),
        "no building block while the workflow script is written:\n{}",
        terminal.screen_text()
    );
    let screen = terminal.screen_text();
    assert!(
        !screen.contains("handling this prompt as a workflow"),
        "the block replaces the interim line:\n{screen}"
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
            screen.contains(
                "Hivecode stopped: interrupted while writing the workflow script (fixture.workflow) after",
            ) && !screen.contains("building a workflow")
        }),
        "the stopped build left no line, or the block stayed:\n{}",
        terminal.screen_text()
    );
    // The block is gone and the screen is idle: nothing is drawn while nothing changes.
    std::thread::sleep(Duration::from_millis(500));
    let settled = terminal.raw_output().len();
    std::thread::sleep(Duration::from_millis(1_500));
    let idle = terminal.raw_output().len() - settled;
    assert_eq!(idle, 0, "the idle screen wrote {idle} bytes");

    terminal.type_keys("\x03");
    let _ = terminal.finish(Duration::from_secs(10));
}
