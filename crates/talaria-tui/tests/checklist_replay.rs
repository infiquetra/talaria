//! The checklist through a frame log: the panel while the turn runs, the block at the end.
//!
//! The harness draws only the checklist panel and the checklist blocks, never a transcript
//! cell, so the tool cell of the todo tool cannot appear in the rows.

mod support;

use std::path::{Path, PathBuf};

use support::{Alac, ByteSink, probe};
use talaria_core::checklist::ChecklistTracker;
use talaria_core::session::{Event, SessionId, SessionState};
use talaria_tui::checklist::{
    ChecklistSection, ChecklistStyle, PANEL_MAX_ROWS, committed_block_lines,
};
use talaria_tui::keys::{Action, KeyMap};
use talaria_tui::live::LiveView;
use talaria_tui::terminal::{InlineTerminal, RestoreLink};

/// The terminal the harness draws into.
const COLS: u16 = 100;
const ROWS: u16 = 30;
/// The live region's height: the panel's own budget.
const LIVE_ROWS: u16 = PANEL_MAX_ROWS;

/// The terminal under test and the emulator that reads what it writes.
struct Harness {
    term: InlineTerminal<ByteSink>,
    sink: ByteSink,
    alac: Alac,
}

impl Harness {
    fn new() -> Harness {
        let sink = ByteSink::default();
        let term = InlineTerminal::new(
            sink.clone(),
            (COLS, ROWS),
            LIVE_ROWS,
            probe(false, false, Some((0, ROWS - 1))),
            RestoreLink::detached(),
        )
        .expect("new terminal");
        let mut harness = Harness {
            term,
            sink,
            alac: Alac::new(COLS, ROWS),
        };
        harness.alac.absorb(&harness.sink);
        harness
    }

    /// Draw the panel, or clear the live region when there is none.
    fn draw(&mut self, list: Option<&talaria_core::checklist::Checklist>) {
        let style = ChecklistStyle::default();
        let hint = KeyMap::defaults().label(Action::ToggleTasks);
        let section =
            list.map(|list| ChecklistSection::new(list, true, PANEL_MAX_ROWS, hint, &style));
        let sections: Vec<&dyn talaria_tui::live::LiveSection> = match &section {
            Some(section) => vec![section],
            None => Vec::new(),
        };
        self.term
            .draw_live(&LiveView { sections })
            .expect("draw_live");
        self.alac.absorb(&self.sink);
    }

    /// Write the block into the scrollback.
    fn commit(&mut self, commit: &talaria_core::checklist::ChecklistCommit) {
        let style = ChecklistStyle::default();
        let lines = committed_block_lines(commit, COLS, &style);
        self.term.commit_lines(lines).expect("commit_lines");
        self.alac.absorb(&self.sink);
    }

    /// Every row on the screen, trailing blanks trimmed.
    fn screen(&self) -> Vec<String> {
        self.alac.screen()
    }

    /// Every row in the scrollback and on the screen.
    fn history(&self) -> Vec<String> {
        self.alac.history_and_screen()
    }
}

/// The path of a file under this crate's `tests/data` or the repository's `fixtures`.
fn data(relative: &str) -> PathBuf {
    let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    path.push("tests/data");
    path.push(relative);
    path
}

/// The repository's `fixtures/frames` directory.
fn fixtures() -> PathBuf {
    let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    path.pop();
    path.pop();
    path.push("fixtures/frames");
    path
}

/// The session id of the first event of a log that names one.
fn session_id(events: &[Event]) -> SessionId {
    events
        .iter()
        .find_map(|event| match event {
            Event::Session { session_id, .. } if !session_id.as_str().is_empty() => {
                Some(session_id.clone())
            }
            _ => None,
        })
        .expect("a session event")
}

/// The name of the event, for the messages the reducer saw.
fn name(event: &Event) -> String {
    match event {
        Event::Session { frame, .. } => frame.type_name().unwrap_or("").to_string(),
        _ => String::new(),
    }
}

#[test]
fn hand_built_frames_render_panel_then_block() {
    let path = data("todo_frames.jsonl");
    let log = support::frames::read_path(&path).expect("the hand-built frame log");
    assert_eq!(log.undecodable, 0, "every frame decodes");
    let mut state = SessionState::empty(session_id(&log.events));
    let mut tracker = ChecklistTracker::new();
    let mut harness = Harness::new();
    let mut revisions = Vec::new();
    let mut panels = Vec::new();

    for event in &log.events {
        let label = name(event);
        talaria_core::session::update(&mut state, event.clone());
        let observed = tracker.observe(state.todo(), state.turn());
        if let Some(commit) = &observed.commit {
            harness.commit(commit);
        }
        harness.draw(tracker.live());
        if label == "todo.updated" {
            revisions.push(state.todo().map(|todo| todo.revision).unwrap_or_default());
            panels.push(harness.screen());
        }
        if label == "tool.start" || label == "tool.complete" {
            let screen = harness.screen();
            assert!(
                !screen.iter().any(|row| row.contains("Tasks")),
                "the tool frames change nothing: {screen:?}"
            );
        }
    }

    assert_eq!(revisions, vec![1, 2, 3], "the three snapshots");
    assert!(
        panels[0]
            .iter()
            .any(|row| row.contains("▾ Tasks 0/3 · Ctrl+T to collapse")),
        "the first snapshot: {panels:?}"
    );
    assert!(
        panels[1]
            .iter()
            .any(|row| row.contains("  [>] Fix average()")),
        "the second snapshot: {panels:?}"
    );
    let screen = harness.screen();
    assert!(
        !screen.iter().any(|row| row.contains("▾ Tasks")),
        "the panel is gone: {screen:?}"
    );
    let history = harness.history();
    let headers = history
        .iter()
        .filter(|row| row.trim() == "Tasks 3/3 · done")
        .count();
    assert_eq!(headers, 1, "one block header: {history:?}");
}

#[test]
fn recorded_todo_fixture_renders_panel_then_block() {
    let path = fixtures().join("todo-checklist.jsonl");
    let log = support::frames::read_path(&path).expect("the recorded frame log");
    let mut state = SessionState::empty(session_id(&log.events));
    let mut tracker = ChecklistTracker::new();
    let mut harness = Harness::new();
    let mut seen = false;

    for event in &log.events {
        let label = name(event);
        talaria_core::session::update(&mut state, event.clone());
        let observed = tracker.observe(state.todo(), state.turn());
        if let Some(commit) = &observed.commit {
            harness.commit(commit);
        }
        harness.draw(tracker.live());
        if label == "todo.updated" && !seen {
            // The model chooses the wording, so the rows are read from the payload.
            let first_snapshot: Vec<String> = state
                .todo()
                .map(|todo| {
                    todo.todos
                        .iter()
                        .filter_map(|item| item["content"].as_str().map(str::to_string))
                        .collect()
                })
                .unwrap_or_default();
            let screen = harness.screen();
            for content in &first_snapshot {
                assert!(
                    screen.iter().any(|row| row.contains(content.as_str())),
                    "the panel shows {content:?}: {screen:?}"
                );
            }
            seen = true;
        }
    }

    assert!(seen, "the recording holds a todo snapshot");
    let history = harness.history();
    let headers = history
        .iter()
        .filter(|row| row.trim_start().starts_with("Tasks ") && row.contains(" · "))
        .count();
    assert_eq!(headers, 1, "one block header: {history:?}");
    let screen = harness.screen();
    assert!(
        !screen.iter().any(|row| row.contains("▾ Tasks")),
        "no panel is left: {screen:?}"
    );
}

/// The path of a file the harness reads, for the tests that name one.
#[allow(dead_code)]
fn frame_log(path: &Path) -> PathBuf {
    path.to_path_buf()
}
