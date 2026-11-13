//! Red-run test 38 (#231): the recording reads through the directory.

use std::collections::HashMap;
use std::path::PathBuf;

use serde_json::Value;
use talaria_core::directory::rows::{DirectoryRow, RowOrigin, merge, order};
use talaria_core::mutation::{MutationOutcome, classify_result};

fn fixture_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/frames/session-picker.jsonl")
}

fn records() -> Vec<Value> {
    let text = std::fs::read_to_string(fixture_path()).expect("the recording ships with the repo");
    text.lines()
        .map(|line| serde_json::from_str(line).expect("every line parses"))
        .collect()
}

#[test]
fn every_recorded_row_reads() {
    let records = records();
    assert_eq!(records[0]["kind"], "header");

    // The restricted shape: profile only, fixture; limit 20.
    let mut methods: HashMap<String, String> = HashMap::new();
    let mut lists = 0;
    for record in &records[1..] {
        let frame = &record["frame"];
        if record["dir"] != "out" || !frame["method"].is_string() {
            continue;
        }
        let method = frame["method"].as_str().expect("method");
        methods.insert(frame["id"].to_string(), method.to_owned());
        if method == "session.list" {
            assert_eq!(frame["params"]["profile"], "fixture");
            assert_eq!(frame["params"]["limit"], Value::from(20));
            lists += 1;
        }
    }
    assert_eq!(lists, 3, "live, stored, and the relist");
    let statuses: Vec<&str> = records[1..]
        .iter()
        .filter_map(|record| {
            let params = record["frame"].get("params")?;
            (params["type"] == "message.complete")
                .then(|| params["payload"]["status"].as_str())
                .flatten()
        })
        .collect();
    assert_eq!(statuses, ["complete"]);

    // Every recorded row parses.
    let mut live: Vec<Value> = Vec::new();
    let mut stored: Vec<Value> = Vec::new();
    for record in &records[1..] {
        let frame = &record["frame"];
        if record["dir"] != "in" || frame["result"].is_null() {
            continue;
        }
        let id = frame["id"].to_string();
        if methods.get(&id).map(String::as_str) != Some("session.list") {
            continue;
        }
        let result = &frame["result"];
        match result["scope"].as_str() {
            Some("live") => live.extend(result["sessions"].as_array().cloned().unwrap_or_default()),
            Some("stored") => {
                stored.extend(result["sessions"].as_array().cloned().unwrap_or_default())
            }
            other => panic!("unexpected scope {other:?}"),
        }
    }
    assert!(!live.is_empty(), "the recording holds live rows");
    for row in live.iter().chain(stored.iter()) {
        let parsed = DirectoryRow::from_list_row("fixture", RowOrigin::Live, row);
        assert!(parsed.is_some(), "every recorded row reads: {row}");
    }

    // A title-less row and a missing model survive.
    let mut bare = live[0].clone();
    // A session of its own, so the merge keeps it beside the two live rows.
    bare["session_id"] = Value::from("bare-row");
    bare["title"] = Value::from("");
    bare.as_object_mut().expect("object").remove("model");
    let read = DirectoryRow::from_list_row("fixture", RowOrigin::Stored, &bare)
        .expect("the bare row reads");
    assert_eq!(read.title, "");
    assert_eq!(read.model, None);

    // Two live rows and one stored row merge and order.
    let first = DirectoryRow::from_list_row("fixture", RowOrigin::Live, &live[0]).expect("row");
    let second = DirectoryRow::from_list_row("fixture", RowOrigin::Live, &live[1]).expect("row");
    let merged = merge(vec![first, second], vec![read]);
    assert_eq!(merged.len(), 3);
    let mut merged = merged;
    order(&mut merged);
    assert_eq!(merged[0].origin, RowOrigin::Live);

    // The rename receipt and the export result match the frame log.
    let receipt = records
        .iter()
        .map(|record| &record["frame"]["result"])
        .find(|result| result["operation"] == "rename")
        .expect("a rename receipt");
    assert_eq!(receipt["revision"], Value::from(2));
    assert_eq!(receipt["title"], Value::from("fixture picker 88115"));
    let outcome = classify_result(receipt);
    assert!(matches!(outcome, MutationOutcome::Committed(_)));

    let export = records
        .iter()
        .map(|record| &record["frame"]["result"])
        .find(|result| result["format"] == "md")
        .expect("an export result");
    assert!(
        export["filename"]
            .as_str()
            .expect("filename")
            .starts_with("hermes_session_")
    );
    assert!(!export["content"].as_str().expect("content").is_empty());
}
