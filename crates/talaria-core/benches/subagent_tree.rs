//! Budgets for the subagent tree. Issue #242, test 47.
//!
//! Each case runs, prints one row per budget (`<name> median <value> limit
//! <limit> pass` or `fail`) and exits non-zero when any row fails. Red: the
//! stubbed tree panics before the first sample.

use std::hint::black_box;
use std::time::Instant;

use hermes_contract::events::EventFrame;
use serde_json::{Value, json};
use talaria_core::subagents::{Clock, SubagentEvent, SubagentTree};

const ROWS: usize = 200;
const RUNS: usize = 200;
const APPLY_LIMIT_US: u128 = 20;
const RECONCILE_LIMIT_US: u128 = 1_000;

fn clock_ms(ms: u64) -> Clock {
    Clock {
        mono_ms: ms,
        wall_s: ms as f64 / 1000.0,
    }
}

fn event(kind: &str, payload: Value) -> SubagentEvent {
    let params = json!({
        "type": kind,
        "session_id": "local-bench",
        "seq": 1,
        "replay_epoch": "epoch",
        "payload": payload,
    });
    let frame = EventFrame::from_params(&params).expect("contract-shaped params");
    SubagentEvent::from_notification(&frame.notification).expect("subagent event")
}

fn payload(id: &str) -> Value {
    json!({
        "subagent_id": id,
        "goal": format!("goal {id}"),
        "task_index": 0,
        "task_count": 1,
    })
}

fn with(payload: Value, key: &str, value: Value) -> Value {
    let mut object = payload.as_object().cloned().expect("object");
    object.insert(key.to_owned(), value);
    Value::Object(object)
}

/// 200 rows across 4 levels: 50 roots, each with a child, a grandchild and a
/// great-grandchild.
fn big_tree() -> SubagentTree {
    let mut tree = SubagentTree::new();
    for root in 0..50 {
        let ids = [
            format!("bench-{root}-0"),
            format!("bench-{root}-1"),
            format!("bench-{root}-2"),
            format!("bench-{root}-3"),
        ];
        for (level, id) in ids.iter().enumerate() {
            let mut body = payload(id);
            if level > 0 {
                body = with(body, "parent_id", json!(ids[level - 1].clone()));
            }
            tree.apply(
                &event("subagent.spawn_requested", body.clone()),
                clock_ms(1_000),
            );
            tree.apply(&event("subagent.start", body), clock_ms(1_001));
        }
    }
    tree
}

fn median_us(samples: &mut [u128]) -> u128 {
    samples.sort_unstable();
    samples[samples.len() / 2]
}

fn main() {
    let mut failed = false;

    let mut tree = big_tree();
    let tool = with(
        with(payload("bench-0-3"), "tool_name", json!("Terminal")),
        "tool_preview",
        json!("a preview of the tool output"),
    );
    let tool = event("subagent.tool", tool);
    let mut samples = Vec::with_capacity(RUNS);
    for _ in 0..RUNS {
        let started = Instant::now();
        tree.apply(&tool, clock_ms(2_000));
        samples.push(started.elapsed().as_micros());
        black_box(tree.version());
    }
    let median = median_us(&mut samples);
    println!(
        "apply_tool_200_rows median {median}us limit {APPLY_LIMIT_US}us {}",
        if median <= APPLY_LIMIT_US {
            "pass"
        } else {
            failed = true;
            "fail"
        }
    );

    let mut tree = big_tree();
    let snapshots: Vec<Value> = (0..50)
        .flat_map(|root| {
            (0..4).map(move |level| {
                json!({
                    "subagent_id": format!("bench-{root}-{level}"),
                    "goal": format!("goal bench-{root}-{level}"),
                    "status": "running",
                    "task_index": 0,
                    "task_count": 1,
                })
            })
        })
        .collect();
    assert_eq!(snapshots.len(), ROWS);
    let list: hermes_contract::schemas::SubagentListResult =
        serde_json::from_value(json!({"subagents": snapshots, "delegations": []}))
            .expect("contract-shaped list answer");
    let mut samples = Vec::with_capacity(RUNS);
    for _ in 0..RUNS {
        let started = Instant::now();
        tree.reconcile(&list, clock_ms(3_000));
        samples.push(started.elapsed().as_micros());
        black_box(tree.version());
    }
    let median = median_us(&mut samples);
    println!(
        "reconcile_200_rows median {median}us limit {RECONCILE_LIMIT_US}us {}",
        if median <= RECONCILE_LIMIT_US {
            "pass"
        } else {
            failed = true;
            "fail"
        }
    );

    let tree = big_tree();
    let mut samples = Vec::with_capacity(RUNS);
    for _ in 0..RUNS {
        let started = Instant::now();
        let order = tree.rows_in_order();
        samples.push(started.elapsed().as_micros());
        black_box(order);
    }
    let median = median_us(&mut samples);
    println!("rows_in_order_200_rows median {median}us informational");

    if failed {
        std::process::exit(1);
    }
}
