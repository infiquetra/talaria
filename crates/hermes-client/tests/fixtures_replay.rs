//! The fixtures under `fixtures/frames/` exist, replay in order through the shared decode
//! function, and carry the evidence each scenario promises.
//!
//! These tests read the real files. They fail with the message
//! `expected twelve fixtures in fixtures/frames, found N` when a file is missing. A failure
//! names the fixture and the rule, never the content of a frame. The directory also holds
//! the model-switch recording, covered by its own replay tests.

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::AtomicU64;

use hermes_client::{
    Decoded, Generation, Inbound, MalformedKind, ReplayItem, ReplaySource, Responder, Speed,
    decode_frame,
};
use serde_json::Value;
use tokio::sync::mpsc;

const NAMES: [&str; 11] = [
    "plain-answer",
    "tool-diff",
    "approval",
    "clarification",
    "interrupted",
    "subagent-batch",
    "queued-prompt",
    "slash-palette",
    "rewind-and-rollback",
    "context-compaction",
    "shell-and-btw",
];

/// What each fixture's header must list as `expectedStatuses`, from the scenario table.
fn expected_statuses(name: &str) -> &'static [&'static str] {
    match name {
        "interrupted" => &["interrupted"],
        "queued-prompt" => &["complete", "complete"],
        "rewind-and-rollback" => &["complete", "complete", "complete"],
        "context-compaction" => &["complete"; 17],
        _ => &["complete"],
    }
}

fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join("fixtures/frames")
}

/// The `*.jsonl` files directly inside `fixtures/frames`, in name order. A missing directory
/// counts as none.
fn jsonl_files() -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(fixtures_dir()) else {
        return Vec::new();
    };
    let mut files: Vec<PathBuf> = entries
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "jsonl"))
        .collect();
    files.sort();
    files
}

/// Asserts the nine fixtures exist, then returns `(name, path)` for each, in the order of
/// `NAMES`. Every test in this file starts here so that none of them can index into a missing
/// file.
fn all_fixtures() -> Vec<(&'static str, PathBuf)> {
    let files = jsonl_files();
    assert!(
        files.len() == 12,
        "expected twelve fixtures in fixtures/frames, found {}",
        files.len()
    );
    NAMES
        .iter()
        .map(|name| {
            let path = fixtures_dir().join(format!("{name}.jsonl"));
            assert!(
                files.contains(&path),
                "expected fixture {name}.jsonl in fixtures/frames"
            );
            (*name, path)
        })
        .collect()
}

/// Every line of a fixture, parsed. A line that is not JSON fails with the file name and number.
fn records(name: &str, path: &PathBuf) -> Vec<Value> {
    let text =
        std::fs::read_to_string(path).unwrap_or_else(|_| panic!("{name}.jsonl could not be read"));
    assert!(
        text.ends_with('\n'),
        "{name}.jsonl does not end with a newline"
    );
    text.lines()
        .enumerate()
        .map(|(index, line)| {
            serde_json::from_str(line)
                .unwrap_or_else(|_| panic!("{name}.jsonl line {} is not JSON", index + 1))
        })
        .collect()
}

/// `(event type, params)` of every inbound event frame, in file order.
fn inbound_events(records: &[Value]) -> Vec<(String, Value)> {
    records
        .iter()
        .filter(|record| record["kind"] == "frame" && record["dir"] == "in")
        .filter(|record| record["frame"]["method"] == "event")
        .filter_map(|record| {
            let params = &record["frame"]["params"];
            Some((params["type"].as_str()?.to_owned(), params.clone()))
        })
        .collect()
}

fn position(events: &[(String, Value)], kind: &str, from: usize) -> Option<usize> {
    events
        .iter()
        .enumerate()
        .skip(from)
        .find(|(_, (event_kind, _))| event_kind == kind)
        .map(|(index, _)| index)
}

fn completed_statuses(events: &[(String, Value)]) -> Vec<String> {
    events
        .iter()
        .filter(|(kind, _)| kind == "message.complete")
        .map(|(_, params)| {
            params["payload"]["status"]
                .as_str()
                .unwrap_or("")
                .to_owned()
        })
        .collect()
}

#[test]
fn every_fixture_exists() {
    let fixtures = all_fixtures();
    assert_eq!(fixtures.len(), 11);
}

#[test]
fn each_has_v3_header_and_gapless_seq() {
    for (name, path) in all_fixtures() {
        let records = records(name, &path);
        let header = &records[0];
        assert_eq!(header["kind"], "header", "{name}: line 1 is not a header");
        assert_eq!(header["version"], 3, "{name}: header version is not 3");
        for field in ["startedAt", "endpoint"] {
            assert!(
                header[field].as_str().is_some_and(|text| !text.is_empty()),
                "{name}: header has no {field}"
            );
        }
        assert!(
            header["subprotocols"].is_array(),
            "{name}: header has no subprotocols list"
        );
        assert_eq!(
            header["scenario"].as_str(),
            Some(name),
            "{name}: header scenario does not match the file name"
        );
        let wanted: Vec<&str> = expected_statuses(name).to_vec();
        let listed: Vec<&str> = header["expectedStatuses"]
            .as_array()
            .map(|list| list.iter().filter_map(Value::as_str).collect())
            .unwrap_or_default();
        assert_eq!(listed, wanted, "{name}: header expectedStatuses");

        let mut last_t_ms = 0_u64;
        for (index, record) in records.iter().enumerate().skip(1) {
            let line = index + 1;
            assert_eq!(
                record["seq"].as_u64(),
                Some(index as u64),
                "{name} line {line}: seq is not gapless from 1"
            );
            let kind = record["kind"].as_str().unwrap_or("");
            assert!(
                kind == "frame" || kind == "dropped",
                "{name} line {line}: unexpected record kind"
            );
            let t_ms = record["tMs"].as_u64();
            assert!(t_ms.is_some(), "{name} line {line}: tMs missing");
            let t_ms = t_ms.unwrap_or(0);
            assert!(t_ms >= last_t_ms, "{name} line {line}: tMs went backwards");
            last_t_ms = t_ms;
            if kind == "frame" {
                assert!(
                    matches!(record["dir"].as_str(), Some("in" | "out")),
                    "{name} line {line}: dir is neither in nor out"
                );
                assert!(
                    record["generation"].as_u64().is_some(),
                    "{name} line {line}: generation missing"
                );
            }
        }
    }
}

#[test]
fn each_ends_with_expected_status() {
    for (name, path) in all_fixtures() {
        let source = ReplaySource::open(&path)
            .unwrap_or_else(|error| panic!("{name}: could not open the fixture: {error}"));
        assert!(!source.truncated(), "{name}: the recording is cut short");
        // The same rule the `replay` command uses for "final statuses".
        let statuses: Vec<String> = source
            .inbound()
            .filter(|frame| {
                frame.frame["method"] == "event"
                    && frame.frame["params"]["type"] == "message.complete"
            })
            .map(|frame| {
                frame.frame["params"]["payload"]["status"]
                    .as_str()
                    .unwrap_or("")
                    .to_owned()
            })
            .collect();
        let wanted: Vec<String> = expected_statuses(name)
            .iter()
            .map(|status| (*status).to_owned())
            .collect();
        assert_eq!(statuses, wanted, "{name}: final statuses");
        assert_eq!(
            source.header().expected_statuses,
            wanted,
            "{name}: header expectedStatuses"
        );
    }
}

#[tokio::test]
async fn each_replays_recorded_inbound_sequence() {
    for (name, path) in all_fixtures() {
        // What the recorder wrote, read straight from the file.
        let written: Vec<(u64, Value)> = records(name, &path)
            .iter()
            .filter(|record| record["kind"] == "frame" && record["dir"] == "in")
            .filter(|record| !record["frame"].is_null())
            .map(|record| {
                (
                    record["generation"].as_u64().unwrap_or(0),
                    record["frame"].clone(),
                )
            })
            .collect();
        assert!(!written.is_empty(), "{name}: no inbound frames recorded");

        let source = ReplaySource::open(&path)
            .unwrap_or_else(|error| panic!("{name}: could not open the fixture: {error}"));
        let (sink, mut receiver) = mpsc::channel(written.len() + 1);
        let sent = source
            .run(Speed::Instant, sink)
            .await
            .unwrap_or_else(|error| panic!("{name}: replay failed: {error}"));
        let mut items: Vec<ReplayItem> = Vec::new();
        while let Ok(item) = receiver.try_recv() {
            items.push(item);
        }
        assert_eq!(sent, written.len(), "{name}: frames sent");
        assert_eq!(items.len(), written.len(), "{name}: frames received");

        let swallowed = Arc::new(AtomicU64::new(0));
        for (index, (item, (generation, frame))) in items.iter().zip(&written).enumerate() {
            let number = index + 1;
            assert_eq!(
                item.generation, *generation,
                "{name}: frame {number} generation"
            );
            let parsed: Value = serde_json::from_str(&item.text)
                .unwrap_or_else(|_| panic!("{name}: replayed frame {number} is not JSON"));
            assert_eq!(&parsed, frame, "{name}: replayed frame {number} differs");

            // Through the same decode function the live reader uses.
            let counter = Arc::clone(&swallowed);
            let decoded = decode_frame(
                Generation(item.generation),
                item.text.as_bytes(),
                &mut |_id| Responder::detached(Arc::clone(&counter)),
            );
            let is_response = frame.get("method").is_none()
                && !frame["id"].is_null()
                && (frame.get("result").is_some() || frame.get("error").is_some());
            match decoded {
                Decoded::Reply(_) => assert!(
                    is_response,
                    "{name}: frame {number} decoded as a reply but is not a response"
                ),
                Decoded::Inbound(Inbound::Malformed(malformed)) => assert!(
                    !matches!(
                        malformed.kind,
                        MalformedKind::NotJson | MalformedKind::NotObject
                    ),
                    "{name}: frame {number} was stored as JSON but decodes as not JSON"
                ),
                Decoded::Inbound(Inbound::Event(_) | Inbound::ServerRequest(_)) => assert!(
                    !is_response,
                    "{name}: frame {number} is a response but decoded as an inbound item"
                ),
                Decoded::Inbound(Inbound::Closed(_)) => {
                    panic!("{name}: frame {number} decoded as a closed connection")
                }
            }
        }
    }
}

/// Issue #223: a catalogue with a skill, two completion replies, five slash replies, and a
/// finished turn. Requests are matched to replies in file order because a later call can reuse
/// an earlier JSON-RPC id.
fn slash_palette_evidence(name: &str, records: &[Value], events: &[(String, Value)]) {
    let catalog = records.iter().any(|record| {
        record["dir"] == "in"
            && record["frame"]["result"]["skills"]
                .as_object()
                .is_some_and(|skills| !skills.is_empty())
    });
    assert!(catalog, "{name}: no commands.catalog result with a skill");
    let mut method_for: std::collections::HashMap<i64, String> = std::collections::HashMap::new();
    let mut complete = 0;
    let mut exec = 0;
    for record in records {
        let frame = &record["frame"];
        let Some(id) = frame["id"].as_i64() else {
            continue;
        };
        if record["dir"] == "out" {
            if let Some(method) = frame["method"].as_str() {
                method_for.insert(id, method.to_owned());
            }
        } else if record["dir"] == "in" {
            let Some(method) = method_for.get(&id) else {
                continue;
            };
            let replied = frame.get("result").is_some() || frame.get("error").is_some();
            if !replied {
                continue;
            }
            if method == "complete.slash" && frame["result"]["items"].is_array() {
                complete += 1;
            }
            if method == "slash.exec" {
                exec += 1;
            }
        }
    }
    assert_eq!(complete, 2, "{name}: complete.slash results");
    assert_eq!(exec, 5, "{name}: slash.exec replies");
    assert!(
        events.iter().any(|(kind, _)| kind == "message.complete"),
        "{name}: no message.complete"
    );
}

#[test]
fn scenario_evidence_present() {
    for (name, path) in all_fixtures() {
        let records = records(name, &path);
        let events = inbound_events(&records);
        assert!(!events.is_empty(), "{name}: no inbound events");
        match name {
            "plain-answer" => {
                assert!(
                    position(&events, "message.delta", 0).is_some(),
                    "{name}: no message.delta"
                );
                assert!(
                    position(&events, "tool.start", 0).is_none(),
                    "{name}: a tool.start appears in a no-tools scenario"
                );
            }
            "tool-diff" => {
                let has_diff = events.iter().any(|(kind, params)| {
                    kind == "tool.complete"
                        && params["payload"]["inline_diff"]
                            .as_str()
                            .is_some_and(|diff| !diff.is_empty())
                });
                assert!(
                    has_diff,
                    "{name}: no tool.complete with a non-empty inline_diff"
                );
            }
            "approval" => {
                let request = position(&events, "approval.request", 0);
                assert!(request.is_some(), "{name}: no approval.request");
                let settled = position(&events, "approval.settled", request.unwrap_or(0));
                assert!(
                    settled.is_some(),
                    "{name}: no approval.settled after the approval.request"
                );
            }
            "clarification" => {
                let request = position(&events, "clarify.request", 0);
                assert!(request.is_some(), "{name}: no clarify.request");
                let settled = position(&events, "clarify.settled", request.unwrap_or(0));
                assert!(
                    settled.is_some(),
                    "{name}: no clarify.settled after the clarify.request"
                );
            }
            "interrupted" => {
                assert!(
                    completed_statuses(&events)
                        .iter()
                        .any(|status| status == "interrupted"),
                    "{name}: no message.complete with status interrupted"
                );
            }
            "subagent-batch" => {
                assert!(
                    position(&events, "subagent.start", 0).is_some(),
                    "{name}: no subagent.start"
                );
                assert!(
                    position(&events, "subagent.complete", 0).is_some(),
                    "{name}: no subagent.complete"
                );
            }
            "queued-prompt" => {
                let first_complete = position(&events, "message.complete", 0);
                assert!(first_complete.is_some(), "{name}: no message.complete");
                let first_complete = first_complete.unwrap_or(0);
                let first_start = position(&events, "message.start", 0);
                assert!(first_start.is_some(), "{name}: no message.start");
                // The second turn starts only after the first one completed.
                let second_start = position(&events, "message.start", first_complete + 1);
                assert!(
                    second_start.is_some(),
                    "{name}: no second message.start after the first message.complete"
                );
                assert_eq!(
                    completed_statuses(&events),
                    ["complete", "complete"],
                    "{name}: message.complete statuses"
                );
            }
            "slash-palette" => slash_palette_evidence(name, &records, &events),
            "rewind-and-rollback" => {
                let sent: Vec<&Value> = records
                    .iter()
                    .filter(|record| record["dir"] == "out")
                    .map(|record| &record["frame"])
                    .collect();
                for method in [
                    "rollback.list",
                    "rollback.diff",
                    "rollback.restore",
                    "rollback.session_diff",
                ] {
                    assert!(
                        sent.iter().any(|frame| frame["method"] == method),
                        "{name}: no outbound {method}"
                    );
                }
                assert!(
                    sent.iter().any(|frame| frame["method"] == "session.mutate"
                        && frame["params"]["operation"] == "rewind"),
                    "{name}: no session.mutate with operation rewind"
                );
                assert!(
                    position(&events, "session.updated", 0).is_some(),
                    "{name}: no session.updated after the rewind"
                );
            }
            "context-compaction" => {
                assert!(
                    position(&events, "session.updated", 0).is_some(),
                    "{name}: no session.updated"
                );
            }
            "shell-and-btw" => {
                assert!(
                    position(&events, "btw.complete", 0).is_some(),
                    "{name}: no btw.complete"
                );
                let refused = records.iter().any(|record| {
                    record["kind"] == "frame"
                        && record["dir"] == "in"
                        && record["frame"]["error"]["data"]["reason"] == "dangerous_command"
                });
                assert!(refused, "{name}: no dangerous_command refusal");
            }
            other => panic!("unlisted fixture {other}"),
        }
    }
}

#[test]
fn clarify_answer_is_withheld() {
    let fixtures = all_fixtures();
    let (name, path) = fixtures
        .iter()
        .find(|(name, _)| *name == "clarification")
        .expect("the clarification fixture is in the list");
    let records = records(name, path);
    let answers: Vec<(usize, &Value)> = records
        .iter()
        .enumerate()
        .filter(|(_, record)| record["kind"] == "frame" && record["dir"] == "out")
        .filter(|(_, record)| record["frame"]["method"] == "clarify.respond")
        .collect();
    assert!(
        !answers.is_empty(),
        "{name}: no outbound clarify.respond was recorded"
    );
    for (index, record) in answers {
        let line = index + 1;
        let reasons: Vec<&str> = record["redactions"]
            .as_array()
            .map(|list| {
                list.iter()
                    .filter_map(|entry| entry["reason"].as_str())
                    .collect()
            })
            .unwrap_or_default();
        assert!(
            reasons.contains(&"deny-set:clarify.respond"),
            "{name} line {line}: the answer carries no deny-set:clarify.respond redaction"
        );
        assert_eq!(
            record["frame"]["params"]["answer"], "[redacted]",
            "{name} line {line}: the answer was not replaced by the marker"
        );
    }
}
