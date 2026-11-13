//! Roster and liveness decoding.

use serde_json::json;
use talaria_core::fleet::{LiveItem, LiveStatus, Outcome, RosterRow};

#[test]
fn roster_row_reads_contract_fields() {
    let row = json!({
        "session_id": "local-3f2a",
        "id": "local-3f2a",
        "title": "",
        "source": "tui",
        "started_at": 1_792_690_000.5,
        "message_count": 4,
        "model": "minimax-m3.1-flash",
        "running": true
    });
    let decoded = RosterRow::from_value(&row).expect("a contract row");
    assert_eq!(decoded.session_id, "local-3f2a");
    assert_eq!(decoded.title, "");
    assert_eq!(decoded.source.as_deref(), Some("tui"));
    assert_eq!(decoded.started_at, Some(1_792_690_000.5));
    assert_eq!(decoded.message_count, 4);
    assert_eq!(decoded.model.as_deref(), Some("minimax-m3.1-flash"));
    assert!(decoded.running);

    let blank_model = json!({
        "session_id": "local-3f2a",
        "title": "kept",
        "message_count": 0,
        "running": false,
        "model": ""
    });
    let decoded = RosterRow::from_value(&blank_model).expect("an empty model is absent");
    assert_eq!(decoded.model, None);
    assert_eq!(decoded.title, "kept");
    assert!(RosterRow::from_value(&json!({"title": "x"})).is_none());
}

#[test]
fn live_items_decode_from_raw_reply() {
    let sessions = json!([
        {"id": "s-idle", "status": "idle", "message_count": 1, "title": "", "model": ""},
        {"id": "s-wait", "status": "waiting", "message_count": 2, "title": "t", "model": null},
        {"id": "s-work", "status": "working", "message_count": 3, "title": "", "model": "minimax-m3.1-flash"},
        {"id": "s-start", "status": "starting", "message_count": 4, "title": ""},
        {"id": "s-stream", "status": "streaming", "message_count": 5, "title": ""},
        {"id": "s-resume", "status": "resuming", "message_count": 6, "title": ""},
        {"id": "s-paused", "status": "paused", "message_count": 7, "title": "kept"},
        {"status": "idle", "message_count": 8, "title": "no id"}
    ]);
    let mut decoded = Vec::new();
    let mut malformed = 0;
    for item in sessions.as_array().expect("sessions") {
        match LiveItem::from_value(item) {
            Some(item) => decoded.push(item),
            None => malformed += 1,
        }
    }
    assert_eq!(malformed, 1);
    assert_eq!(decoded.len(), 7);
    assert_eq!(decoded[0].status, LiveStatus::Idle);
    assert_eq!(decoded[1].status, LiveStatus::Waiting);
    assert_eq!(decoded[2].status, LiveStatus::Working);
    assert_eq!(decoded[3].status, LiveStatus::Starting);
    assert_eq!(decoded[4].status, LiveStatus::Working);
    assert_eq!(decoded[5].status, LiveStatus::Working);
    assert_eq!(decoded[6].status, LiveStatus::Other("paused".into()));
    assert_eq!(decoded[0].model, None);
    assert_eq!(decoded[1].model, None);
    assert_eq!(decoded[2].model.as_deref(), Some("minimax-m3.1-flash"));
    assert_eq!(decoded[6].title, "kept");
}

#[test]
fn outcome_from_turn_status() {
    assert_eq!(
        Outcome::from_turn_status(Some("complete")),
        Outcome::Completed
    );
    assert_eq!(
        Outcome::from_turn_status(Some("interrupted")),
        Outcome::Interrupted
    );
    assert_eq!(Outcome::from_turn_status(Some("error")), Outcome::Failed);
    assert_eq!(Outcome::from_turn_status(Some("paused")), Outcome::Unknown);
    assert_eq!(Outcome::from_turn_status(None), Outcome::Unknown);
}
