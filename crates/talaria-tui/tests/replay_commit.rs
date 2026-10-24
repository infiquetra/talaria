//! Replays a recorded turn that carries a tool diff through the reducer and commits history after
//! every event, as the application will. Each final cell must reach scrollback once, in order,
//! with no escape byte in the text rows.

mod support;

use std::path::PathBuf;

use serde_json::Value;
use support::frames::{self, FrameLog};
use support::{Alac, ByteSink, probe};
use talaria_core::session::{Cell, Event, SessionId, SessionState, ToolStatus, update};
use talaria_tui::history::{CommitLedger, PlainCellLines};
use talaria_tui::terminal::{InlineTerminal, RestoreLink};

const COLS: u16 = 200;
const ROWS: u16 = 24;
const LIVE_HEIGHT: u16 = 2;
const ESCAPE: char = '\u{1b}';

fn frames_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/frames")
}

/// Whether the log text holds a `tool.complete` event whose `inline_diff` is a non-empty string.
fn has_inline_diff(text: &str) -> bool {
    text.lines().any(|line| {
        let Ok(record) = serde_json::from_str::<Value>(line) else {
            return false;
        };
        let Some(params) = record.get("frame").and_then(|frame| frame.get("params")) else {
            return false;
        };
        params.get("type").and_then(Value::as_str) == Some("tool.complete")
            && params
                .get("payload")
                .and_then(|payload| payload.get("inline_diff"))
                .and_then(Value::as_str)
                .is_some_and(|diff| !diff.is_empty())
    })
}

/// Every recorded log under the fixtures directory that has a non-empty `inline_diff`, found by
/// reading the logs, never by name. Fails when there is none.
fn logs_with_inline_diff() -> Vec<(String, FrameLog)> {
    let dir = frames_dir();
    let mut paths: Vec<PathBuf> = std::fs::read_dir(&dir)
        .unwrap_or_else(|error| panic!("cannot read the frame fixtures directory: {error}"))
        .map(|entry| entry.expect("directory entry").path())
        .filter(|path| {
            path.extension()
                .is_some_and(|extension| extension == "jsonl")
        })
        .collect();
    paths.sort();

    let mut found = Vec::new();
    for path in &paths {
        let name = path
            .file_name()
            .expect("fixture file name")
            .to_string_lossy()
            .into_owned();
        let text = std::fs::read_to_string(path)
            .unwrap_or_else(|error| panic!("cannot read fixture {name}: {error}"));
        if !has_inline_diff(&text) {
            continue;
        }
        let log = frames::read_str(&text)
            .unwrap_or_else(|error| panic!("fixture {name} is not a readable frame log: {error}"));
        found.push((name, log));
    }
    assert!(
        !found.is_empty(),
        "no frame log under fixtures/frames/ has a tool.complete with a non-empty inline_diff \
         ({} logs checked); the recording is missing, so this test has nothing to replay",
        paths.len()
    );
    found
}

fn session_id_of(log: &FrameLog) -> String {
    log.events
        .iter()
        .find_map(|event| match event {
            Event::Session { session_id, .. } if !session_id.as_str().is_empty() => {
                Some(session_id.as_str().to_string())
            }
            _ => None,
        })
        .unwrap_or_else(|| "local-session-1".to_string())
}

fn initial_state(log: &FrameLog) -> SessionState {
    match log.initial_snapshot.clone() {
        Some(snapshot) => SessionState::from_snapshot(snapshot),
        None => SessionState::empty(SessionId::new(session_id_of(log))),
    }
}

/// The first piece of a cell's text: up to the first line break, a lone carriage return shown as
/// its visible stand-in.
fn first_piece(text: &str) -> String {
    let piece = match text.split_once('\n') {
        Some((head, _)) => head.strip_suffix('\r').unwrap_or(head),
        None => text,
    };
    piece.replace('\r', "\u{240D}")
}

/// The first row a plain rendering gives a final cell, written out from the rendering table.
fn expected_first_line(cell: &Cell) -> String {
    let line = match cell {
        Cell::User(user) => format!("> {}", first_piece(&user.text)),
        Cell::Assistant(assistant) => first_piece(&assistant.text),
        Cell::Reasoning(reasoning) => first_piece(&reasoning.text),
        Cell::Notice(notice) => format!("\u{b7} {}", first_piece(&notice.text)),
        Cell::Tool(tool) => {
            let mut line = format!("\u{23FA} {}", tool.name);
            if let Some(summary) = &tool.summary {
                line.push_str(": ");
                line.push_str(summary);
            }
            match tool.status {
                ToolStatus::Failed => line.push_str(" (failed)"),
                ToolStatus::Unfinished => line.push_str(" (unfinished)"),
                ToolStatus::Unknown => line.push_str(" (unknown)"),
                ToolStatus::Running | ToolStatus::Succeeded => {}
            }
            line
        }
        Cell::Prompt(_) => {
            panic!("the recorded turn holds a prompt cell, which this test does not expect")
        }
    };
    line.trim_end().to_string()
}

#[test]
fn recorded_turn_commits_each_final_cell_once() {
    for (name, log) in logs_with_inline_diff() {
        let sink = ByteSink::default();
        let mut alac = Alac::new(COLS, ROWS);
        let mut term = InlineTerminal::new(
            sink.clone(),
            (COLS, ROWS),
            LIVE_HEIGHT,
            probe(false, false, Some((0, ROWS - 1))),
            RestoreLink::detached(),
        )
        .expect("inline terminal");
        let mut ledger = CommitLedger::new();
        let mut state = initial_state(&log);

        for (index, event) in log.events.iter().cloned().enumerate() {
            let _ = update(&mut state, event);
            term.commit(ledger.take_ready(state.cells()), &PlainCellLines)
                .unwrap_or_else(|error| panic!("{name}: commit after event {index}: {error}"));
            alac.absorb(&sink);
            let rows = alac.history_and_screen();
            assert!(
                rows.iter().all(|row| !row.contains(ESCAPE)),
                "{name}: an escape byte reached a text row after event {index}: {rows:?}"
            );
        }

        let final_cells = &state.cells()[..state.final_prefix_len()];
        assert!(
            !final_cells.is_empty(),
            "{name}: the replay produced no final cells"
        );
        assert_eq!(
            final_cells.len(),
            state.cells().len(),
            "{name}: the finished turn should leave every cell final"
        );

        let rows = alac.history_and_screen();
        let mut previous: Option<usize> = None;
        for (number, cell) in final_cells.iter().enumerate() {
            let wanted = expected_first_line(cell);
            assert!(
                !wanted.is_empty() && wanted.chars().count() < usize::from(COLS),
                "{name}: cell {number} first line {wanted:?} is empty or wider than a row, \
                 so the check cannot tell it apart"
            );
            let at: Vec<usize> = rows
                .iter()
                .enumerate()
                .filter(|(_, row)| **row == wanted)
                .map(|(index, _)| index)
                .collect();
            assert_eq!(
                at.len(),
                1,
                "{name}: cell {number} first line {wanted:?} should appear on exactly one row, \
                 found rows {at:?} in {rows:?}"
            );
            if let Some(before) = previous {
                assert!(
                    at[0] > before,
                    "{name}: cell {number} first line {wanted:?} at row {} is not after the \
                     previous cell at row {before}",
                    at[0]
                );
            }
            previous = Some(at[0]);
        }

        for cell in final_cells {
            let Cell::Tool(tool) = cell else { continue };
            let Some(diff) = &tool.diff else { continue };
            for diff_line in diff.lines().filter(|line| !line.trim().is_empty()) {
                assert!(
                    rows.iter().all(|row| !row.contains(diff_line.trim())),
                    "{name}: the tool diff line {diff_line:?} must not be shown in history rows"
                );
            }
        }
        assert!(
            rows.iter().all(|row| !row.contains("[38;2")),
            "{name}: a colour sequence leaked into the text rows: {rows:?}"
        );
    }
}

#[test]
fn diff_fixture_reduces_to_final_cells_with_clean_text() {
    for (name, log) in logs_with_inline_diff() {
        let mut state = initial_state(&log);
        for event in log.events.iter().cloned() {
            let _ = update(&mut state, event);
        }
        let diffs: Vec<&String> = state
            .cells()
            .iter()
            .filter_map(|cell| match cell {
                Cell::Tool(tool) => tool.diff.as_ref(),
                _ => None,
            })
            .collect();
        assert!(
            !diffs.is_empty(),
            "{name}: the reduced state has no tool cell with a diff"
        );
        assert!(
            diffs.iter().all(|diff| !diff.contains(ESCAPE)),
            "{name}: a reduced tool diff still holds an escape byte"
        );
        assert_eq!(
            state.final_prefix_len(),
            state.cells().len(),
            "{name}: every cell should be final once the turn completes"
        );
    }
}
