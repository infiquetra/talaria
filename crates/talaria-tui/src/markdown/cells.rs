use ratatui::style::{Modifier, Style};
use ratatui::text::Line;
use talaria_core::session::Cell;

use crate::automation::{AutomationTheme, hermes_command_lines_with_options_and_theme};
use crate::history::{CellLines, PlainCellLines};

use super::{MarkdownOptions, MarkdownTheme, render_markdown_with_theme};
use crate::theme::surfaces::ViewStyles;

pub struct MarkdownCellLines {
    color: bool,
    theme: MarkdownTheme,
    views: ViewStyles,
}

impl MarkdownCellLines {
    #[must_use]
    pub fn new(color: bool, theme: MarkdownTheme) -> Self {
        Self::new_with_theme(color, theme, ViewStyles::default())
    }

    #[must_use]
    pub fn new_with_theme(color: bool, theme: MarkdownTheme, views: ViewStyles) -> Self {
        Self {
            color,
            theme,
            views,
        }
    }
}

impl CellLines for MarkdownCellLines {
    fn lines(&self, cell: &Cell, width: u16) -> Vec<Line<'static>> {
        let (text, base_style) = match cell {
            Cell::Assistant(cell) => (&cell.text, Style::default()),
            Cell::Reasoning(cell) => (&cell.text, Style::default().add_modifier(Modifier::DIM)),
            Cell::HermesCommand(cell) => {
                return hermes_command_lines_with_options_and_theme(
                    cell,
                    width,
                    &AutomationTheme::default(),
                    &MarkdownOptions {
                        width,
                        color: self.color,
                        theme: self.theme.clone(),
                        base_style: Style::default(),
                    },
                    &self.views,
                );
            }
            _ => return PlainCellLines.lines(cell, width),
        };
        render_markdown_with_theme(
            text,
            &MarkdownOptions {
                width,
                color: self.color,
                theme: self.theme.clone(),
                base_style,
            },
            &self.views,
        )
    }
}
