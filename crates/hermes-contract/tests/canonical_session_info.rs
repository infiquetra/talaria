//! Issue 232. Queue and steer. A recorded queue fanout and a settings frame.

use std::path::PathBuf;

use hermes_contract::canonical_events::SessionInfoFrame;
use serde_json::Value;

fn recording(name: &str) -> Vec<Value> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/frames")
        .join(name);
    let text = std::fs::read_to_string(&path).unwrap_or_else(|_| panic!("read the recording"));
    text.lines()
        .filter(|line| !line.is_empty())
        .map(|line| serde_json::from_str(line).unwrap_or_else(|_| panic!("recording line")))
        .collect()
}

fn info_payloads(name: &str) -> Vec<Value> {
    recording(name)
        .into_iter()
        .filter_map(|record| {
            if record.get("kind")?.as_str()? != "frame" {
                return None;
            }
            let frame = record.get("frame")?;
            if frame.get("method")?.as_str()? != "event" {
                return None;
            }
            let params = frame.get("params")?;
            if params.get("type")?.as_str()? != "session.info" {
                return None;
            }
            params.get("payload").cloned()
        })
        .collect()
}

/// A recorded fanout carries the queue fields the generated session-info type lacks.
#[test]
fn decodes_recorded_fanout() {
    let payload = info_payloads("queue-edit-remove.jsonl")
        .into_iter()
        .find(|payload| {
            payload
                .get("pending")
                .and_then(Value::as_array)
                .is_some_and(|rows| !rows.is_empty())
                && payload.get("desktop_protocol").and_then(Value::as_str)
                    == Some("hermes-gateway-v1")
        })
        .expect("recorded fanout");
    let frame = SessionInfoFrame::from_payload(&payload).expect("decode the fanout");
    assert_eq!(frame.desktop_protocol.as_deref(), Some("hermes-gateway-v1"));
    assert!(frame.running.is_some());
    assert!(frame.execution_generation.is_some());
    assert!(frame.authority_epoch.is_some());
    let pending = frame.pending.expect("pending rows");
    assert!(!pending.is_empty());
    for row in &pending {
        assert!(!row.admission_id.is_empty());
        assert!(!row.input_id.is_empty());
        assert!(!row.text.is_empty());
        assert!(row.authority_epoch >= 0);
        let _ = (
            row.sequence,
            &row.status,
            &row.outcome,
            row.execution_generation,
            &row.ref_,
            &row.controls,
            row.steering,
        );
    }
}

/// A settings frame has no queue. Absent is not a default of false.
#[test]
fn settings_frame_has_no_pending_or_running() {
    let payload = info_payloads("queue-edit-remove.jsonl")
        .into_iter()
        .find(|payload| payload.get("pending").is_none() && payload.get("running").is_none())
        .expect("settings frame");
    let frame = SessionInfoFrame::from_payload(&payload).expect("decode the settings frame");
    assert!(frame.pending.is_none());
    assert!(frame.running.is_none());
}
