//! Shell and side cells and the side-tasks section.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::Line;

use talaria_core::side::{ShellCell, SideCell, SideTasks};

use crate::live::LiveSection;

/// Warning, error and dim styles for side content.
#[derive(Debug, Clone)]
pub struct SideTheme {
    pub warning: Style,
    pub error: Style,
    pub dim: Style,
}

impl Default for SideTheme {
    fn default() -> Self {
        Self {
            warning: Style::default().fg(Color::Yellow),
            error: Style::default().fg(Color::Red),
            dim: Style::default().add_modifier(Modifier::DIM),
        }
    }
}

/// The lines of one shell cell at this width.
pub fn shell_cell_lines(cell: &ShellCell, width: u16, theme: &SideTheme) -> Vec<Line<'static>> {
    let _ = (cell, width, theme);
    todo!("#225")
}

/// The lines of one side cell at this width.
pub fn side_cell_lines(cell: &SideCell, width: u16, theme: &SideTheme) -> Vec<Line<'static>> {
    let _ = (cell, width, theme);
    todo!("#225")
}

/// The pending shell and side rows above the composer.
pub struct SideTasksSection<'a> {
    pub tasks: &'a SideTasks,
    pub now_ms: u64,
    pub theme: &'a SideTheme,
}

impl SideTasksSection<'_> {
    /// When the next row changes, if any row is pending.
    pub fn next_tick(&self) -> Option<u64> {
        todo!("#225")
    }
}

impl LiveSection for SideTasksSection<'_> {
    fn height(&self, width: u16) -> u16 {
        let _ = width;
        todo!("#225")
    }

    fn render(&self, area: Rect, buf: &mut Buffer) {
        let _ = (area, buf);
        todo!("#225")
    }
}

/// The composer title for a shell draft, if the draft opens one.
pub fn shell_mode_title(draft: &str) -> Option<&'static str> {
    let _ = draft;
    todo!("#225")
}
