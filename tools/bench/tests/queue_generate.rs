//! Issue 232. The queue scenario is generated from the recorded fanout, deterministically,
//! and the checked-in file equals a fresh generation.

mod support;

use std::fs;
use std::path::{Path, PathBuf};

use hermes_client::replay_source::ReplaySource;
use serde_json::Value;
use talaria_bench::{BenchError, Budgets, generate_queue};

const FANOUT_EVERY_MS: u64 = 100;

fn shipped_budgets() -> Budgets {
    Budgets::load(&support::shipped_budgets_path()).expect("shipped budgets load")
}

fn generate_to(dir: &Path, name: &str) -> PathBuf {
    let path = dir.join(name);
    let fixtures = support::workspace_root().join("fixtures/frames");
    generate_queue(&fixtures, &path, &shipped_budgets())
        .expect("generate_queue writes the scenario");
    path
}

fn records(path: &Path) -> Vec<Value> {
    fs::read_to_string(path)
        .expect("read scenario")
        .lines()
        .skip(1)
        .map(|line| serde_json::from_str(line).expect("record parses"))
        .collect()
}

fn event_type(record: &Value) -> &str {
    record["frame"]["params"]["type"].as_str().unwrap_or("")
}

#[test]
fn generation_is_deterministic_and_opens_with_replay_source() {
    let dir = tempfile::tempdir().expect("temp dir");
    let one = generate_to(dir.path(), "one.jsonl");
    let two = generate_to(dir.path(), "two.jsonl");
    assert_eq!(
        fs::read(&one).expect("read"),
        fs::read(&two).expect("read"),
        "two generations differ"
    );
    let source = ReplaySource::open(&one).expect("ReplaySource opens the queue scenario");
    assert_eq!(source.header().version, 3);
    assert!(!source.truncated());
    assert!(source.inbound().count() > 0);
}

#[test]
fn fanouts_repeat_every_100_ms_while_deltas_stream() {
    let dir = tempfile::tempdir().expect("temp dir");
    let all = records(&generate_to(dir.path(), "queue.jsonl"));
    let start = all
        .iter()
        .find(|r| event_type(r) == "message.start")
        .expect("a reply starts");
    let session = start["frame"]["params"]["session_id"].clone();
    let last_t = all.last().expect("records")["tMs"].as_u64().expect("tMs");

    for (index, record) in all.iter().enumerate() {
        assert_eq!(record["seq"].as_u64(), Some(index as u64 + 1), "record seq");
        assert_eq!(
            record["frame"]["params"]["seq"].as_u64(),
            Some(index as u64 + 1),
            "params.seq"
        );
    }
    let fanouts: Vec<&Value> = all
        .iter()
        .filter(|r| event_type(r) == "session.info")
        .collect();
    assert!(!fanouts.is_empty(), "no fanout in the scenario");
    for (index, fanout) in fanouts.iter().enumerate() {
        let expected = FANOUT_EVERY_MS * (index as u64 + 1);
        assert_eq!(
            fanout["tMs"].as_u64(),
            Some(expected),
            "fanout {index} time"
        );
        assert_eq!(
            fanout["frame"]["params"]["session_id"], session,
            "fanout {index} names another session"
        );
        let rows = fanout["frame"]["params"]["payload"]["pending"]
            .as_array()
            .expect("pending rows");
        assert!(
            rows.iter().any(|row| row["status"] == "queued"),
            "fanout {index} shows no queued row"
        );
    }
    assert_eq!(
        fanouts.len() as u64,
        last_t / FANOUT_EVERY_MS,
        "one fanout per 100 ms of the reply"
    );

    // Deltas stream on both sides of the first and the last fanout.
    let position = |t_ms: u64| {
        all.iter()
            .position(|r| r["tMs"].as_u64() == Some(t_ms) && event_type(r) == "session.info")
            .expect("fanout position")
    };
    let deltas_before = |index: usize| {
        all[..index]
            .iter()
            .any(|r| event_type(r) == "message.delta")
    };
    let deltas_after = |index: usize| {
        all[index..]
            .iter()
            .any(|r| event_type(r) == "message.delta")
    };
    let first = position(FANOUT_EVERY_MS);
    let last = position(FANOUT_EVERY_MS * fanouts.len() as u64);
    assert!(deltas_before(first) && deltas_after(first));
    assert!(deltas_before(last) && deltas_after(last));
}

#[test]
fn checked_in_scenario_equals_a_fresh_generation() {
    let dir = tempfile::tempdir().expect("temp dir");
    let fresh = generate_to(dir.path(), "queue.jsonl");
    let checked_in = Path::new(env!("CARGO_MANIFEST_DIR")).join("scenarios/queue.jsonl");
    assert_eq!(
        fs::read(&checked_in).expect("scenarios/queue.jsonl is checked in"),
        fs::read(&fresh).expect("read"),
        "scenarios/queue.jsonl is stale; run `talaria-bench generate-queue`"
    );
}

#[test]
fn missing_fanout_names_the_template() {
    // The trimmed template copy holds the reply templates but no recorded queue fanout.
    let out = tempfile::tempdir()
        .expect("temp dir")
        .path()
        .join("queue.jsonl");
    let error = generate_queue(&support::templates_dir(), &out, &shipped_budgets())
        .expect_err("no recorded fanout");
    assert!(
        matches!(error, BenchError::TemplateMissing { .. }),
        "{error:?}"
    );
}
