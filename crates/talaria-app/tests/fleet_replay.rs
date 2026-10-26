//! Replay of the recorded fleet frames. Absent or empty recordings panic.

#[path = "support/mock_gateway.rs"]
mod mock_gateway;

use std::collections::HashMap;
use std::path::PathBuf;

use mock_gateway::StandaloneGateway;
use serde_json::Value;
use talaria_app::fleet::{FleetConfig, list_once};
use talaria_core::attention::NeedSummary;
use talaria_core::fleet::{NeedDetail, RowState};

/// The reads a one-shot listing makes.
const FLEET_READS: [&str; 4] = [
    "session.list",
    "session.active_list",
    "approval.pending",
    "session.events.since",
];

#[tokio::test]
async fn recorded_fleet_fixture_replays() {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/frames/fleet-registry.jsonl");
    let text = std::fs::read_to_string(&path).unwrap_or_default();
    if text.trim().is_empty() {
        panic!("fleet-registry fixture not recorded");
    }
    let home = tempfile::tempdir().expect("a temporary home");
    let gateway = StandaloneGateway::start(home.path());
    // Replies are served by method and params, as the recording paired them with their calls.
    let mut calls: HashMap<i64, (String, Value)> = HashMap::new();
    let mut served = 0;
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
                    served += 1;
                }
            }
            _ => {}
        }
    }
    assert_eq!(served, 3, "the recording holds the three fleet reads");
    let cfg = FleetConfig {
        only_profile: Some("fixture".into()),
        ..FleetConfig::default()
    };
    let snapshot = list_once(home.path(), &cfg).await.expect("a snapshot");
    assert_eq!(snapshot.rows.len(), 2, "two rows");
    assert_eq!(
        snapshot
            .rows
            .iter()
            .filter(|row| row.state == RowState::NeedsYou)
            .count(),
        1
    );
    assert_eq!(
        snapshot
            .rows
            .iter()
            .filter(|row| row.state == RowState::Idle)
            .count(),
        1
    );
    assert!(
        snapshot.queue.iter().any(|item| {
            matches!(
                &item.detail,
                NeedDetail::Known(NeedSummary::Approval { command, .. })
                    if command.contains("talaria-fleet-probe")
            )
        }),
        "the queue holds the probe approval"
    );
}
