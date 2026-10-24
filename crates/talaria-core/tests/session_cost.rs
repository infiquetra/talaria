//! Growth check. Ignored on the default test run. Release mode, fresh states, median of five.
//! Only the delta loop is timed. A failure is run once more; the ratios are not raised.

#[path = "support/events.rs"]
mod events;
#[path = "support/snapshot_json.rs"]
mod snapshot_json;

use std::time::{Duration, Instant};

use serde_json::json;
use talaria_core::session::{SessionState, update};

use events::{SESSION, ev};

fn rows(count: usize) -> hermes_contract::schemas::CanonicalSnapshot {
    let mut value = snapshot_json::base(SESSION);
    let messages: Vec<_> = (0..count)
        .map(|index| {
            json!({
                "role": if index % 2 == 0 { "user" } else { "assistant" },
                "text": format!("row {index}"),
            })
        })
        .collect();
    value["messages"] = json!(messages);
    value["message_count"] = json!(count);
    snapshot_json::snapshot(value)
}

fn delta_events(count: usize) -> Vec<talaria_core::session::Event> {
    let chunk = "abcdefghijklmnopqrstuvwxyzabcdefghijklmnopqrstuvwx";
    assert_eq!(chunk.len(), 50);
    (1..=count)
        .map(|seq| ev(seq as u64, "message.delta", json!({"text": chunk})))
        .collect()
}

fn median_delta_loop(row_count: usize, deltas: usize) -> Duration {
    let snapshot = rows(row_count);
    let events = delta_events(deltas);
    let mut state = SessionState::from_snapshot(snapshot.clone());
    for event in events.clone() {
        let _ = update(&mut state, event);
    }
    let mut samples = Vec::with_capacity(5);
    for _ in 0..5 {
        let mut state = SessionState::from_snapshot(snapshot.clone());
        let started = Instant::now();
        for event in events.clone() {
            let _ = update(&mut state, event);
        }
        samples.push(started.elapsed());
    }
    samples.sort();
    samples[2]
}

fn ratios_hold() -> bool {
    let two_thousand = median_delta_loop(2_000, 10_000);
    let eight_thousand = median_delta_loop(8_000, 10_000);
    if eight_thousand.as_nanos() >= two_thousand.as_nanos().saturating_mul(2) {
        return false;
    }
    let ten = median_delta_loop(2_000, 10_000);
    let twenty = median_delta_loop(2_000, 20_000);
    twenty.as_nanos() < ten.as_nanos().saturating_mul(3)
}

#[test]
#[ignore = "release only; times the delta loop on fresh states"]
fn two_thousand_message_session_streams_without_quadratic_cost() {
    if ratios_hold() || ratios_hold() {
        return;
    }
    panic!(
        "the 8000-row delta loop was at least twice the 2000-row loop, or 20000 deltas were at least three times 10000"
    );
}
