//! Per-cell render cache for final transcript cells. Issue 222.
//!
//! A final cell never changes, so its rendered lines are kept by cell id, width and detail level.
//! A cell that is still live is drawn again every time, because it may still grow.
//!
//! Movement and drawing share the same rendered lines.

use std::collections::HashMap;

use ratatui::style::Style;
use ratatui::text::Line;
use talaria_core::session::{Cell, CellId};

use super::render::Details;
use crate::history::{CellLines, sanitize_line};

/// The rendered lines of the cells of one session.
#[derive(Debug, Default)]
pub struct Document {
    drawn: HashMap<(CellId, u16, u8), Vec<Line<'static>>>,
}

impl Document {
    /// An empty document.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Forget every rendered cell.
    pub fn clear(&mut self) {
        self.drawn.clear();
    }

    /// The lines of one cell, with the blank line that separates it from the one before.
    ///
    /// The blank line belongs to the cell, so a position inside a cell can point at it.
    pub fn cell_lines(
        &mut self,
        cell: &Cell,
        index: usize,
        width: u16,
        details: Details,
        tool: &dyn CellLines,
        own: &dyn CellLines,
    ) -> Vec<Line<'static>> {
        // A tool cell is drawn by the renderer the application passes in, so the unit that owns
        // the tool form can take it over. Every other known cell is drawn by the pager.
        let render: &dyn CellLines = if matches!(cell, Cell::Tool(_)) {
            tool
        } else {
            own
        };
        self.lines(cell, index, width, details, render)
    }

    /// The lines of one cell, from one of the two caches.
    fn lines(
        &mut self,
        cell: &Cell,
        index: usize,
        width: u16,
        details: Details,
        render: &dyn CellLines,
    ) -> Vec<Line<'static>> {
        let cache = &mut self.drawn;
        let mut lines = if cell.is_final()
            && let Some(id) = cell_id(cell)
        {
            let key = (id, width, detail_key(details));
            match cache.get(&key) {
                Some(cached) => cached.clone(),
                None => {
                    let made = render.lines(cell, width);
                    cache.insert(key, made.clone());
                    made
                }
            }
        } else {
            render.lines(cell, width)
        };
        // No raw escape byte from a cell may reach the terminal.
        for line in &mut lines {
            *line = sanitize_line(std::mem::take(line));
        }
        if details == Details::Compact
            && matches!(cell, Cell::Tool(_))
            && !has_compact_marker(&lines)
            && !counts_hidden_lines(&lines)
        {
            let body = lines.len().saturating_sub(1);
            let mut compact = vec![lines.first().cloned().unwrap_or_default()];
            compact.push(Line::styled(format!(" (+{body} lines)"), Style::default()));
            lines = compact;
        }
        if index > 0 && matches!(cell, Cell::User(_)) {
            lines.insert(0, Line::default());
        }
        lines
    }

    /// The bottom position using the same renderers as the visible document.
    pub fn rendered_bottom_snap(
        &mut self,
        cells: &[Cell],
        height: u16,
        width: u16,
        details: Details,
        tool: &dyn CellLines,
        own: &dyn CellLines,
    ) -> (usize, usize) {
        let mut remaining = usize::from(height).max(1);
        for index in (0..cells.len()).rev() {
            let count = self
                .cell_lines(&cells[index], index, width, details, tool, own)
                .len();
            if count >= remaining {
                return (index, count - remaining);
            }
            remaining -= count;
        }
        (0, 0)
    }
}

/// The identity of a cell.
fn cell_id(cell: &Cell) -> Option<CellId> {
    match cell {
        Cell::User(inner) => Some(inner.id),
        Cell::Assistant(inner) => Some(inner.id),
        Cell::Reasoning(inner) => Some(inner.id),
        Cell::Tool(inner) => Some(inner.id),
        Cell::Notice(inner) => Some(inner.id),
        Cell::Prompt(inner) => Some(inner.id),
        Cell::Shell(_) | Cell::Side(_) | Cell::HermesCommand(_) | Cell::Background(_) => None,
    }
}

/// The detail level as a key. The level has only two values.
fn detail_key(details: Details) -> u8 {
    match details {
        Details::Expanded => 0,
        Details::Compact => 1,
    }
}

/// Whether the lines already say how many lines the body hides.
fn counts_hidden_lines(lines: &[Line<'static>]) -> bool {
    lines.iter().any(|line| {
        let text: String = line
            .spans
            .iter()
            .map(|span| span.content.as_ref())
            .collect();
        text.contains('+') && text.contains(" lines")
    })
}

/// Whether the lines already carry the compact marker of a hidden body.
fn has_compact_marker(lines: &[Line<'static>]) -> bool {
    let Some(last) = lines.last() else {
        return false;
    };
    let text: String = last
        .spans
        .iter()
        .map(|span| span.content.as_ref())
        .collect();
    text.starts_with(" (+") && text.ends_with(" lines)")
}
