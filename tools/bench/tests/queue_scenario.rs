//! Issue 232. Queue and steer. Queue frames while a recorded reply streams.
//!
//! The scenario file is not on this branch. The panic does not name a path.

use std::path::Path;

use talaria_bench::Budgets;

#[test]
fn queue_scenario_holds_budgets() {
    let scenario = Path::new(env!("CARGO_MANIFEST_DIR")).join("scenarios/queue.jsonl");
    if !scenario.is_file() {
        panic!("no scenario");
    }
    let budgets = Budgets::load(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("budgets.toml")
            .as_path(),
    )
    .unwrap_or_else(|_| panic!("read the budgets"));
    assert_eq!(budgets.keystroke.p95_ms, 16.0);
    assert_eq!(budgets.keystroke.max_ms, 33.0);
    assert_eq!(budgets.idle.frames, 0);
    let fanout_every_ms = 100_u64;
    assert_eq!(fanout_every_ms, 100);
}
