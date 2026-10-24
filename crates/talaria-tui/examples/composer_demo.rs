//! Composer demo: `--running`, `--config <file>`, `--history <a|b>`.
//!
//! It draws the composer as the live region and commits one line per result: `NOTICE: ...`,
//! `SUBMITTED lines=N chars=M text="..."`, `PASTE-EVENT chars=M`, `INTERRUPT` and
//! `EXIT-REQUESTED`. It is the fixture for the pseudo-terminal tests and a worked example of
//! wiring the composer to the terminal module.

use std::collections::BTreeMap;
use std::io::{self, Stdout};
use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::mpsc::{self, RecvTimeoutError};
use std::time::{Duration, Instant};

use crossterm::event::Event;
use ratatui::text::Line;
use talaria_tui::composer::{Composer, ComposerContext, ComposerOutcome};
use talaria_tui::keys::{Key, KeyMap};
use talaria_tui::live::{LiveSection, LiveView};
use talaria_tui::terminal::{
    InlineTerminal, TerminalError, TerminalGuard, probe_startup, spawn_input, terminal_size,
};

const PROBE_DEADLINE: Duration = Duration::from_millis(200);
const MAX_ROWS: u16 = 10;

struct Options {
    running: bool,
    config: Option<PathBuf>,
    history: Vec<String>,
}

fn parse_options() -> Result<Options, String> {
    let mut options = Options {
        running: false,
        config: None,
        history: Vec::new(),
    };
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--running" => options.running = true,
            "--config" => {
                let path = args.next().ok_or("--config needs a file")?;
                options.config = Some(PathBuf::from(path));
            }
            "--history" => {
                let entries = args.next().ok_or("--history needs entries")?;
                options.history = entries.split('|').map(str::to_string).collect();
            }
            other => return Err(format!("unknown argument {other}")),
        }
    }
    Ok(options)
}

/// Loader notices first, then the key map's.
fn keymap_and_notices(options: &Options) -> (KeyMap, Vec<String>) {
    let Some(path) = &options.config else {
        return (KeyMap::defaults(), Vec::new());
    };
    let loaded = talaria_app::config::load_file(path);
    let entries: BTreeMap<String, Vec<String>> = loaded.config.keys;
    let (map, key_notices) = KeyMap::from_entries(&entries);
    let mut notices = loaded.notices;
    notices.extend(key_notices);
    (map, notices)
}

fn commit(terminal: &mut InlineTerminal<Stdout>, text: String) -> io::Result<()> {
    terminal.commit_lines(vec![Line::from(text)])
}

fn draw(terminal: &mut InlineTerminal<Stdout>, composer: &mut Composer) -> io::Result<()> {
    let width = terminal.viewport().width;
    let section = composer.section(width, MAX_ROWS);
    terminal.draw_live(&LiveView {
        sections: vec![&section as &dyn LiveSection],
    })?;
    Ok(())
}

/// What the loop does after one result.
enum Next {
    Continue,
    Exit,
}

fn report(
    terminal: &mut InlineTerminal<Stdout>,
    composer: &mut Composer,
    outcome: ComposerOutcome,
) -> io::Result<Next> {
    match outcome {
        ComposerOutcome::Unchanged => return Ok(Next::Continue),
        ComposerOutcome::Changed => {}
        ComposerOutcome::Submit(text) => {
            let lines = text.matches('\n').count() + 1;
            let chars = text.chars().count();
            commit(
                terminal,
                format!("SUBMITTED lines={lines} chars={chars} text={text:?}"),
            )?;
        }
        ComposerOutcome::Interrupt => commit(terminal, "INTERRUPT".to_string())?,
        ComposerOutcome::ExitRequested => {
            commit(terminal, "EXIT-REQUESTED".to_string())?;
            return Ok(Next::Exit);
        }
    }
    draw(terminal, composer)?;
    Ok(Next::Continue)
}

fn run(terminal: &mut InlineTerminal<Stdout>, options: &Options, keyboard: bool) -> io::Result<()> {
    let (keymap, notices) = keymap_and_notices(options);
    let mut composer = Composer::with_history(keymap, options.history.clone());
    composer.set_keyboard_enhancement(keyboard);
    for notice in notices {
        commit(terminal, format!("NOTICE: {notice}"))?;
    }
    let ctx = ComposerContext {
        turn_running: options.running,
    };

    let (sender, receiver) = mpsc::channel::<Event>();
    let input = spawn_input(move |event| sender.send(event).is_ok());
    draw(terminal, &mut composer)?;
    let outcome = loop {
        let event = match composer.next_tick() {
            Some(due) => {
                match receiver.recv_timeout(due.saturating_duration_since(Instant::now())) {
                    Ok(event) => Some(event),
                    Err(RecvTimeoutError::Timeout) => None,
                    Err(RecvTimeoutError::Disconnected) => break Ok(()),
                }
            }
            None => match receiver.recv() {
                Ok(event) => Some(event),
                Err(_) => break Ok(()),
            },
        };
        let result = match event {
            None => composer.tick(Instant::now()),
            Some(Event::Key(key)) => composer.handle_key(Key::from(key), ctx, Instant::now()),
            Some(Event::Paste(text)) => {
                commit(
                    terminal,
                    format!("PASTE-EVENT chars={}", text.chars().count()),
                )?;
                composer.handle_paste(&text)
            }
            Some(Event::Resize(cols, rows)) => {
                terminal.resize(cols, rows)?;
                ComposerOutcome::Changed
            }
            Some(_) => ComposerOutcome::Unchanged,
        };
        match report(terminal, &mut composer, result) {
            Ok(Next::Continue) => {}
            Ok(Next::Exit) => break Ok(()),
            Err(error) => break Err(error),
        }
    };
    if let Some(input) = input {
        input.stop();
    }
    outcome
}

/// Acquire the terminal, run, and restore. The guard drops when this returns, after the
/// terminal is restored, so messages are printed by the caller with the terminal back to normal.
fn session(options: &Options) -> Result<(), TerminalError> {
    let guard = TerminalGuard::acquire()?;
    let probe = probe_startup(PROBE_DEADLINE)?;
    let size = terminal_size().map_err(TerminalError::Io)?;
    let mut terminal = InlineTerminal::new(io::stdout(), size, 3, probe, guard.link())
        .map_err(TerminalError::Io)?;
    run(&mut terminal, options, probe.caps.keyboard_enhancement)
        .and(terminal.restore())
        .map_err(TerminalError::Io)
}

fn main() -> ExitCode {
    let options = match parse_options() {
        Ok(options) => options,
        Err(message) => {
            eprintln!("composer_demo: {message}");
            return ExitCode::from(1);
        }
    };
    match session(&options) {
        Ok(()) => ExitCode::SUCCESS,
        Err(TerminalError::NotATerminal) => {
            println!("composer_demo: standard input or output is not a terminal");
            ExitCode::from(2)
        }
        Err(TerminalError::Io(error)) => {
            eprintln!("composer_demo: {error}");
            ExitCode::from(1)
        }
    }
}
