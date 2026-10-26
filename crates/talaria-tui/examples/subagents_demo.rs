//! Replay a subagent frame log without connecting to a gateway.
use hermes_contract::events::EventFrame;
use std::{path::PathBuf, process::ExitCode};
use talaria_core::subagents::{Clock, Spawning, SubagentEvent, SubagentTree};
use talaria_tui::subagents::{SubagentDock, SubagentStyle, batch_block_lines};

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(reason) => {
            eprintln!("{reason}");
            ExitCode::from(1)
        }
    }
}
fn run() -> Result<(), String> {
    let mut args = std::env::args().skip(1);
    let mut width = 100u16;
    let mut path = None;
    while let Some(arg) = args.next() {
        if arg == "--width" {
            width = args
                .next()
                .ok_or("--width needs a number of columns")?
                .parse()
                .map_err(|_| "invalid width")?;
        } else if path.replace(PathBuf::from(arg)).is_some() {
            return Err("one frame log at a time".into());
        }
    }
    let path = path.ok_or("usage: subagents_demo [--width <cols>] <frame-log>")?;
    let input = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
    let mut tree = SubagentTree::new();
    let mut session = None;
    let style = SubagentStyle::plain();
    for line in input.lines() {
        let value: serde_json::Value = serde_json::from_str(line).map_err(|e| e.to_string())?;
        let frame = value.get("frame").unwrap_or(&value);
        let params = frame.get("params").unwrap_or(frame);
        let Ok(decoded) = EventFrame::from_params(params) else {
            continue;
        };
        let id = decoded.session_id.as_deref().unwrap_or_default();
        if id.is_empty() {
            continue;
        }
        if session.is_none() {
            session = Some(id.to_owned());
        }
        if session.as_deref() != Some(id) {
            continue;
        }
        let ms = value
            .get("t_ms")
            .and_then(|v| v.as_u64())
            .unwrap_or_default();
        let now = Clock {
            mono_ms: ms,
            wall_s: ms as f64 / 1000.0,
        };
        if params["type"] == "message.start" {
            tree.prune_finished();
        }
        if let Some(event) = SubagentEvent::from_notification(&decoded.notification) {
            tree.apply(&event, now);
        }
        for batch in tree.take_finished_batches() {
            for row in batch_block_lines(&batch, width, &style) {
                println!("scrollback| {}", text(&row));
            }
        }
        let dock = SubagentDock::new(
            &tree,
            tree.counts(),
            None,
            Spawning::Unknown,
            Some("alt+s"),
            &style,
            3,
            48,
        );
        for row in dock.lines(width) {
            println!("live| {}", text(&row));
        }
    }
    Ok(())
}
fn text(line: &ratatui::text::Line<'static>) -> String {
    line.spans.iter().map(|s| s.content.as_ref()).collect()
}
