//! Recorded tool cells, drawn as text. Issue 221.
//!
//! A recorded edit renders as a numbered diff without a gateway.

#[path = "support/replay_tools.rs"]
mod replay_tools;

use std::path::PathBuf;

use ratatui::text::Line;
use replay_tools::{inbound_events, replay_log, tool_cells};
use serde_json::Value;
use talaria_core::session::{SessionState, ToolCell};
use talaria_tui::tool_cell::{Detail, ToolRenderOptions, ToolTheme, tool_lines};

fn frames(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join("fixtures")
        .join("frames")
        .join(name)
}

fn completes(events: &[Value]) -> Vec<&Value> {
    events
        .iter()
        .filter(|params| params.get("type").and_then(Value::as_str) == Some("tool.complete"))
        .collect()
}

fn payload(event: &Value) -> &Value {
    event.get("payload").unwrap_or(&Value::Null)
}

/// The reducer kept one cell for each finished call.
fn assert_reduced(state: &SessionState, events: &[Value]) {
    assert!(!state.needs_resume(), "the recording was not applied");
    let tools = tool_cells(state);
    let finished = completes(events);
    assert_eq!(
        tools.len(),
        finished.len(),
        "each tool.complete is one cell"
    );
    for event in &finished {
        let body = payload(event);
        let name = body.get("name").and_then(Value::as_str).unwrap_or("");
        let id = body.get("tool_id").and_then(Value::as_str).unwrap_or("");
        assert!(
            tools
                .iter()
                .any(|tool| tool.name == name && tool.tool_id == id),
            "{name} {id}"
        );
    }
}

fn render(state: &SessionState) -> Vec<String> {
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

fn headers_starting<'a>(rows: &'a [String], prefix: &str) -> Vec<&'a String> {
    rows.iter()
        .filter(|row| row.starts_with('⏺') && row.starts_with(prefix))
        .collect()
}

fn count_name(events: &[Value], name: &str) -> usize {
    completes(events)
        .iter()
        .filter(|event| payload(event).get("name").and_then(Value::as_str) == Some(name))
        .count()
}

fn assert_plain(rows: &[String]) {
    for row in rows {
        assert!(!row.contains('┊'), "{row}");
        assert!(
            !row.chars().any(|ch| ch == '\u{1b}' || ch == '\u{7}'),
            "{row}"
        );
    }
}

fn patch_cell(state: &SessionState) -> &ToolCell {
    tool_cells(state)
        .into_iter()
        .find(|tool| tool.name == "patch")
        .expect("patch cell")
}

#[test]
fn recorded_tool_fixtures_render_expected_screens() {
    // source: fixtures/frames/tool-diff.jsonl
    let diff_path = frames("tool-diff.jsonl");
    let diff_events = inbound_events(&diff_path);
    let diff_state = replay_log(&diff_path);
    assert_reduced(&diff_state, &diff_events);
    let patch = patch_cell(&diff_state);
    let diff_text = patch.diff.as_deref().expect("the edit stored a diff");
    assert!(diff_text.contains("notes.txt"));
    assert!(diff_text.contains("beta") && diff_text.contains("gamma"));

    // source: fixtures/frames/approval.jsonl
    let approval_path = frames("approval.jsonl");
    let approval_events = inbound_events(&approval_path);
    let approval_state = replay_log(&approval_path);
    assert_reduced(&approval_state, &approval_events);
    let shell = tool_cells(&approval_state)
        .into_iter()
        .find(|tool| tool.name == "terminal")
        .expect("terminal cell");
    let command = shell
        .args
        .as_ref()
        .and_then(|args| args.get("command"))
        .and_then(Value::as_str)
        .unwrap_or("");
    assert!(command.contains("rm -rf .pytest_cache"));

    let diff_rows = render(&diff_state);
    assert_plain(&diff_rows);
    assert_eq!(
        headers_starting(&diff_rows, "⏺ Search").len(),
        count_name(&diff_events, "search_files")
    );
    assert_eq!(
        headers_starting(&diff_rows, "⏺ Read").len(),
        count_name(&diff_events, "read_file")
    );
    let edit_headers: Vec<_> = diff_rows
        .iter()
        .filter(|row| row.starts_with("⏺ Edit ") || row.starts_with("⏺ Write "))
        .collect();
    assert_eq!(edit_headers.len(), count_name(&diff_events, "patch"));
    let header_at = diff_rows
        .iter()
        .position(|row| {
            (row.starts_with("⏺ Edit ") || row.starts_with("⏺ Write "))
                && row.contains("notes.txt")
                && row.contains("✓ +1 -1")
        })
        .expect("edit header");
    let mut cursor = header_at + 1;
    for expected in ["    1   alpha", "    2 - beta", "    2 + gamma"] {
        let found = diff_rows[cursor..]
            .iter()
            .position(|row| row == expected)
            .unwrap_or_else(|| panic!("missing {expected}"));
        cursor += found + 1;
    }

    let approval_rows = render(&approval_state);
    assert_plain(&approval_rows);
    let shells = headers_starting(&approval_rows, "⏺ Shell");
    assert_eq!(shells.len(), count_name(&approval_events, "terminal"));
    assert!(
        approval_rows.iter().any(|row| {
            row.starts_with("⏺ Shell(rm -rf .pytest_cache") && row.contains('✓')
        })
    );
}
