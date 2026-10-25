//! Print tool cells from a frame log. Issue 221.
//!
//! Colour escapes are written only when standard output is a terminal and `NO_COLOR` is unset.

#[path = "../tests/support/replay_tools.rs"]
#[allow(dead_code)]
mod replay_tools;

use std::io::{self, IsTerminal, Write};
use std::path::PathBuf;
use std::process::ExitCode;

use ratatui::style::Color;
use ratatui::text::Line;
use replay_tools::{replay_log, tool_cells};
use talaria_tui::terminal::terminal_size;
use talaria_tui::tool_cell::{Detail, ToolRenderOptions, ToolTheme, tool_lines};

fn main() -> ExitCode {
    let (path, width, full) = match parse_args() {
        Ok(parsed) => parsed,
        Err(message) => {
            eprintln!("{message}");
            return ExitCode::from(2);
        }
    };
    let color = io::stdout().is_terminal() && std::env::var_os("NO_COLOR").is_none();
    let state = replay_log(&path);
    let options = ToolRenderOptions {
        width,
        theme: ToolTheme::default(),
        color,
        detail: if full { Detail::Full } else { Detail::Compact },
        expand_hint: None,
        elapsed: None,
    };
    let mut out = io::stdout().lock();
    for tool in tool_cells(&state) {
        for line in tool_lines(tool, &options) {
            if write_line(&mut out, &line, color).is_err() {
                return ExitCode::from(1);
            }
        }
    }
    ExitCode::SUCCESS
}

fn parse_args() -> Result<(PathBuf, u16, bool), String> {
    let mut width = None;
    let mut full = false;
    let mut path = None;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--full" => full = true,
            "--width" => {
                let value = args.next().ok_or_else(usage)?;
                let parsed = value.parse::<u16>().map_err(|_| usage())?;
                if parsed == 0 {
                    return Err(usage());
                }
                width = Some(parsed);
            }
            other if other.starts_with('-') => return Err(usage()),
            other => {
                if path.is_some() {
                    return Err(usage());
                }
                path = Some(PathBuf::from(other));
            }
        }
    }
    let path = path.ok_or_else(usage)?;
    let width = width.unwrap_or_else(|| {
        terminal_size()
            .map(|(columns, _)| columns.max(1))
            .unwrap_or(100)
    });
    Ok((path, width, full))
}

fn usage() -> String {
    "usage: tool_cells_demo [--width N] [--full] <frame-log>".to_owned()
}

fn write_line(out: &mut impl Write, line: &Line<'_>, color: bool) -> io::Result<()> {
    if !color {
        for span in &line.spans {
            write!(out, "{}", span.content)?;
        }
        writeln!(out)?;
        return Ok(());
    }
    for span in &line.spans {
        if let Some(code) = fg_code(span.style.fg) {
            write!(out, "\u{1b}[{code}m")?;
        }
        write!(out, "{}", span.content)?;
        if span.style.fg.is_some() {
            write!(out, "\u{1b}[0m")?;
        }
    }
    writeln!(out)
}

fn fg_code(color: Option<Color>) -> Option<u8> {
    match color {
        Some(Color::Black) => Some(30),
        Some(Color::Red) => Some(31),
        Some(Color::Green) => Some(32),
        Some(Color::Yellow) => Some(33),
        Some(Color::Blue) => Some(34),
        Some(Color::Magenta) => Some(35),
        Some(Color::Cyan) => Some(36),
        Some(Color::Gray) => Some(37),
        Some(Color::White) => Some(97),
        Some(Color::DarkGray) => Some(90),
        Some(Color::LightRed) => Some(91),
        Some(Color::LightGreen) => Some(92),
        Some(Color::LightYellow) => Some(93),
        Some(Color::LightBlue) => Some(94),
        Some(Color::LightMagenta) => Some(95),
        Some(Color::LightCyan) => Some(96),
        Some(_) => None,
        None => None,
    }
}
