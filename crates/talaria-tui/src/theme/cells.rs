//! Assistant and reasoning cells drawn with one resolved theme.
//!
//! A cell renders with the theme it started under, so a reply that streams across a skin change
//! finishes and commits in its starting colours; the caller keeps that theme per cell id.

use ratatui::style::{Modifier, Style};
use ratatui::text::Line;
use talaria_core::session::{Cell, CellId};

use super::Theme;
use crate::history::{CellLines, PlainCellLines};
use crate::markdown::{MarkdownOptions, render_markdown};

/// Draws Markdown cells with one theme; every other cell keeps its owner's look.
pub struct ThemedCells<'a> {
    /// The theme the cells started under.
    pub theme: &'a Theme,
}

/// The base style of reasoning text: the skin's thinking colour, else dim as in phase 1.
pub fn reasoning_base(theme: &Theme) -> Style {
    if theme.reasoning.fg.is_some() {
        theme.reasoning
    } else {
        theme.reasoning.add_modifier(Modifier::DIM)
    }
}

impl CellLines for ThemedCells<'_> {
    fn lines(&self, cell: &Cell, width: u16) -> Vec<Line<'static>> {
        let (text, base_style) = match cell {
            Cell::Assistant(cell) => (&cell.text, Style::default()),
            Cell::Reasoning(cell) => (&cell.text, reasoning_base(self.theme)),
            _ => return PlainCellLines.lines(cell, width),
        };
        render_markdown(
            text,
            &MarkdownOptions {
                width,
                theme: self.theme.markdown.clone(),
                color: self.theme.color,
                base_style,
            },
        )
    }
}

/// The id of a cell drawn with a theme (assistant and reasoning text), if it is one.
pub fn themed_cell_id(cell: &Cell) -> Option<CellId> {
    match cell {
        Cell::Assistant(cell) => Some(cell.id),
        Cell::Reasoning(cell) => Some(cell.id),
        _ => None,
    }
}
