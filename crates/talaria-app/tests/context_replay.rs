//! Red-run test 39 (#234): every recorded reply reads through this unit.

use std::collections::HashMap;
use std::path::PathBuf;

use hermes_contract::schemas::{
    SessionContextBreakdownResult, SessionMutateResult, SessionStats, UsageModel,
};
use serde_json::Value;
use talaria_core::context::breakdown::Breakdown;
use talaria_core::context::compact::{CompactResult, interpret};
use talaria_core::context::usage::UsageReport;
use talaria_core::mutation::MutationOutcome;
use talaria_core::session::Usage;

fn fixture_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/frames/context-compaction.jsonl")
}

fn records() -> Vec<Value> {
    let text = std::fs::read_to_string(fixture_path()).expect("the recording ships with the repo");
    text.lines()
        .map(|line| serde_json::from_str(line).expect("every line parses"))
        .collect()
}

#[test]
fn test_39_recorded_replies_parse_and_read() {
    let records = records();
    assert_eq!(records[0]["kind"], "header");

    let mut methods: HashMap<String, String> = HashMap::new();
    for record in &records[1..] {
        let frame = &record["frame"];
        if record["dir"] == "out" && frame["method"].is_string() {
            methods.insert(
                frame["id"].to_string(),
                frame["method"].as_str().expect("method").to_owned(),
            );
        }
    }

    let mut breakdowns = 0;
    let mut stats = 0;
    let mut plans = 0;
    let mut headlines: Vec<String> = Vec::new();
    for record in &records[1..] {
        let frame = &record["frame"];
        if record["dir"] != "in" || frame["result"].is_null() {
            continue;
        }
        let id = frame["id"].to_string();
        let Some(method) = methods.get(&id) else {
            continue;
        };
        let result = &frame["result"];
        match method.as_str() {
            "session.context_breakdown" => {
                let parsed: SessionContextBreakdownResult =
                    serde_json::from_value(result.clone()).expect("breakdown fits its type");
                let _ = Breakdown::from_result(&parsed);
                breakdowns += 1;
            }
            "session.stats" => {
                let parsed: SessionStats =
                    serde_json::from_value(result.clone()).expect("stats fit their type");
                let _ = UsageReport::build(Ok(&parsed), &Usage::default(), None, false);
                stats += 1;
            }
            "usage.bars" => {
                let parsed: UsageModel =
                    serde_json::from_value(result.clone()).expect("bars fit their type");
                let _ = &parsed;
                plans += 1;
            }
            "session.mutate" if result["operation"] == "compress" => {
                let parsed: SessionMutateResult =
                    serde_json::from_value(result.clone()).expect("mutate fits its type");
                assert_eq!(parsed.operation, "compress");
                let outcome = match result.get("status").and_then(Value::as_str) {
                    Some(status @ ("preview" | "noop")) => MutationOutcome::OtherStatus {
                        status: status.to_owned(),
                        result: result.clone(),
                    },
                    _ => MutationOutcome::Committed(result.clone()),
                };
                match interpret(&outcome, "recorded") {
                    CompactResult::Compacted {
                        summary: Some(summary),
                        ..
                    } => headlines.push(summary.headline),
                    CompactResult::Preview { .. }
                    | CompactResult::NoChange { .. }
                    | CompactResult::Compacted { .. } => {}
                    other => panic!("recorded reply is unreadable: {other:?}"),
                }
            }
            _ => {}
        }
    }

    assert!(breakdowns >= 2, "the recording holds breakdowns");
    assert!(stats >= 2, "the recording holds stats replies");
    let _ = plans;

    let committed: Vec<&Value> = records[1..]
        .iter()
        .map(|record| &record["frame"]["result"])
        .filter(|result| result["operation"] == "compress" && result["revision"].is_i64())
        .collect();
    assert_eq!(committed.len(), 1, "one committed compress reply");
    let recorded = committed[0]["summary"]["headline"]
        .as_str()
        .expect("recorded headline");
    assert_eq!(headlines, [recorded]);
}
