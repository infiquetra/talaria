//! The only module that writes escape sequences: start-up probe, inline viewport,
//! scrollback commits, overlays and terminal restore.

use std::io;
use std::marker::PhantomData;
use std::os::fd::AsFd;
use std::time::Duration;

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::text::Line;
use talaria_core::session::Cell;

use crate::history::CellLines;
use crate::live::{FrameStats, LiveView};

/// What the terminal said it can do.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TerminalCaps {
    pub synchronized_output: bool,
    pub keyboard_enhancement: bool,
}

/// The start-up probe's answers. `cursor` is (column, row), 0-based.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ProbeReply {
    pub caps: TerminalCaps,
    pub cursor: Option<(u16, u16)>,
}

/// The four queries, written once at start-up.
pub const PROBE_QUERY: &[u8] = b"";

/// Parse a probe reply. The flag is true when the end marker was seen.
pub fn parse_probe_reply(bytes: &[u8]) -> (ProbeReply, bool) {
    let _ = bytes;
    todo!("terminal")
}

/// Read a probe reply from `fd` until the marker or `deadline`. Never writes.
pub fn read_probe_reply(fd: impl AsFd, deadline: Duration) -> io::Result<ProbeReply> {
    let _ = (fd, deadline);
    todo!("terminal")
}

/// Write the probe query and read the reply. Raw mode must already be on.
pub fn probe_startup(deadline: Duration) -> Result<ProbeReply, TerminalError> {
    let _ = deadline;
    todo!("terminal")
}

/// The terminal size as (columns, rows).
pub fn terminal_size() -> io::Result<(u16, u16)> {
    todo!("terminal")
}

#[derive(Debug)]
pub enum TerminalError {
    NotATerminal,
    Io(io::Error),
}

/// Raw mode and the panic hook, restored on drop.
pub struct TerminalGuard {
    _private: (),
}

impl TerminalGuard {
    pub fn acquire() -> Result<Self, TerminalError> {
        todo!("terminal")
    }

    pub fn link(&self) -> RestoreLink {
        todo!("terminal")
    }
}

/// The shared state the restore routine runs from.
#[derive(Clone)]
pub struct RestoreLink {
    _private: (),
}

impl RestoreLink {
    pub fn detached() -> Self {
        todo!("terminal")
    }

    /// Write the restore sequence once. `Ok(false)` means nothing was written.
    pub fn restore_into(&self, w: &mut dyn io::Write) -> io::Result<bool> {
        let _ = w;
        todo!("terminal")
    }
}

/// A fixed viewport at the bottom of the terminal with native scrollback above it.
pub struct InlineTerminal<W: io::Write> {
    _writer: PhantomData<W>,
}

impl<W: io::Write> InlineTerminal<W> {
    pub fn new(
        w: W,
        size: (u16, u16),
        live_height: u16,
        probe: ProbeReply,
        link: RestoreLink,
    ) -> io::Result<Self> {
        let _ = (w, size, live_height, probe, link);
        todo!("terminal")
    }

    pub fn commit(&mut self, cells: &[Cell], render: &dyn CellLines) -> io::Result<()> {
        let _ = (cells, render);
        todo!("terminal")
    }

    pub fn commit_lines(&mut self, lines: Vec<Line<'static>>) -> io::Result<()> {
        let _ = lines;
        todo!("terminal")
    }

    pub fn draw_live(&mut self, view: &LiveView<'_>) -> io::Result<FrameStats> {
        let _ = view;
        todo!("terminal")
    }

    /// No repaint: the caller's next `draw_live` paints.
    pub fn resize(&mut self, cols: u16, rows: u16) -> io::Result<()> {
        let _ = (cols, rows);
        todo!("terminal")
    }

    pub fn open_overlay(&mut self) -> io::Result<()> {
        todo!("terminal")
    }

    pub fn draw_overlay(
        &mut self,
        render: impl FnOnce(Rect, &mut Buffer),
    ) -> io::Result<FrameStats> {
        let _ = render;
        todo!("terminal")
    }

    /// No repaint: the caller's next `draw_live` paints.
    pub fn close_overlay(&mut self) -> io::Result<()> {
        todo!("terminal")
    }

    pub fn restore(&mut self) -> io::Result<()> {
        todo!("terminal")
    }

    /// Height 0 while the screen is too small.
    pub fn viewport(&self) -> Rect {
        todo!("terminal")
    }
}

/// The reader thread; dropping it stops and joins it.
pub struct InputThread {
    _private: (),
}

impl InputThread {
    pub fn stop(self) {
        todo!("terminal")
    }

    pub fn is_finished(&self) -> bool {
        todo!("terminal")
    }
}

/// Start the key reader. `None` when standard input is not a terminal. The closure
/// returns false to stop the thread.
pub fn spawn_input<F>(forward: F) -> Option<InputThread>
where
    F: FnMut(crossterm::event::Event) -> bool + Send + 'static,
{
    let _ = forward;
    todo!("terminal")
}
