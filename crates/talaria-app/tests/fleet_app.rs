//! Configuration notices and the interactive launch's fleet attach.

#[path = "support/mock_gateway.rs"]
mod mock_gateway;

use std::time::{Duration, Instant};

use mock_gateway::StandaloneGateway;
use talaria_app::config::parse;
use talaria_app::fleet::{FleetConfig, FleetService, attaches_after_first_frame};

#[tokio::test]
async fn app_wiring_and_config() {
    let loaded = parse(
        "[fleet]\nenabled = \"yes\"\nhot_poll_ms = 50\ncold_poll_ms = 999999\nbackstop_s = 1\n",
    );
    let notices = loaded.notices.join("\n");
    assert!(
        notices.contains("[fleet] enabled must be true or false; using true."),
        "{notices}"
    );
    assert!(
        notices
            .contains("[fleet] hot_poll_ms must be a whole number from 1000 to 10000; using 1000."),
        "{notices}"
    );
    assert!(
        notices.contains(
            "[fleet] cold_poll_ms must be a whole number from 2000 to 30000; using 30000."
        ),
        "{notices}"
    );
    assert!(
        notices.contains("[fleet] backstop_s must be a whole number from 10 to 300; using 10."),
        "{notices}"
    );
    assert!(loaded.config.fleet.enabled);
    assert_eq!(loaded.config.fleet.hot_poll, Duration::from_millis(1000));
    assert_eq!(loaded.config.fleet.cold_poll, Duration::from_millis(30_000));
    assert_eq!(loaded.config.fleet.backstop, Duration::from_secs(10));

    let chosen = parse("[fleet]\nenabled = false\nhot_poll_ms = 1500\n");
    assert!(chosen.notices.is_empty(), "{:?}", chosen.notices);
    assert!(!chosen.config.fleet.enabled);
    assert_eq!(chosen.config.fleet.hot_poll, Duration::from_millis(1500));

    let defaults = FleetConfig::default();
    assert!(defaults.enabled);
    assert_eq!(defaults.hot_poll, Duration::from_secs(2));
    assert_eq!(defaults.cold_poll, Duration::from_secs(4));
    assert_eq!(defaults.backstop, Duration::from_secs(30));
    assert_eq!(defaults.roster_gap, Duration::from_secs(3));
    assert_eq!(defaults.liveness_gap, Duration::from_secs(1));
    assert_eq!(defaults.coalesce, Duration::from_millis(250));
    assert_eq!(defaults.call_timeout, Duration::from_secs(10));
    assert_eq!(defaults.max_in_flight, 8);
    assert_eq!(defaults.suspect_in_flight, 2);
    let scaled = FleetConfig::scaled_for_tests();
    assert_eq!(scaled.hot_poll, Duration::from_millis(100));
    assert_eq!(scaled.cold_poll, Duration::from_millis(200));
    assert_eq!(scaled.backstop, Duration::from_millis(1500));
    assert_eq!(scaled.coalesce, Duration::from_micros(12_500));
    assert_eq!(scaled.max_in_flight, 8);

    let home = tempfile::tempdir().expect("a temporary home");
    let gateway = StandaloneGateway::start(home.path());
    assert_eq!(
        gateway.websocket_connections(),
        0,
        "no fleet connection before the first frame"
    );
    assert!(
        !attaches_after_first_frame(true, true),
        "print mode does not attach"
    );
    assert!(
        !attaches_after_first_frame(false, false),
        "a disabled setting does not attach"
    );
    let handle = FleetService::start(
        home.path().to_path_buf(),
        "fixture".into(),
        FleetConfig::default(),
        |_| {},
    );
    let started = Instant::now();
    handle.close().await;
    assert!(
        started.elapsed() < Duration::from_millis(200),
        "close returns within 200 ms when the mock never answers"
    );
    assert_eq!(
        gateway
            .ticket_records()
            .iter()
            .filter(|ticket| ticket.scope == "host")
            .count(),
        0,
        "print mode mints no host ticket"
    );
    assert!(
        attaches_after_first_frame(false, true),
        "an interactive launch with the fleet enabled attaches after the first frame"
    );
}
