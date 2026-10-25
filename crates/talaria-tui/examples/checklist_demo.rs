//! Replay a frame log through the reducer, the checklist tracker and the renderer, and print
//! what the checklist draws.
//!
//! Usage: `checklist_demo [--width <cols>] <frame-log>`. The width defaults to 100. Every frame
//! that changed the live panel or wrote a block prints a heading, then the rows as plain text:
//! `live| ` for the panel and `scrollback| ` for the block. Nothing else is printed, and no
//! escape sequence is ever written.

use std::path::PathBuf;
use std::process::ExitCode;

use hermes_client::ReplaySource;
use hermes_contract::events::EventFrame;
use talaria_core::checklist::ChecklistTracker;
use talaria_core::session::{Event, SessionId, SessionState};
use talaria_tui::checklist::{
    ChecklistSection, ChecklistStyle, PANEL_MAX_ROWS, committed_block_lines,
};
use talaria_tui::keys::{Action, KeyMap};

/// The default width of the printed rows.
const DEFAULT_WIDTH: u16 = 100;

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(reason) => {
            eprintln!("{reason}");
            ExitCode::from(2)
        }
    }
}

/// The width and the frame log, from the command line.
fn arguments() -> Result<(u16, PathBuf), String> {
    let mut width = DEFAULT_WIDTH;
    let mut path: Option<PathBuf> = None;
    let mut args = std::env::args().skip(1);
    while let Some(argument) = args.next() {
        match argument.as_str() {
            "--width" => {
                let value = args
                    .next()
                    .ok_or_else(|| "--width needs a number of columns.".to_string())?;
                width = value
                    .parse()
                    .map_err(|_| format!("{value} is not a number of columns."))?;
            }
            "--help" | "-h" => {
                return Err("usage: checklist_demo [--width <cols>] <frame-log>".to_string());
            }
            _ => {
                if path.is_some() {
                    return Err("one frame log at a time.".to_string());
                }
                path = Some(PathBuf::from(argument));
            }
        }
    }
    let path =
        path.ok_or_else(|| "usage: checklist_demo [--width <cols>] <frame-log>".to_string())?;
    Ok((width, path))
}

/// Replay the log and print the checklist.
fn run() -> Result<(), String> {
    let (width, path) = arguments()?;
    let source = ReplaySource::open(&path).map_err(|error| error.to_string())?;
    let style = ChecklistStyle::default();
    let hint = KeyMap::defaults().label(Action::ToggleTasks);
    let mut tracker = ChecklistTracker::new();
    // The state belongs to the session the first event names; the frames before it
    // are the gateway's own.
    let session = source
        .inbound()
        .filter_map(|frame| {
            frame
                .frame
                .get("params")
                .and_then(|params| params["session_id"].as_str())
        })
        .find(|id| !id.is_empty())
        .unwrap_or_default()
        .to_string();
    let mut state = SessionState::empty(SessionId::new(session));
    let mut number = 0;
    for frame in source.inbound() {
        number += 1;
        let params = frame
            .frame
            .get("params")
            .cloned()
            .unwrap_or(serde_json::Value::Null);
        let name = params["type"].as_str().unwrap_or("").to_string();
        let Ok(decoded) = EventFrame::from_params(&params) else {
            continue;
        };
        let session_id = decoded.session_id.clone().unwrap_or_default();
        talaria_core::session::update(
            &mut state,
            Event::Session {
                session_id: SessionId::new(session_id),
                frame: decoded,
                raw: params,
            },
        );
        let observed = tracker.observe(state.todo(), state.turn());
        if !observed.live_changed && observed.commit.is_none() {
            continue;
        }
        println!("== {number} {name} ==");
        if let Some(list) = tracker.live() {
            let section = ChecklistSection::new(
                list,
                tracker.expanded(),
                PANEL_MAX_ROWS,
                hint.clone(),
                &style,
            );
            for row in section.lines(width) {
                println!("live| {}", text(&row));
            }
        }
        if let Some(commit) = observed.commit {
            for row in committed_block_lines(&commit, width, &style) {
                println!("scrollback| {}", text(&row));
            }
        }
    }
    Ok(())
}

/// One row as plain text: its spans' content, joined.
fn text(row: &ratatui::text::Line<'static>) -> String {
    row.spans.iter().map(|span| span.content.as_ref()).collect()
}
