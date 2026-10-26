//! Budget for the subagent panel. Issue #242, test 48.
//!
//! Renders a 200-row tree at 160x48, prints one row (`<name> median <value>
//! limit <limit> pass` or `fail`) and exits non-zero when it fails. Red: the
//! stubbed tree or renderer panics before the first sample.

use std::hint::black_box;
use std::time::Instant;

use hermes_contract::events::EventFrame;
use serde_json::{Value, json};
use talaria_core::directory::rows::RowKey;
use talaria_core::subagents::{Caps, Clock, Spawning, SubagentEvent, SubagentTree};
use talaria_tui::subagents::{AgentsPanel, PanelHeader};

const RUNS: usize = 200;
const RENDER_LIMIT_US: u128 = 4_000;

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

fn big_tree() -> SubagentTree {
    let mut tree = SubagentTree::new();
    for n in 0..200 {
        let id = format!("bench-{n:03}");
        let body = json!({
            "subagent_id": id,
            "goal": format!("goal {n:03} with a reasonably long description"),
            "task_index": 0,
            "task_count": 1,
            "model": "minimax-m3.1-flash",
        });
        tree.apply(
            &event("subagent.spawn_requested", body.clone()),
            clock_ms(1_000),
        );
        tree.apply(&event("subagent.start", body), clock_ms(1_001));
    }
    tree
}

fn main() {
    let tree = big_tree();
    let panel = AgentsPanel::new(RowKey {
        profile: "fixture".to_owned(),
        session_id: "local-bench".to_owned(),
    });
    let header = PanelHeader {
        title: "bench".to_owned(),
        spawning: Spawning::On(Some(Caps {
            max_spawn_depth: 1,
            max_concurrent_children: 10,
        })),
        key_label: None,
    };
    let mut samples = Vec::with_capacity(RUNS);
    for _ in 0..RUNS {
        let started = Instant::now();
        let lines = panel.render(&tree, &header, None, 160, 48, clock_ms(13_000));
        samples.push(started.elapsed().as_micros());
        black_box(lines);
    }
    samples.sort_unstable();
    let median = samples[samples.len() / 2];
    println!(
        "panel_render_200_rows median {median}us limit {RENDER_LIMIT_US}us {}",
        if median <= RENDER_LIMIT_US {
            "pass"
        } else {
            "fail"
        }
    );
    if median > RENDER_LIMIT_US {
        std::process::exit(1);
    }
}
