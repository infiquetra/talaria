//! Red-run test 40 (#234): the live context recording, ignored by default.

use std::collections::HashMap;
use std::path::PathBuf;

use hermes_contract::schemas::{SessionContextBreakdownResult, SessionMutateResult, SessionStats};
use serde_json::Value;
use talaria_core::context::breakdown::Breakdown;
use talaria_core::context::compact::{CompactResult, interpret};
use talaria_core::context::usage::UsageReport;
use talaria_core::mutation::MutationOutcome;
use talaria_core::session::Usage;

fn live_dir() -> Option<PathBuf> {
    std::env::var_os("TALARIA_LIVE_FRAMES").map(PathBuf::from)
}

fn required() -> bool {
    std::env::var("TALARIA_LIVE_REQUIRE").as_deref() == Ok("1")
}

fn logs(dir: &PathBuf) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut logs: Vec<PathBuf> = entries
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .filter(|path| path.extension().and_then(|ext| ext.to_str()) == Some("jsonl"))
        .collect();
    logs.sort();
    logs
}

#[test]
#[ignore]
fn live_context_usage_and_compaction() {
    let Some(dir) = live_dir() else {
        println!("LIVE_CONTEXT_RESULT: SKIPPED (TALARIA_LIVE_FRAMES is not set)");
        assert!(!required(), "live run required but no frames directory");
        return;
    };
    let found = logs(&dir);
    if found.is_empty() {
        println!(
            "LIVE_CONTEXT_RESULT: SKIPPED (no frame logs in {})",
            dir.display()
        );
        assert!(!required(), "live run required but no frame logs");
        return;
    }

    let mut stats_after = 0_i64;
    for path in &found {
        let text = std::fs::read_to_string(path).expect("live log reads");
        let records: Vec<Value> = text
            .lines()
            .map(|line| serde_json::from_str(line).expect("live log parses"))
            .collect();
        let mut methods: HashMap<String, String> = HashMap::new();
        for record in records.iter().skip(1) {
            let frame = &record["frame"];
            if record["dir"] == "out" && frame["method"].is_string() {
                methods.insert(
                    frame["id"].to_string(),
                    frame["method"].as_str().expect("method").to_owned(),
                );
            }
        }
        let mut saw_stats = false;
        let mut saw_breakdown = false;
        let mut saw_preview = false;
        let mut saw_commit = false;
        let mut saw_event = false;
        let mut saw_usage = false;
        let mut committed_at: Option<usize> = None;
        for (index, record) in records.iter().enumerate().skip(1) {
            let frame = &record["frame"];
            if record["dir"] == "in" {
                if frame["method"] == "event" {
                    let params = &frame["params"];
                    match params["type"].as_str() {
                        Some("session.updated") if params["payload"]["operation"] == "compress" => {
                            saw_event = true;
                        }
                        Some("session.usage")
                            if params["payload"]["usage"]["context_percent"].is_i64() =>
                        {
                            saw_usage = true;
                        }
                        _ => {}
                    }
                    continue;
                }
                if frame["result"].is_null() {
                    continue;
                }
                let id = frame["id"].to_string();
                match methods.get(&id).map(String::as_str) {
                    Some("session.context_breakdown") => {
                        let parsed: SessionContextBreakdownResult =
                            serde_json::from_value(frame["result"].clone())
                                .expect("breakdown fits its type");
                        if committed_at.is_some_and(|at| index > at) {
                            // After a compaction the gateway measures a conversation
                            // that is gone, so it reports no categories.
                            assert!(
                                parsed.categories.is_empty(),
                                "the breakdown after the compaction is empty"
                            );
                        } else {
                            assert!(!parsed.categories.is_empty());
                            assert!(parsed.context_max > 0);
                        }
                        let _ = Breakdown::from_result(&parsed);
                        saw_breakdown = true;
                    }
                    Some("session.stats") => {
                        let parsed: SessionStats = serde_json::from_value(frame["result"].clone())
                            .expect("stats fit their type");
                        let _ = UsageReport::build(Ok(&parsed), &Usage::default(), None, false);
                        if committed_at.is_some_and(|at| index > at) {
                            // The gateway's counters start again after a compaction.
                            assert_eq!(
                                parsed.api_calls, 0,
                                "the session's calls restart after the compaction"
                            );
                            stats_after = parsed.api_calls;
                        }
                        if parsed.api_calls >= 16 {
                            saw_stats = true;
                        }
                    }
                    Some("session.mutate") if frame["result"]["operation"] == "compress" => {
                        let result = &frame["result"];
                        let parsed: SessionMutateResult =
                            serde_json::from_value(result.clone()).expect("mutate fits its type");
                        assert_eq!(parsed.operation, "compress");
                        match result.get("status").and_then(Value::as_str) {
                            Some("preview") => {
                                let outcome = MutationOutcome::OtherStatus {
                                    status: "preview".to_owned(),
                                    result: result.clone(),
                                };
                                assert!(matches!(
                                    interpret(&outcome, "live"),
                                    CompactResult::Preview { .. }
                                ));
                                saw_preview = true;
                            }
                            _ if result["revision"].is_i64() => {
                                let outcome = MutationOutcome::Committed(result.clone());
                                assert!(matches!(
                                    interpret(&outcome, "live"),
                                    CompactResult::Compacted { .. }
                                ));
                                committed_at = Some(index);
                                saw_commit = true;
                            }
                            _ => {}
                        }
                    }
                    _ => {}
                }
            }
        }
        assert!(
            saw_stats,
            "{}: no stats reply with sixteen calls",
            path.display()
        );
        assert!(saw_breakdown, "{}: no breakdown", path.display());
        assert!(saw_preview, "{}: no preview", path.display());
        assert!(saw_commit, "{}: no committed compress", path.display());
        assert!(saw_event, "{}: no compress event", path.display());
        assert!(saw_usage, "{}: no usage tick", path.display());
    }

    println!("LIVE_CONTEXT_RESULT: PASSED");
    println!("LIVE_CONTEXT_STATS_AFTER_COMPACTION: api_calls={stats_after}");
}
