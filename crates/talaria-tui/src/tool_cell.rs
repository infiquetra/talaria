//! Tool-call rows and the drafting line. Issue 221.

use std::collections::HashMap;
use std::time::{Duration, Instant};

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::Line;
use talaria_core::session::{Cell, CellId, ToolCell};

use crate::history::CellLines;
use crate::live::LiveSection;

/// Compact scrollback, or the full form the transcript can show.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Detail {
    /// The short form written into scrollback.
    Compact,
    /// Arguments, output and the whole diff.
    Full,
}

/// Named colours for a tool cell.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ToolTheme {
    /// The mark while a call runs.
    pub glyph_running: Style,
    /// The mark when a call succeeded.
    pub glyph_ok: Style,
    /// The mark when a call failed.
    pub glyph_failed: Style,
    /// The mark when a call did not finish, and an unknown row.
    pub glyph_muted: Style,
    /// The tool title.
    pub title: Style,
    /// What the call acted on.
    pub subject: Style,
    /// Duration, progress, counts and hints.
    pub meta: Style,
    /// Ordinary output.
    pub output: Style,
    /// Failure text.
    pub error: Style,
    /// An added diff row.
    pub diff_added: Style,
    /// A removed diff row.
    pub diff_removed: Style,
    /// An unchanged diff row.
    pub diff_context: Style,
    /// A hunk header after the first.
    pub diff_hunk: Style,
    /// A file row when a diff has several files.
    pub diff_file: Style,
    /// A diff note.
    pub diff_note: Style,
    /// The line number column.
    pub line_number: Style,
}

impl Default for ToolTheme {
    fn default() -> Self {
        let green = Style::default().fg(Color::Green);
        let red = Style::default().fg(Color::Red);
        let dim = Style::default().add_modifier(Modifier::DIM);
        let bold = Style::default().add_modifier(Modifier::BOLD);
        Self {
            glyph_running: dim,
            glyph_ok: green,
            glyph_failed: red,
            glyph_muted: dim,
            title: bold,
            subject: Style::default(),
            meta: dim,
            output: Style::default(),
            error: red,
            diff_added: green,
            diff_removed: red,
            diff_context: Style::default(),
            diff_hunk: Style::default().fg(Color::Cyan),
            diff_file: bold,
            diff_note: dim,
            line_number: dim,
        }
    }
}

/// What one draw of a tool cell is allowed to use.
pub struct ToolRenderOptions {
    /// Columns for every row.
    pub width: u16,
    /// Colours and weight.
    pub theme: ToolTheme,
    /// When false, colour is removed and bold and dim stay.
    pub color: bool,
    /// Compact or full.
    pub detail: Detail,
    /// The whole phrase inside the parentheses of a more-lines row.
    pub expand_hint: Option<String>,
    /// How long a running call has been on screen.
    pub elapsed: Option<Duration>,
}

/// Rows for one tool cell. Issue 221. The cell is not drawn yet.
#[must_use]
pub fn tool_lines(cell: &ToolCell, options: &ToolRenderOptions) -> Vec<Line<'static>> {
    let _ = (cell, options);
    todo!()
}

/// The application's cell renderer, with tool cells drawn by this unit.
pub struct ToolCellLines {
    inner: Box<dyn CellLines>,
    options: ToolRenderOptions,
    elapsed: HashMap<CellId, Duration>,
}

impl ToolCellLines {
    /// Wrap the renderer the application already uses for other cells.
    #[must_use]
    pub fn new(inner: Box<dyn CellLines>, options: ToolRenderOptions) -> Self {
        Self {
            inner,
            options,
            elapsed: HashMap::new(),
        }
    }

    /// Copy elapsed times of running cells. Issue 221. The clock is not read yet.
    pub fn set_clock(&mut self, clock: &ToolClock, now: Instant) {
        let _ = (clock, now, &mut self.elapsed, &self.inner, &self.options);
        todo!()
    }

    /// Replace the more-lines phrase.
    pub fn set_expand_hint(&mut self, hint: Option<String>) {
        self.options.expand_hint = hint;
    }
}

impl CellLines for ToolCellLines {
    fn lines(&self, cell: &Cell, width: u16) -> Vec<Line<'static>> {
        let _ = (cell, width, &self.inner, &self.options, &self.elapsed);
        todo!()
    }
}

/// When each running tool cell was first seen.
pub struct ToolClock {
    first_seen: HashMap<CellId, Instant>,
}

impl ToolClock {
    /// An empty clock.
    #[must_use]
    pub fn new() -> Self {
        Self {
            first_seen: HashMap::new(),
        }
    }

    /// Remember running cells and forget the ones that stopped.
    ///
    /// Issue 221. The cells are not recorded yet.
    pub fn observe(&mut self, cells: &[Cell], now: Instant) {
        let _ = (cells, now, &mut self.first_seen);
        todo!()
    }

    /// How long one running cell has been on screen.
    #[must_use]
    pub fn elapsed(&self, id: CellId, now: Instant) -> Option<Duration> {
        let _ = (id, now, &self.first_seen);
        todo!()
    }

    /// The next whole second of any running cell, or nothing when none run.
    #[must_use]
    pub fn next_tick(&self, now: Instant) -> Option<Instant> {
        let _ = (now, &self.first_seen);
        todo!()
    }
}

impl Default for ToolClock {
    fn default() -> Self {
        Self::new()
    }
}

/// The "preparing" row while a tool call is still being written.
pub struct DraftingLine<'a> {
    /// The tool name, when one is being written.
    pub drafted: Option<&'a str>,
    /// Colours and weight.
    pub theme: &'a ToolTheme,
}

impl LiveSection for DraftingLine<'_> {
    fn height(&self, width: u16) -> u16 {
        let _ = (width, self.drafted, self.theme);
        todo!()
    }

    fn render(&self, area: Rect, buf: &mut Buffer) {
        let _ = (area, buf, self.drafted, self.theme);
        todo!()
    }
}
