//! One recorded fleet session raises one needs-you alert and no done alert.

#[path = "support/mock_gateway.rs"]
mod mock_gateway;

use std::collections::HashMap;
use std::path::Path;
use std::time::{Duration, Instant};

use mock_gateway::StandaloneGateway;
use serde_json::Value;
use talaria_app::fleet::{FleetConfig, FleetNotice, FleetService};
use talaria_app::fleet_attention::AttentionHub;
use talaria_core::attention::AlertKind;
use talaria_core::fleet_attention::{FleetAttentionConfig, FleetScope};

/// The reads a one-shot listing makes.
const FLEET_READS: [&str; 4] = [
    "session.list",
    "session.active_list",
    "approval.pending",
    "session.events.since",
];

fn recorded(path: &Path) -> bool {
    std::fs::read(path).is_ok_and(|bytes| !bytes.is_empty())
}

#[tokio::test]
async fn recorded_fleet_fixture_gives_expected_alerts() {
    let path =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/frames/fleet-registry.jsonl");
    if !recorded(&path) {
        panic!("fleet-registry fixture not recorded (BLOCKED)");
    }
    let text = std::fs::read_to_string(&path).expect("the recorded fixture");
    let home = tempfile::tempdir().expect("a temporary home");
    let gateway = StandaloneGateway::start(home.path());
    // The recording names one profile, so the link serves that profile only.
    gateway.set_refuse_host_scope(true);
    // Replies are served by method and params, as the recording paired them with their calls.
    let mut calls: HashMap<i64, (String, Value)> = HashMap::new();
    for line in text.lines() {
        let entry: Value = serde_json::from_str(line).expect("a recorded frame");
        let frame = entry.get("frame").unwrap_or(&entry);
        let Some(id) = frame.get("id").and_then(Value::as_i64) else {
            continue;
        };
        match entry.get("dir").and_then(Value::as_str) {
            Some("out") => {
                if let Some(method) = frame.get("method").and_then(Value::as_str) {
                    calls.insert(id, (method.to_owned(), frame["params"].clone()));
                }
            }
            Some("in") => {
                if let (Some(result), Some((method, params))) =
                    (frame.get("result"), calls.get(&id))
                    && FLEET_READS.contains(&method.as_str())
                {
                    gateway.push_matched_result(method, params.clone(), result.clone());
                }
            }
            _ => {}
        }
    }

    let (sender, receiver) = std::sync::mpsc::channel();
    let handle = FleetService::start(
        home.path().to_path_buf(),
        "fixture".to_owned(),
        FleetConfig {
            hot_poll: Duration::from_secs(60),
            cold_poll: Duration::from_secs(60),
            ..FleetConfig::scaled_for_tests()
        },
        move |notice| {
            let _ = sender.send(notice);
        },
    );
    // The fleet reads the recorded session and its approval.
    let deadline = Instant::now() + Duration::from_secs(10);
    let mut latest = handle.snapshot();
    while Instant::now() < deadline && !(latest.rows.len() == 2 && latest.queue.len() == 1) {
        tokio::time::sleep(Duration::from_millis(50)).await;
        latest = handle.snapshot();
    }
    assert_eq!(latest.rows.len(), 2, "the recording holds two rows");
    assert_eq!(latest.queue.len(), 1, "the recording holds one queue item");
    drop(handle);

    let notices: Vec<FleetNotice> = receiver.try_iter().collect();
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
    // The quiet window covers the fleet's first listing, so the notices are read after it.
    let base = Instant::now();
    let mut needs = 0usize;
    let mut finished = 0usize;
    for notice in &notices {
        let at = match notice {
            FleetNotice::Changed(_) => base,
            FleetNotice::Attention { .. } => base + Duration::from_secs(20),
        };
        let output = hub.on_notice(notice, &latest, at);
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
