//! The recorded shell and side frames reduce to the expected cells.

#[path = "../../talaria-core/tests/support/events.rs"]
mod events;

use events::session_event;
use talaria_core::session::{Cell, Event, SessionId, SessionState, update};
use talaria_core::side::{RequestId, ShellOutcome};

// The recording of the shell-and-btw scenario against the pinned Hermes.
fn fixture_path() -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/frames/shell-and-btw.jsonl")
}

#[test]
fn recorded_shell_and_btw_reduce_to_expected_cells() {
    let text = std::fs::read_to_string(fixture_path()).expect("the recorded fixture");
    assert!(!text.contains("/Users/"));
    assert!(!text.contains("/home/"));
    assert!(!text.contains("\"unattended\""));
    let mut commands: Vec<String> = Vec::new();
    let mut shells: Vec<ShellOutcome> = Vec::new();
    let mut completions: Vec<serde_json::Value> = Vec::new();
    for line in text.lines().filter(|line| !line.trim().is_empty()) {
        let entry: serde_json::Value = serde_json::from_str(line).expect("one JSON entry per line");
        if entry["kind"] != "frame" {
            continue;
        }
        let frame = &entry["frame"];
        if entry["dir"] == "out" {
            if frame["method"] == "shell.exec" {
                commands.push(
                    frame["params"]["command"]
                        .as_str()
                        .unwrap_or_default()
                        .to_string(),
                );
            }
            continue;
        }
        if let Some(result) = frame.get("result") {
            if let Some(stdout) = result.get("stdout") {
                shells.push(ShellOutcome::Ok {
                    stdout: stdout.as_str().unwrap_or_default().to_string(),
                    stderr: result["stderr"].as_str().unwrap_or_default().to_string(),
                    code: result["code"].as_i64().unwrap_or_default(),
                });
            }
            continue;
        }
        if let Some(error) = frame.get("error") {
            shells.push(ShellOutcome::Refused {
                reason: error["data"]["reason"]
                    .as_str()
                    .unwrap_or_default()
                    .to_string(),
                detail: None,
                rpc_code: error["code"].as_i64().unwrap_or_default(),
            });
            continue;
        }
        if frame["method"] == "event" && frame["params"]["type"] == "btw.complete" {
            completions.push(frame["params"].clone());
        }
    }
    assert_eq!(shells.len(), 3, "two answers and one refusal");
    assert_eq!(completions.len(), 1, "one side answer");
    assert_eq!(commands.len(), 3, "three commands were sent");
    let mut state = SessionState::empty(SessionId::new("local-session-1"));
    for (index, (command, outcome)) in commands.into_iter().zip(shells).enumerate() {
        let id = u64::try_from(index + 1).unwrap_or(1);
        let _ = update(
            &mut state,
            Event::ShellRequested {
                command,
                started_ms: 0,
            },
        );
        let _ = update(
            &mut state,
            Event::ShellFinished {
                request_id: RequestId(id),
                outcome,
                duration_ms: 1,
            },
        );
    }
    for params in completions {
        let _ = update(&mut state, session_event("local-session-1", params));
    }
    let texts: Vec<String> = state
        .cells()
        .iter()
        .filter_map(|cell| match cell {
            Cell::Shell(cell) => Some(format!("shell {}", cell.command)),
            Cell::Side(cell) => match &cell.outcome {
                talaria_core::side::SideOutcome::Answered { text } => Some(format!("side {text}")),
                _ => None,
            },
            _ => None,
        })
        .collect();
    assert!(texts.iter().any(|text| text.contains("talaria-shell-ok")));
    assert!(texts.iter().any(|text| text.contains("oops")));
    assert_eq!(
        texts
            .iter()
            .filter(|text| text.starts_with("shell"))
            .count(),
        3
    );
    assert!(texts.iter().any(|text| text.contains("ready")));
}
