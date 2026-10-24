//! Print the cells, prompts, turn, usage and effects for one version 3 frame log.
//!
//! The reader and cell descriptions are shared with the replay tests.

#[path = "../tests/support/describe.rs"]
mod describe;
#[path = "../tests/support/frames.rs"]
mod frames;

use std::{path::Path, process::ExitCode};
use talaria_core::session::{Event, SessionId, SessionState, Usage, update};

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(reason) => {
            println!("REDUCE FAILED: {reason}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<(), String> {
    let mut args = std::env::args_os().skip(1);
    let Some(path) = args.next() else {
        return Err("Provide one version 3 frame log.".to_owned());
    };
    if args.next().is_some() {
        return Err("Provide just one frame log.".to_owned());
    }
    let log = frames::read_path(Path::new(&path))?;
    let had_snapshot = log.initial_snapshot.is_some();
    let mut state = if let Some(snapshot) = log.initial_snapshot {
        SessionState::from_snapshot(snapshot)
    } else {
        let session = log
            .events
            .iter()
            .find_map(|event| match event {
                Event::Session { session_id, .. } if !session_id.as_str().is_empty() => {
                    Some(session_id.clone())
                }
                _ => None,
            })
            .unwrap_or_else(|| SessionId::new(""));
        SessionState::empty(session)
    };
    let mut effects = Vec::new();
    for event in log.events {
        effects.extend(update(&mut state, event));
    }
    print!("{}", describe::describe(&state));
    println!("activity: {}", state.activity().unwrap_or("none"));
    match state.status() {
        Some(note) => println!("status: {} {}", note.kind, note.text),
        None => println!("status: none"),
    }
    println!("usage: {}", usage_line(state.usage()));
    let cursor = state.cursor();
    println!(
        "cursor: epoch={} last_seq={}",
        cursor.replay_epoch.as_deref().unwrap_or("none"),
        cursor.last_seq
    );
    if had_snapshot {
        println!(
            "queued admissions ignored: {}",
            state.queued_admissions_ignored()
        );
    }
    println!("undecodable frames skipped: {}", log.undecodable);
    println!("effects: {}", effects.len());
    for effect in effects {
        println!("{}", describe::effect_line(&effect));
    }
    println!("REDUCE OK");
    Ok(())
}

fn usage_line(usage: &Usage) -> String {
    let mut fields = Vec::new();
    macro_rules! field {
        ($name:ident) => {
            if let Some(value) = &usage.$name {
                fields.push(format!("{}={value}", stringify!($name)));
            }
        };
    }
    field!(model);
    field!(input);
    field!(output);
    field!(reasoning);
    field!(prompt);
    field!(completion);
    field!(total);
    field!(calls);
    field!(context_used);
    field!(context_max);
    field!(context_percent);
    field!(cache_hit_pct);
    field!(cache_read);
    field!(cache_write);
    field!(compressions);
    field!(cost_usd);
    field!(cost_status);
    if fields.is_empty() {
        "none".to_owned()
    } else {
        fields.join(" ")
    }
}
