//! Offline terminal fixture: `--workspace --lines N --interval-ms M`.
//! `w` enters the workspace, `i` returns inline, `o` opens a picker, Esc closes it,
//! and `q` restores the terminal and exits.

use std::io::{self, Stdout};
use std::process::ExitCode;
use std::sync::mpsc::{self, RecvTimeoutError};
use std::time::{Duration, Instant};

use crossterm::event::{Event, KeyCode, KeyEventKind};
use ratatui::text::Line;
use talaria_core::layout::{LayoutPreferences, LayoutState};
use talaria_tui::history::PlainCellLines;
use talaria_tui::live::{LiveSection, LiveView, PlainTextSection};
use talaria_tui::pager::{Details, TranscriptEntry};
use talaria_tui::terminal::{
    InlineTerminal, TerminalGuard, probe_startup, spawn_input, terminal_size,
};
use talaria_tui::workspace::{self, WorkspaceView};

struct Options {
    lines: u32,
    interval: Duration,
    workspace: bool,
}

fn options() -> Result<Options, String> {
    let mut result = Options {
        lines: 100,
        interval: Duration::from_millis(20),
        workspace: false,
    };
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--workspace" => result.workspace = true,
            "--lines" => {
                result.lines = args
                    .next()
                    .ok_or("--lines needs a number")?
                    .parse()
                    .map_err(|_| "--lines needs a number")?;
            }
            "--interval-ms" => {
                let milliseconds: u64 = args
                    .next()
                    .ok_or("--interval-ms needs a number")?
                    .parse()
                    .map_err(|_| "--interval-ms needs a number")?;
                result.interval = Duration::from_millis(milliseconds);
            }
            _ => return Err(format!("unknown argument {arg}")),
        }
    }
    Ok(result)
}

fn draw(
    terminal: &mut InlineTerminal<Stdout>,
    layout: &LayoutState,
    lines: &[Line<'static>],
    overlay: bool,
) -> io::Result<()> {
    if overlay {
        let picker = PlainTextSection::new("Temporary picker · Esc returns to workspace");
        terminal.draw_overlay(|area, buffer| picker.render(area, buffer))?;
        return Ok(());
    }
    let help = PlainTextSection::new("w workspace · i inline · o picker · q quit");
    let footer = LiveView {
        sections: vec![&help],
    };
    if layout.workspace_enabled() {
        let entries = [TranscriptEntry::Lines(lines)];
        let view = WorkspaceView {
            entries: &entries,
            details: Details::Expanded,
            panel: layout.panel,
            checklist: None,
            checklist_expanded: false,
            footer: &footer,
        };
        terminal.draw_workspace(|area, buffer| {
            workspace::draw(&view, area, buffer, &PlainCellLines);
        })?;
    } else {
        terminal.draw_live(&footer)?;
    }
    Ok(())
}

fn run(terminal: &mut InlineTerminal<Stdout>, options: &Options) -> io::Result<()> {
    let (sender, receiver) = mpsc::channel();
    let input = spawn_input(move |event| sender.send(event).is_ok());
    let area = terminal.screen_area();
    let mut layout = LayoutState::new(
        LayoutPreferences {
            work_panel: options.workspace,
            ..LayoutPreferences::default()
        },
        area.width,
        area.height,
    );
    if options.workspace {
        terminal.enter_workspace()?;
    }
    let mut lines = Vec::new();
    let mut committed = 0;
    let mut overlay = false;
    let mut next_line = Instant::now();
    draw(terminal, &layout, &lines, overlay)?;
    let outcome = loop {
        let wait = if committed < options.lines {
            next_line.saturating_duration_since(Instant::now())
        } else {
            Duration::from_millis(250)
        };
        let mut changed = false;
        match receiver.recv_timeout(wait) {
            Ok(Event::Key(key)) if key.kind != KeyEventKind::Release => match key.code {
                KeyCode::Char('q') => break Ok(()),
                KeyCode::Esc if overlay => {
                    terminal.close_overlay()?;
                    overlay = false;
                    changed = true;
                }
                KeyCode::Char('o') if !overlay => {
                    terminal.open_overlay()?;
                    overlay = true;
                    changed = true;
                }
                KeyCode::Char('i') if !overlay => {
                    layout = LayoutState::default();
                    terminal.leave_workspace()?;
                    changed = true;
                }
                KeyCode::Char('w') if !overlay => {
                    let area = terminal.screen_area();
                    layout = LayoutState::new(
                        LayoutPreferences {
                            work_panel: true,
                            ..LayoutPreferences::default()
                        },
                        area.width,
                        area.height,
                    );
                    terminal.enter_workspace()?;
                    changed = true;
                }
                _ => {}
            },
            Ok(Event::Resize(columns, rows)) => {
                terminal.resize(columns, rows)?;
                layout.resize(columns, rows);
                changed = true;
            }
            Ok(_) | Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => break Ok(()),
        }
        if committed < options.lines && Instant::now() >= next_line {
            committed += 1;
            let line = Line::from(format!("line {committed:04}"));
            terminal.commit_lines(vec![line.clone()])?;
            lines.push(line);
            next_line = Instant::now() + options.interval;
            changed = true;
        }
        if changed {
            draw(terminal, &layout, &lines, overlay)?;
        }
    };
    drop(input);
    outcome
}

fn main() -> ExitCode {
    let result = (|| -> Result<(), Box<dyn std::error::Error>> {
        let options = options().map_err(io::Error::other)?;
        let guard = TerminalGuard::acquire()?;
        let probe = probe_startup(Duration::from_millis(200))?;
        let mut terminal =
            InlineTerminal::new(io::stdout(), terminal_size()?, 1, probe, guard.link())?;
        let outcome = run(&mut terminal, &options);
        terminal.restore()?;
        outcome?;
        Ok(())
    })();
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("workspace_demo: {error}");
            ExitCode::FAILURE
        }
    }
}
