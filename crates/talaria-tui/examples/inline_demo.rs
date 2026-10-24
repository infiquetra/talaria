//! A small program that commits numbered lines above a two row live region.
//!
//! It is the fixture for the tmux scrollback test and the manual terminal checks, and a worked
//! example of the public terminal interfaces.
//!
//! Flags: `--lines N` (default 100), `--interval-ms M` (default 10) and `--hostile`, which
//! commits one line full of control bytes after `line 0005`. Keys: `o` opens an overlay, `Esc`
//! closes it, `+` and `-` change the live height, `q` restores the terminal and exits.

use std::io::{self, Stdout};
use std::process::ExitCode;
use std::sync::mpsc::{self, RecvTimeoutError};
use std::time::{Duration, Instant};

use crossterm::event::{Event, KeyCode, KeyEventKind};
use ratatui::text::Line;
use talaria_tui::live::{LiveSection, LiveView, PlainTextSection};
use talaria_tui::terminal::{
    InlineTerminal, TerminalError, TerminalGuard, probe_startup, spawn_input, terminal_size,
};

const PROBE_DEADLINE: Duration = Duration::from_millis(200);
const MAX_EXTRA_ROWS: u16 = 20;

struct Options {
    lines: u32,
    interval: Duration,
    hostile: bool,
}

fn parse_options() -> Result<Options, String> {
    let mut options = Options {
        lines: 100,
        interval: Duration::from_millis(10),
        hostile: false,
    };
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--lines" => {
                let value = args.next().ok_or("--lines needs a number")?;
                options.lines = value.parse().map_err(|_| "--lines needs a number")?;
            }
            "--interval-ms" => {
                let value = args.next().ok_or("--interval-ms needs a number")?;
                let millis: u64 = value.parse().map_err(|_| "--interval-ms needs a number")?;
                options.interval = Duration::from_millis(millis);
            }
            "--hostile" => options.hostile = true,
            other => return Err(format!("unknown argument {other}")),
        }
    }
    Ok(options)
}

fn describe(error: &TerminalError) -> String {
    match error {
        TerminalError::NotATerminal => {
            "inline_demo: standard input or output is not a terminal".to_string()
        }
        TerminalError::Io(error) => format!("inline_demo: {error}"),
    }
}

fn tagged(number: u32) -> String {
    format!("line {number:04}")
}

fn hostile_line() -> String {
    format!(
        "HOSTILE {}[2J{}[H{}{} END",
        '\u{1b}', '\u{1b}', '\u{7}', '\u{202e}'
    )
}

fn draw_live(
    terminal: &mut InlineTerminal<Stdout>,
    committed: u32,
    total: u32,
    extra_rows: u16,
) -> io::Result<()> {
    let status = PlainTextSection::new(&format!("LIVE# committed {committed} of {total}"));
    let help = PlainTextSection::new("o overlay  +/- height  q quit");
    let padding_text = vec!["LIVE# padding"; usize::from(extra_rows)].join("\n");
    let padding = PlainTextSection::new(&padding_text);
    let mut sections: Vec<&dyn LiveSection> = vec![&status, &help];
    if extra_rows > 0 {
        sections.push(&padding);
    }
    terminal.draw_live(&LiveView { sections })?;
    Ok(())
}

fn draw_overlay(terminal: &mut InlineTerminal<Stdout>) -> io::Result<()> {
    let text = PlainTextSection::new("OVERLAY - press Esc");
    terminal.draw_overlay(|area, buf| text.render(area, buf))?;
    Ok(())
}

fn run(terminal: &mut InlineTerminal<Stdout>, options: &Options) -> io::Result<()> {
    let (sender, receiver) = mpsc::channel::<Event>();
    let input = spawn_input(move |event| sender.send(event).is_ok());
    let total = options.lines;
    let mut committed: u32 = 0;
    let mut extra_rows: u16 = 0;
    let mut overlay = false;
    let mut next_tick = Instant::now();

    draw_live(terminal, committed, total, extra_rows)?;
    let outcome = loop {
        let wait = next_tick.saturating_duration_since(Instant::now());
        match receiver.recv_timeout(wait) {
            Ok(Event::Key(key)) if key.kind != KeyEventKind::Release => match key.code {
                KeyCode::Char('q') => break Ok(()),
                KeyCode::Char('o') if !overlay => {
                    overlay = true;
                    terminal.open_overlay()?;
                    draw_overlay(terminal)?;
                }
                KeyCode::Esc if overlay => {
                    overlay = false;
                    terminal.close_overlay()?;
                    draw_live(terminal, committed, total, extra_rows)?;
                }
                KeyCode::Char('+') if !overlay => {
                    extra_rows = (extra_rows + 1).min(MAX_EXTRA_ROWS);
                    draw_live(terminal, committed, total, extra_rows)?;
                }
                KeyCode::Char('-') if !overlay => {
                    extra_rows = extra_rows.saturating_sub(1);
                    draw_live(terminal, committed, total, extra_rows)?;
                }
                _ => {}
            },
            Ok(Event::Resize(cols, rows)) => {
                terminal.resize(cols, rows)?;
                if overlay {
                    draw_overlay(terminal)?;
                } else {
                    draw_live(terminal, committed, total, extra_rows)?;
                }
            }
            Ok(_) => {}
            Err(RecvTimeoutError::Timeout) => {
                if committed < total {
                    committed += 1;
                    let mut lines = vec![Line::raw(tagged(committed))];
                    if options.hostile && committed == 5 {
                        lines.push(Line::raw(hostile_line()));
                    }
                    terminal.commit_lines(lines)?;
                }
                next_tick = Instant::now() + options.interval;
                if overlay {
                    draw_overlay(terminal)?;
                } else {
                    draw_live(terminal, committed, total, extra_rows)?;
                }
            }
            Err(RecvTimeoutError::Disconnected) => break Ok(()),
        }
    };
    if let Some(input) = input {
        input.stop();
    }
    outcome
}

fn start() -> Result<(TerminalGuard, InlineTerminal<Stdout>), TerminalError> {
    let guard = TerminalGuard::acquire()?;
    let probe = probe_startup(PROBE_DEADLINE)?;
    let size = terminal_size().map_err(TerminalError::Io)?;
    let terminal = InlineTerminal::new(io::stdout(), size, 2, probe, guard.link())
        .map_err(TerminalError::Io)?;
    Ok((guard, terminal))
}

fn main() -> ExitCode {
    let options = match parse_options() {
        Ok(options) => options,
        Err(message) => {
            eprintln!("inline_demo: {message}");
            return ExitCode::from(1);
        }
    };
    let result = {
        // The guard is declared first, so it drops last, after the terminal is restored.
        let (_guard, mut terminal) = match start() {
            Ok(started) => started,
            Err(error) => {
                eprintln!("{}", describe(&error));
                return ExitCode::from(if matches!(error, TerminalError::NotATerminal) {
                    2
                } else {
                    1
                });
            }
        };
        let outcome = run(&mut terminal, &options);
        outcome.and(terminal.restore())
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("inline_demo: {error}");
            ExitCode::from(1)
        }
    }
}
