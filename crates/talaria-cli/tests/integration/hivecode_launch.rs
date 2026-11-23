//! Through the binary: the launch flag's earlier name still starts Hivecode, and help lists
//! only the current name.

use crate::support;

use crate::mock_gateway;

use std::time::Duration;

use mock_gateway::StandaloneGateway;
use serde_json::json;
use support::Launch;

#[test]
fn the_earlier_flag_name_still_starts_hivecode_and_help_does_not_list_it() {
    let home = tempfile::tempdir().expect("home");
    let output = support::captured(
        support::talaria(&["--help"], home.path()),
        Duration::from_secs(10),
    );
    assert_eq!(output.status.code(), Some(0));
    let help = String::from_utf8_lossy(&output.stdout);
    assert!(help.contains("--hivecode"), "{help}");
    assert!(
        !help.to_lowercase().contains("ultracode"),
        "help lists the earlier name:\n{help}"
    );

    std::fs::create_dir(home.path().join(".talaria")).expect("config folder");
    std::fs::write(
        home.path().join(".talaria/config.toml"),
        "[fleet]\nenabled = false\n",
    )
    .expect("disable unrelated fleet polling");
    let gateway = StandaloneGateway::start(home.path());
    let id = format!("local-5ee5a141{}", "0".repeat(56));
    let mut snapshot = mock_gateway::recorded_resume_snapshot();
    snapshot["session_id"] = json!(id);
    snapshot["stored_session_id"] = json!(id);
    snapshot["prompts"] = json!([]);
    snapshot["running"] = json!(false);
    gateway.set_resume_snapshot(snapshot);
    let mut terminal = Launch::new(home.path())
        .args(&["--ultracode", "--resume", &id, "--profile", "fixture"])
        .answer_queries()
        .spawn();
    assert!(
        terminal.wait_for_text("── Resumed: ", Duration::from_secs(10)),
        "the session never opened:\n{}",
        terminal.screen_text()
    );
    assert!(
        terminal.wait_for_text("hivecode", Duration::from_secs(10)),
        "the status bar does not show Hivecode on:\n{}",
        terminal.screen_text()
    );
    terminal.type_keys("\x03");
    let _ = terminal.finish(Duration::from_secs(10));
}
