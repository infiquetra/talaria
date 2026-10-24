//! Pure history rendering: cells to lines, sanitizing, wrapping and the commit ledger.

use std::borrow::Cow;

use ratatui::text::Line;
use talaria_core::session::Cell;

/// Turns one cell into lines for the given width.
pub trait CellLines {
    fn lines(&self, cell: &Cell, width: u16) -> Vec<Line<'static>>;
}

/// One plain rendering per cell variant.
pub struct PlainCellLines;

impl CellLines for PlainCellLines {
    fn lines(&self, cell: &Cell, width: u16) -> Vec<Line<'static>> {
        let _ = (cell, width);
        todo!("history")
    }
}

/// Remembers which cells were already handed out.
pub struct CommitLedger {
    _private: (),
}

impl Default for CommitLedger {
    fn default() -> Self {
        Self::new()
    }
}

impl CommitLedger {
    pub fn new() -> Self {
        todo!("history")
    }

    /// The longest run of final cells after the last one handed out.
    pub fn take_ready<'a>(&mut self, cells: &'a [Cell]) -> &'a [Cell] {
        let _ = cells;
        todo!("history")
    }

    /// A new foreground session starts again from the first cell.
    pub fn reset(&mut self) {
        todo!("history")
    }
}

/// Wrap by grapheme with the terminal's width rule.
pub fn wrap_lines(lines: Vec<Line<'static>>, width: u16) -> Vec<Line<'static>> {
    let _ = (lines, width);
    todo!("history")
}

/// Replace controls and direction marks with visible stand-ins. Clean text is borrowed.
pub fn sanitize(text: &str) -> Cow<'_, str> {
    let _ = text;
    todo!("history")
}
