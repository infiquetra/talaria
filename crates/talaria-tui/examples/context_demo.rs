//! Replay a frame log through the context tracker and the renderers, and print what the
//! view would show.
//!
//! Usage: `context_demo [--width <cols>] <frame-log>`. The width defaults to 80. The file is
//! read line by line with `serde_json`, as the frame-log format defines it: the header and the
//! `dropped` records are skipped, and every inbound event frame is fed to the tracker. The
//! compacting line is printed as `live| ` when it appears and as an empty row when it goes;
//! every committed row is printed as `scrollback| `.

use std::path::PathBuf;
use std::process::ExitCode;

use serde_json::Value;
use talaria_core::context::tracker::{ContextTracker, HeldBlocks};
use talaria_tui::context_view::{CompactingSection, ContextStyle, notice_block};

/// The default width of the printed rows.
const DEFAULT_WIDTH: u16 = 80;

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
                return Err("usage: context_demo [--width <cols>] <frame-log>".to_string());
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
        path.ok_or_else(|| "usage: context_demo [--width <cols>] <frame-log>".to_string())?;
    Ok((width, path))
}

/// Replay the log and print the rows.
fn run() -> Result<(), String> {
    let (width, path) = arguments()?;
    let text = std::fs::read_to_string(&path).map_err(|_| "The frame log could not be read.")?;
    let style = ContextStyle::plain();
    let mut tracker = ContextTracker::new();
    let mut held: HeldBlocks<String> = HeldBlocks::new();
    let mut session_id = String::new();
    let mut shown: Option<String> = None;
    let mut turn_running = false;

    for (number, line) in text.lines().enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        let record: Value =
            serde_json::from_str(line).map_err(|_| format!("line {} is not JSON.", number + 1))?;
        if record["kind"].as_str() != Some("frame") || record["dir"].as_str() != Some("in") {
            continue;
        }
        let frame = &record["frame"];
        if frame["method"].as_str() != Some("event") {
            continue;
        }
        let params = &frame["params"];
        let type_name = params["type"].as_str().unwrap_or("").to_string();
        let payload = params["payload"].clone();
        if let Some(id) = params["session_id"].as_str()
            && !id.is_empty()
        {
            session_id = id.to_owned();
        }
        let now_ms = record["tMs"].as_u64().unwrap_or(0);
        match type_name.as_str() {
            "message.start" | "message.delta" => turn_running = true,
            "message.complete" => turn_running = false,
            _ => {}
        }
        let notices = tracker.observe(&session_id, &type_name, &payload, now_ms);
        for notice in notices {
            for row in notice_block(&notice, width, &style) {
                let text = row
                    .spans
                    .iter()
                    .map(|span| span.content.as_ref())
                    .collect::<String>();
                let blocks = held.push(text, turn_running);
                for block in blocks {
                    println!("scrollback| {block}");
                }
            }
        }
        let line = CompactingSection {
            line: tracker.compacting_line(&session_id),
            now_ms,
        }
        .row();
        if line != shown {
            if let Some(text) = &line {
                println!("live| {text}");
            } else if shown.is_some() {
                println!("live| ");
            }
            shown = line;
        }
    }
    for block in held.release() {
        println!("scrollback| {block}");
    }
    Ok(())
}
