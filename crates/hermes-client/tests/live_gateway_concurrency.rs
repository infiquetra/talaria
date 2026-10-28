//! Ignored measurements against a running gateway.
//!
//! Each test returns before touching the gateway when the live preconditions
//! are not met. With a gateway, the probe helpers are still stubs.

mod support;

#[path = "concurrency_probe/mod.rs"]
mod concurrency_probe;

use concurrency_probe::import_operator_lines;
use serde_json::Value;
use support::{live_preconditions, report_skip};

const RESULT: &str = "GATEWAY_CONCURRENCY_RESULT";

async fn ready() -> bool {
    match live_preconditions().await {
        Ok(_) => true,
        Err(reason) => {
            report_skip(RESULT, &reason);
            false
        }
    }
}

fn print_records(records: &[Value]) {
    for record in records {
        println!("RECORD {record}");
        if record["healthy"] == false {
            println!("UNHEALTHY {}", record["failed_criteria"]);
        }
    }
}

#[tokio::test]
#[ignore]
async fn probe_series_a_stub_provider() {
    if !ready().await {
        return;
    }
    let records = concurrency_probe::run_series_a().await;
    assert_eq!(records.len(), 10, "ten series A records");
    let control = records
        .iter()
        .find(|record| record["id"] == "A-shared-1")
        .expect("A-shared-1");
    assert_eq!(control["evidence"], "live");
    assert_eq!(control["healthy"], true);
    print_records(&records);
    println!("{RESULT}: PASSED");
}

#[tokio::test]
#[ignore]
async fn probe_series_b_free_model() {
    if !ready().await {
        return;
    }
    let records = concurrency_probe::run_series_b().await;
    for id in ["B-shared-1", "B-shared-5", "B-shared-10", "B-shared-16"] {
        assert!(records.iter().any(|record| record["id"] == id), "{id}");
    }
    print_records(&records);
    println!("{RESULT}: PASSED");
}

#[tokio::test]
#[ignore]
async fn probe_series_c_tool_turns() {
    if !ready().await {
        return;
    }
    let records = concurrency_probe::run_series_c().await;
    assert_eq!(records.len(), 1);
    let record = &records[0];
    assert_eq!(record["id"], "C-shared-10");
    assert_eq!(record["evidence"], "live");
    assert!(record["observations"]["tool_executions"].is_number());
    print_records(&records);
    println!("{RESULT}: PASSED");
}

#[tokio::test]
#[ignore]
async fn probe_series_d_reattach_after_drop() {
    if !ready().await {
        return;
    }
    let records = concurrency_probe::run_series_d().await;
    assert_eq!(records.len(), 1);
    let record = &records[0];
    assert_eq!(record["id"], "D-shared-10");
    assert_eq!(record["evidence"], "live");
    assert_eq!(recovery_total(&record["recovery"]), 10);
    print_records(&records);
    println!("{RESULT}: PASSED");
}

#[tokio::test]
#[ignore]
async fn probe_series_e_detached_retention() {
    if !ready().await {
        return;
    }
    let records = concurrency_probe::run_series_e().await;
    assert_eq!(records.len(), 1);
    let record = &records[0];
    assert_eq!(record["id"], "E-shared-20");
    assert_eq!(record["evidence"], "live");
    assert_eq!(recovery_total(&record["recovery"]), 20);
    assert_eq!(record["recovery"]["submit_after_detach_refused"], true);
    print_records(&records);
    println!("{RESULT}: PASSED");
}

#[tokio::test]
#[ignore]
async fn import_operator_records() {
    if !ready().await {
        return;
    }
    let incomplete = import_operator_lines("RECORD {\"id\":\"A-shared-1\"}\n");
    assert!(incomplete.is_err(), "incomplete record is refused");
    let text = operator_import_text();
    let records = import_operator_lines(&text).expect("operator records");
    let lines = text
        .lines()
        .filter(|line| line.starts_with("RECORD "))
        .count();
    assert_eq!(records.len(), lines);
    print_records(&records);
    println!("{RESULT}: PASSED");
}

fn recovery_total(recovery: &Value) -> u64 {
    [
        "terminal_with_result",
        "terminal_without_result",
        "unknown",
        "unresolved",
    ]
    .into_iter()
    .map(|key| recovery[key].as_u64().expect(key))
    .sum()
}

fn operator_import_text() -> String {
    let path = std::env::var("TALARIA_PROBE_IMPORT")
        .unwrap_or_else(|_| panic!("operator import was not provided"));
    std::fs::read_to_string(&path)
        .unwrap_or_else(|err| panic!("operator import is unreadable: {err}"))
}
