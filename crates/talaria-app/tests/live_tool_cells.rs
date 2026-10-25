//! Render frame logs recorded on the pinned Hermes. Ignored unless the operator asks.
//!
//! Issue 221. `TALARIA_LIVE_FRAMES` is a directory of `*.jsonl` logs, or one file.
//! Unset or empty prints `LIVE_TOOL_CELLS_RESULT: SKIPPED` and returns.
//! `TALARIA_LIVE_REQUIRE=1` turns that skip into a failure.
//! A run that copies the checked-in frame logs is a red run only, never live evidence.

#[path = "support/replay_tools.rs"]
mod replay_tools;

use std::path::{Path, PathBuf};

use ratatui::text::Line;
use replay_tools::{inbound_events, replay_log, tool_cells};
use serde_json::Value;
use talaria_tui::tool_cell::{Detail, ToolRenderOptions, ToolTheme, tool_lines};

const RESULT: &str = "LIVE_TOOL_CELLS_RESULT";

#[test]
#[ignore = "needs TALARIA_LIVE_FRAMES from a recording on the pinned Hermes"]
fn live_recordings_render_tool_cells() {
    let Some(raw) = std::env::var_os("TALARIA_LIVE_FRAMES") else {
        skip("set TALARIA_LIVE_FRAMES to a directory of frame logs recorded on the pinned Hermes");
        return;
    };
    let logs = frame_logs(&PathBuf::from(raw));
    if logs.is_empty() {
        skip("no frame logs in TALARIA_LIVE_FRAMES");
        return;
    }
    let mut all_completes = Vec::new();
    for log in &logs {
        all_completes.extend(completes(&inbound_events(log)));
    }
    let edit = all_completes.iter().any(|body| {
        body.get("inline_diff")
            .and_then(Value::as_str)
            .is_some_and(|text| !text.is_empty())
    });
    let shell = all_completes
        .iter()
        .any(|body| body.get("name").and_then(Value::as_str) == Some("terminal"));
    if !edit || !shell {
        skip("recording has no edit or no shell call; re-record");
        return;
    }
    for body in &all_completes {
        assert!(
            body.get("is_error").and_then(Value::as_bool).is_some(),
            "tool.complete is_error is a boolean"
        );
        assert!(
            body.get("result").and_then(Value::as_str).is_some(),
            "tool.complete result is a string"
        );
    }
    let mut prepared = Vec::new();
    for log in &logs {
        let events = inbound_events(log);
        let state = replay_log(log);
        let label = log
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("frame-log");
        assert!(!state.needs_resume(), "{label} was not applied");
        assert_eq!(
            tool_cells(&state).len(),
            completes(&events).len(),
            "{label} cell count"
        );
        prepared.push(state);
    }
    let mut saw_diff = false;
    let mut saw_shell = false;
    for state in &prepared {
        for row in render(state) {
            if numbered_change(&row) {
                saw_diff = true;
            }
            if row.starts_with("⏺ Shell") && row.contains('✓') {
                saw_shell = true;
            }
        }
    }
    assert!(saw_diff, "a numbered diff row");
    assert!(saw_shell, "a shell header with a success mark");
    println!("{RESULT}: PASSED");
}

fn completes(events: &[Value]) -> Vec<Value> {
    events
        .iter()
        .filter(|params| params.get("type").and_then(Value::as_str) == Some("tool.complete"))
        .filter_map(|params| params.get("payload").cloned())
        .collect()
}

fn render(state: &talaria_core::session::SessionState) -> Vec<String> {
    let options = ToolRenderOptions {
        width: 100,
        theme: ToolTheme::default(),
        color: false,
        detail: Detail::Compact,
        expand_hint: None,
        elapsed: None,
    };
    let mut rows = Vec::new();
    for tool in tool_cells(state) {
        for line in tool_lines(tool, &options) {
            rows.push(row_text(&line));
        }
    }
    rows
}

fn row_text(line: &Line<'_>) -> String {
    line.spans
        .iter()
        .map(|span| span.content.as_ref())
        .collect()
}

/// `    2 - beta` and `    2 + gamma`. Four spaces, a line number, then a sign.
fn numbered_change(row: &str) -> bool {
    let Some(rest) = row.strip_prefix("    ") else {
        return false;
    };
    let rest = rest.trim_start();
    let Some((digits, rest)) = rest.split_once(' ') else {
        return false;
    };
    !digits.is_empty()
        && digits.chars().all(|ch| ch.is_ascii_digit())
        && (rest.starts_with("+ ") || rest.starts_with("- "))
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

fn skip(reason: &str) {
    println!("{RESULT}: SKIPPED ({reason})");
    if std::env::var("TALARIA_LIVE_REQUIRE").ok().as_deref() == Some("1") {
        panic!("{RESULT}: SKIPPED ({reason})");
    }
}
