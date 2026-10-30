//! Configuration for live terminal agents.

#[test]
fn discovery_is_opt_in_and_keeps_state_reporting() {
    let loaded = talaria_app::config::parse("");
    assert!(!loaded.config.herdr.enabled);
    assert!(loaded.config.herdr.report_state);
    assert_eq!(loaded.config.herdr.poll_interval_ms, 2_000);
}

#[test]
fn configured_discovery_and_invalid_values_are_handled() {
    let loaded = talaria_app::config::parse(
        "[herdr]\nenabled = true\nbinary = 'terminal-manager'\npoll_interval_ms = 750\nworkspace_filter = 'fixture'\n",
    );
    let config = loaded.config.herdr.fleet_config();
    assert!(config.enabled);
    assert_eq!(config.binary.to_string_lossy(), "terminal-manager");
    assert_eq!(config.poll_interval_ms, 750);
    assert_eq!(config.workspace_filter.as_deref(), Some("fixture"));
    assert!(loaded.notices.is_empty());
    let invalid = talaria_app::config::parse("[herdr]\nenabled = 'yes'\npoll_interval_ms = 0\n");
    assert!(!invalid.config.herdr.enabled);
    assert_eq!(invalid.config.herdr.poll_interval_ms, 2_000);
    assert_eq!(invalid.notices.len(), 2);
}
