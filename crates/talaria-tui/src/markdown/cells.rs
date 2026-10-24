use ratatui::style::{Modifier, Style};
use ratatui::text::Line;
use talaria_core::session::Cell;

use crate::history::{CellLines, PlainCellLines};

use super::{MarkdownOptions, MarkdownTheme, render_markdown};

pub struct MarkdownCellLines {
    color: bool,
    theme: MarkdownTheme,
}

impl MarkdownCellLines {
    #[must_use]
    pub fn new(color: bool, theme: MarkdownTheme) -> Self {
        Self { color, theme }
    }
}

impl CellLines for MarkdownCellLines {
    fn lines(&self, cell: &Cell, width: u16) -> Vec<Line<'static>> {
        let (text, base_style) = match cell {
            Cell::Assistant(cell) => (&cell.text, Style::default()),
            Cell::Reasoning(cell) => (&cell.text, Style::default().add_modifier(Modifier::DIM)),
            _ => return PlainCellLines.lines(cell, width),
        };
        render_markdown(
            text,
            &MarkdownOptions {
                width,
                color: self.color,
                theme: self.theme.clone(),
                base_style,
            },
        )
    }
}
