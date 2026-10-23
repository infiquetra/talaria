//! Reduce frame logs recorded against the pinned Hermes. Ignored unless the operator asks.
//!
//! `TALARIA_LIVE_FRAMES` is a directory of `*.jsonl` logs, or one file. Unset or empty passes
//! with `SKIPPED`, unless `TALARIA_LIVE_REQUIRE=1`.

#[path = "support/drive.rs"]
mod drive;
#[path = "support/frames.rs"]
mod frames;

use std::path::{Path, PathBuf};

use serde_json::Value;
use talaria_core::session::{Cell, Event, SessionState, ToolStatus, TurnOutcome, TurnState};

#[test]
#[ignore = "needs TALARIA_LIVE_FRAMES from tools/record-fixture on the pinned Hermes"]
fn live_recording_reduces_to_expected_cells() {
    let Some(raw) = std::env::var_os("TALARIA_LIVE_FRAMES") else {
        skipped(
            "set TALARIA_LIVE_FRAMES to a directory of frame logs recorded by tools/record-fixture on the pinned Hermes",
        );
        return;
    };
    let path = PathBuf::from(raw);
    let logs = frame_logs(&path);
    if logs.is_empty() {
        skipped("no frame logs in TALARIA_LIVE_FRAMES");
        return;
    }
    let mut unknown = Vec::new();
    for log_path in logs {
        let base = log_path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("frame-log")
            .to_string();
        let text = std::fs::read_to_string(&log_path).unwrap_or_else(|_| {
            panic!("LIVE_REDUCE_RESULT: FAILED {base}: the frame log could not be read")
        });
        let log = frames::read_str(&text)
            .unwrap_or_else(|error| panic!("LIVE_REDUCE_RESULT: FAILED {base}: {error}"));
        let (state, _) = reduce_log(&log);
        check_log(&base, &text, &state);
        for name in state.unknown_event_types() {
            if !unknown.contains(name) {
                unknown.push(name.clone());
            }
        }
        println!("LIVE_REDUCE_RESULT: PASSED {base}");
    }
    if unknown.is_empty() {
        println!("unknown_event_types: none");
    } else {
        println!("unknown_event_types: {}", unknown.join(", "));
    }
}

fn skipped(reason: &str) {
    println!("LIVE_REDUCE_RESULT: SKIPPED ({reason})");
    if std::env::var("TALARIA_LIVE_REQUIRE").ok().as_deref() == Some("1") {
        panic!("LIVE_REDUCE_RESULT: SKIPPED ({reason})");
    }
}

fn frame_logs(path: &Path) -> Vec<PathBuf> {
    if path.is_file() {
        return vec![path.to_path_buf()];
    }
    let Ok(entries) = std::fs::read_dir(path) else {
        return Vec::new();
    };
    let mut logs: Vec<PathBuf> = entries
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.path())
        .filter(|path| path.extension().and_then(|ext| ext.to_str()) == Some("jsonl"))
        .collect();
    logs.sort();
    logs
}

fn reduce_log(log: &frames::FrameLog) -> (SessionState, Vec<talaria_core::session::Effect>) {
    if let Some(snapshot) = log.initial_snapshot.clone() {
        return drive::reduce_snapshot(snapshot, log.events.clone());
    }
    let session = log
        .events
        .iter()
        .find_map(|event| match event {
            Event::Session { session_id, .. } if !session_id.as_str().is_empty() => {
                Some(session_id.as_str().to_string())
            }
            _ => None,
        })
        .unwrap_or_else(|| "local-session-1".to_string());
    drive::reduce(&session, log.events.clone())
}

fn check_log(base: &str, text: &str, state: &SessionState) {
    let params = inbound_events(text);
    let completes: Vec<&Value> = params
        .iter()
        .filter(|params| params.get("type").and_then(Value::as_str) == Some("message.complete"))
        .collect();
    if let Some(last) = completes.last() {
        let status = last.pointer("/payload/status");
        let interrupted = turn_had_outbound_interrupt(text);
        let expected = if interrupted {
            TurnOutcome::Interrupted
        } else {
            match status {
                None | Some(Value::Null) => TurnOutcome::Completed,
                Some(value) if value.as_str() == Some("complete") => TurnOutcome::Completed,
                Some(value) if value.as_str() == Some("interrupted") => TurnOutcome::Interrupted,
                _ => TurnOutcome::Failed,
            }
        };
        assert_eq!(state.turn(), &TurnState::Finished(expected), "{base}");
        if status.and_then(Value::as_str) == Some("complete")
            && !interrupted
            && let Some(reply) = last.pointer("/payload/text").and_then(Value::as_str)
        {
            let expected_text = without_whitespace(reply);
            if !expected_text.is_empty() {
                let shown = assistant_after_last_user(state);
                assert!(
                    without_whitespace(&shown).contains(&expected_text),
                    "{base}: message.complete text is not in the assistant cells"
                );
            }
        }
    }
    for tool_id in tool_start_ids_before_interrupt(text) {
        let found = state.cells().iter().any(|cell| {
            matches!(cell, Cell::Tool(tool) if tool.tool_id == tool_id && tool.status != ToolStatus::Running)
        });
        assert!(found, "{base}: tool {tool_id} has no final cell");
    }
}

fn inbound_events(text: &str) -> Vec<Value> {
    let mut found = Vec::new();
    for line in text.lines() {
        let Ok(value) = serde_json::from_str::<Value>(line) else {
            continue;
        };
        if value.get("dir").and_then(Value::as_str) != Some("in") {
            continue;
        }
        let Some(frame) = value.get("frame") else {
            continue;
        };
        if frame.get("method").and_then(Value::as_str) != Some("event") {
            continue;
        }
        if let Some(params) = frame.get("params") {
            found.push(params.clone());
        }
    }
    found
}

/// Tool ids started before the turn's outbound interrupt, if any: once an interrupt is
/// sent the reducer suppresses later tool events until the turn ends, so a tool that
/// starts after the interrupt rightly has no cell.
fn tool_start_ids_before_interrupt(text: &str) -> Vec<String> {
    let mut ids = Vec::new();
    let mut latched = false;
    for line in text.lines() {
        let Ok(value) = serde_json::from_str::<Value>(line) else {
            continue;
        };
        let Some(frame) = value.get("frame") else {
            continue;
        };
        let dir = value.get("dir").and_then(Value::as_str).unwrap_or("");
        if dir == "out" && frame.get("method").and_then(Value::as_str) == Some("session.interrupt")
        {
            latched = true;
        }
        if dir == "in" && frame.get("method").and_then(Value::as_str) == Some("event") {
            match frame.pointer("/params/type").and_then(Value::as_str) {
                Some("message.complete") => latched = false,
                Some("tool.start") if !latched => {
                    if let Some(id) = frame
                        .pointer("/params/payload/tool_id")
                        .and_then(Value::as_str)
                    {
                        ids.push(id.to_string());
                    }
                }
                _ => {}
            }
        }
    }
    ids
}

fn turn_had_outbound_interrupt(text: &str) -> bool {
    let mut in_turn = false;
    for line in text.lines() {
        let Ok(value) = serde_json::from_str::<Value>(line) else {
            continue;
        };
        let Some(frame) = value.get("frame") else {
            continue;
        };
        let dir = value.get("dir").and_then(Value::as_str).unwrap_or("");
        if dir == "in" && frame.get("method").and_then(Value::as_str) == Some("event") {
            match frame.pointer("/params/type").and_then(Value::as_str) {
                Some("message.start") => in_turn = true,
                Some("message.complete") => in_turn = false,
                _ => {}
            }
        }
        if in_turn
            && dir == "out"
            && frame.get("method").and_then(Value::as_str) == Some("session.interrupt")
        {
            return true;
        }
    }
    false
}

fn assistant_after_last_user(state: &SessionState) -> String {
    let cells = state.cells();
    let start = cells
        .iter()
        .rposition(|cell| matches!(cell, Cell::User(_)))
        .map(|index| index + 1)
        .unwrap_or(0);
    cells[start..]
        .iter()
        .filter_map(|cell| match cell {
            Cell::Assistant(cell) => Some(cell.text.as_str()),
            _ => None,
        })
        .collect()
}

fn without_whitespace(text: &str) -> String {
    text.chars().filter(|ch| !ch.is_whitespace()).collect()
}
