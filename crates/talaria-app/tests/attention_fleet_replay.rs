//! One recorded fleet session raises one needs-you alert and no done alert.

use std::path::{Path, PathBuf};
use std::time::Duration;

use talaria_app::fleet::{FleetConfig, FleetService};
use talaria_app::fleet_attention::AttentionHub;
use talaria_core::attention::AlertKind;
use talaria_core::fleet_attention::{FleetAttentionConfig, FleetScope};

fn recorded(path: &Path) -> bool {
    std::fs::read(path).is_ok_and(|bytes| !bytes.is_empty())
}

#[test]
fn recorded_fleet_fixture_gives_expected_alerts() {
    let path =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/frames/fleet-registry.jsonl");
    if !recorded(&path) {
        panic!("fleet-registry fixture not recorded (BLOCKED)");
    }
    expect_probe_alerts();
}

#[allow(unreachable_code)]
fn expect_probe_alerts() {
    let (sender, receiver) = std::sync::mpsc::channel();
    let _handle = FleetService::start(
        PathBuf::from("fixture"),
        "fixture".to_owned(),
        FleetConfig::scaled_for_tests(),
        move |notice| {
            let _ = sender.send(notice);
        },
    );
    let mut hub = AttentionHub::new(
        FleetAttentionConfig {
            bell: vec![AlertKind::NeedsYou, AlertKind::Failed],
            desktop: vec![AlertKind::NeedsYou, AlertKind::Done, AlertKind::Failed],
            done_after: Duration::from_secs(20),
            needs_you: FleetScope::All,
            finished: FleetScope::All,
        },
        "fixture".to_owned(),
        Some("Alt+A".to_owned()),
    );
    let snap = empty_snapshot();
    let notices: Vec<_> = receiver.try_iter().collect();
    let mut needs = 0usize;
    let mut finished = 0usize;
    for notice in &notices {
        let output = hub.on_notice(notice, &snap, std::time::Instant::now());
        for alert in output.alerts {
            if alert.kind == AlertKind::NeedsYou && alert.text.contains("talaria-fleet-probe") {
                needs += 1;
            }
            if alert.kind == AlertKind::Done
                || alert.text.contains(": done")
                || alert.text.contains(": finished")
            {
                finished += 1;
            }
        }
    }
    assert_eq!(needs, 1, "one needs-you alert naming talaria-fleet-probe");
    assert_eq!(finished, 0, "no done or finished alert");
}

fn empty_snapshot() -> talaria_app::fleet::FleetSnapshot {
    talaria_app::fleet::FleetSnapshot {
        version: 0,
        scope: talaria_app::side_connection::SideScope::ProfileOnly {
            profile: "fixture".to_owned(),
            why: String::new(),
        },
        link: talaria_app::fleet::FleetLinkState::Connected,
        profiles: Vec::new(),
        rows: Vec::new(),
        queue: Vec::new(),
        counts: talaria_core::fleet::FleetCounts::default(),
        diagnostics: Vec::new(),
        as_of: 0.0,
    }
}
