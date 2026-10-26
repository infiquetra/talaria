//! Replay of the recorded fleet frames. Absent or empty recordings panic.

#[path = "support/mock_gateway.rs"]
mod mock_gateway;

use std::path::PathBuf;

use mock_gateway::StandaloneGateway;
use serde_json::Value;
use talaria_app::fleet::{FleetConfig, list_once};
use talaria_core::attention::NeedSummary;
use talaria_core::fleet::{NeedDetail, RowState};

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
    for line in text.lines() {
        let entry: Value = serde_json::from_str(line).expect("a recorded frame");
        let frame = entry.get("frame").unwrap_or(&entry);
        if entry.get("dir").and_then(Value::as_str) == Some("in")
            && let Some(result) = frame.get("result")
        {
            gateway.push_result(result.clone());
        }
    }
    let mut cfg = FleetConfig::default();
    cfg.only_profile = Some("fixture".into());
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
