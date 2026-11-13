//! Exercise supported seat records without opening a gateway session or a viewport.

use std::io::{self, IsTerminal, Write};
use std::thread;
use std::time::Duration;

use talaria_core::{BlockKind, SeatState};
use talaria_tui::terminal::{TerminalGuard, clear_status_record, probe_startup, status_record};

fn announce(record: &[u8]) {
    let text = String::from_utf8_lossy(record);
    let payload = text.split_once("state=").map_or("", |(_, payload)| payload);
    let mut fields = payload.split(':');
    let word = fields.next().unwrap_or_default();
    let kind = fields
        .find_map(|field| field.strip_prefix("kind="))
        .map(|kind| kind.trim_end_matches(char::is_control));
    if let Some(kind) = kind {
        eprintln!("seat-probe {word} {kind}");
    } else {
        eprintln!("seat-probe {word}");
    }
}

fn run() -> Result<(), Box<dyn std::error::Error>> {
    let stdout = io::stdout();
    if !stdout.is_terminal() {
        return Ok(());
    }
    let _guard = TerminalGuard::acquire()?;
    let probe = probe_startup(Duration::from_millis(200))?;
    let Some(support) = probe.status else {
        return Ok(());
    };
    let mut out = stdout.lock();
    let mut written = false;
    for state in [
        SeatState::Idle,
        SeatState::Working,
        SeatState::Blocked(Some(BlockKind::Permission)),
        SeatState::Blocked(Some(BlockKind::Question)),
        SeatState::Blocked(Some(BlockKind::Auth)),
        SeatState::Done,
        SeatState::Error,
    ] {
        if let Some(record) = status_record(state, support) {
            out.write_all(&record)?;
            out.flush()?;
            written = true;
            announce(&record);
            thread::sleep(Duration::from_secs(2));
        }
    }
    if written {
        out.write_all(clear_status_record())?;
        out.flush()?;
        eprintln!("seat-probe clear");
    }
    Ok(())
}

fn main() -> std::process::ExitCode {
    match run() {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(_) => std::process::ExitCode::FAILURE,
    }
}
