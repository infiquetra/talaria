use ratatui::text::Line;
use talaria_core::session::Cell;

use crate::history::{CellLines, PlainCellLines};

use super::MarkdownTheme;

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
        let _ = (
            self.color,
            self.theme.heading1,
            width,
            cell.is_final(),
            PlainCellLines,
        );
        todo!()
    }
}
