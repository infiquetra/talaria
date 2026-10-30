//! Push-to-talk reaches the real terminal router in each Vim mode.

mod support;

#[path = "../../talaria-app/tests/support/mock_gateway.rs"]
mod mock_gateway;

use std::time::{Duration, Instant};

use mock_gateway::StandaloneGateway;
use support::Launch;

#[test]
fn voice_shortcut_reaches_the_composer_in_each_vim_mode() {
    for (mode, preparation) in [
        ("-- NORMAL --", ""),
        ("-- INSERT --", "i"),
        ("-- VISUAL --", "visual"),
    ] {
        let home = tempfile::tempdir().expect("home");
        let config_dir = home.path().join(".talaria");
        std::fs::create_dir(&config_dir).expect("config directory");
        std::fs::write(config_dir.join("config.toml"), "[composer]\nvim = true\n").expect("config");
        let gateway = StandaloneGateway::start(home.path());
        let mut terminal = Launch::new(home.path())
            .args(&["--no-ensure"])
            .answer_queries()
            .spawn();
        assert!(
            terminal.wait_for_text("-- NORMAL --", Duration::from_secs(8)),
            "normal mode did not appear: {}",
            terminal.screen_text()
        );
        let deadline = Instant::now() + Duration::from_secs(5);
        while !gateway
            .requests("config.get")
            .iter()
            .any(|request| request.params["key"] == "voice")
        {
            assert!(Instant::now() < deadline, "voice mode was not requested");
            std::thread::sleep(Duration::from_millis(10));
        }
        if preparation == "visual" {
            terminal.type_keys("i");
            assert!(terminal.wait_for_text("-- INSERT --", Duration::from_secs(3)));
            terminal.type_keys("hello");
            std::thread::sleep(Duration::from_millis(150));
            terminal.type_keys("\x1b");
            assert!(terminal.wait_for_text("-- NORMAL --", Duration::from_secs(3)));
            terminal.type_keys("v");
        } else {
            terminal.type_keys(preparation);
        }
        assert!(
            terminal.wait_for_text(mode, Duration::from_secs(3)),
            "{mode} did not appear: {}",
            terminal.screen_text()
        );
        terminal.type_keys("\x1br");
        assert!(
            terminal.wait_for_text("Checking voice mode", Duration::from_secs(5)),
            "voice shortcut did not reach {mode}: {}",
            terminal.screen_text()
        );
        assert!(terminal.screen_text().contains(mode));
        assert!(gateway.requests("voice.record").is_empty());
    }
}
