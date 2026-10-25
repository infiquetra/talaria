//! The live recording drives the checklist: every snapshot parses, the revisions never fall,
//! and the turn ends with one block.
//!
//! The test needs a directory of recorded sessions, named by `TALARIA_LIVE_FRAMES`. Without one
//! it skips and says so; with `TALARIA_LIVE_REQUIRE=1` a skip is a failure. Run it with
//! `--ignored --nocapture` so the result line shows.

mod support;

use std::path::{Path, PathBuf};

use talaria_core::checklist::{Checklist, ChecklistTracker, CommitKind};
use talaria_core::session::{Event, SessionId, SessionState};

/// The environment variable that names the directory of recordings.
const FRAMES: &str = "TALARIA_LIVE_FRAMES";

/// Whether a skip is a failure.
fn required() -> bool {
    std::env::var("TALARIA_LIVE_REQUIRE").is_ok_and(|value| value == "1")
}

/// The recorded sessions, in name order.
fn recordings() -> Vec<PathBuf> {
    let Ok(directory) = std::env::var(FRAMES) else {
        return Vec::new();
    };
    if directory.is_empty() {
        return Vec::new();
    }
    let Ok(entries) = std::fs::read_dir(&directory) else {
        return Vec::new();
    };
    let mut files: Vec<PathBuf> = entries
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| {
            path.extension()
                .is_some_and(|extension| extension == "jsonl")
        })
        .collect();
    files.sort();
    files
}

/// The session id of the first event that names one.
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

/// One recording: the snapshots it holds, and the block the turn ends with.
fn check(path: &Path) {
    let log = support::frames::read_path(path).expect("the recorded frame log");
    let mut state = SessionState::empty(session_id(&log.events));
    let mut tracker = ChecklistTracker::new();
    let mut snapshots = 0;
    let mut todo_tool = false;
    let mut last_revision: Option<i64> = None;
    let mut commits = 0;

    for event in &log.events {
        let Event::Session { raw, .. } = event else {
            continue;
        };
        let name = raw["type"].as_str().unwrap_or("");
        if name == "tool.complete" && raw["payload"]["name"].as_str() == Some("todo_list") {
            todo_tool = true;
        }
        talaria_core::session::update(&mut state, event.clone());
        if name == "todo.updated" {
            let payload = &raw["payload"];
            let todos = payload["todos"]
                .as_array()
                .expect("the snapshot holds a list of todos");
            let revision = payload["revision"]
                .as_i64()
                .expect("the snapshot holds a revision");
            if let Some(last) = last_revision {
                assert!(
                    revision >= last,
                    "the revision fell: {last} then {revision}"
                );
            }
            last_revision = Some(revision);
            let list = Checklist::from_values(u64::try_from(revision).unwrap_or_default(), todos);
            assert_eq!(list.skipped, 0, "every item parses: {todos:?}");
            snapshots += 1;
        }
        let observed = tracker.observe(state.todo(), state.turn());
        if let Some(commit) = observed.commit {
            assert_eq!(commit.kind, CommitKind::TurnEnded, "the turn ended");
            commits += 1;
        }
    }

    assert!(
        snapshots > 0,
        "the recording holds no todo snapshot: the model did not use the todo tool, or the \
         gateway published no snapshot for this session (a local viewer turn)"
    );
    assert!(todo_tool, "the recording holds no todo tool call");
    assert_eq!(commits, 1, "the turn ends with one block");
}

#[test]
#[ignore = "needs a live recording; run it with --ignored --nocapture"]
fn live_recording_drives_the_checklist() {
    let files = recordings();
    if files.is_empty() {
        let reason = format!("{FRAMES} names no recording");
        if required() {
            panic!("LIVE_CHECKLIST_RESULT: FAILED ({reason})");
        }
        println!("LIVE_CHECKLIST_RESULT: SKIPPED ({reason})");
        return;
    }
    for file in &files {
        check(file);
    }
    println!("LIVE_CHECKLIST_RESULT: PASSED");
}
