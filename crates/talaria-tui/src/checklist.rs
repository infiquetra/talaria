//! The agent's task checklist: the live panel and the committed block.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::Line;

use talaria_core::checklist::{Checklist, ChecklistCommit};

use crate::live::LiveSection;

/// Most rows the live panel ever shows.
pub const PANEL_MAX_ROWS: u16 = 8;

/// Below this many terminal rows the panel is always collapsed.
pub const SHORT_TERMINAL_ROWS: u16 = 16;

/// Most item rows one committed block shows.
pub const BLOCK_MAX_ITEMS: usize = 50;

/// Styles for the checklist panel and block.
#[derive(Debug, Clone)]
pub struct ChecklistStyle {
    pub header: Style,
    pub hint: Style,
    pub in_progress: Style,
    pub pending: Style,
    pub completed: Style,
    pub cancelled: Style,
    pub glyph_in_progress: Style,
    pub glyph_completed: Style,
}

impl ChecklistStyle {
    /// The full-colour styles.
    #[allow(clippy::should_implement_trait)]
    pub fn default() -> ChecklistStyle {
        unimplemented!("#226")
    }

    /// Bold and dim kept, every colour dropped.
    pub fn plain() -> ChecklistStyle {
        unimplemented!("#226")
    }
}

/// The live panel above the composer.
#[allow(dead_code)]
pub struct ChecklistSection<'a> {
    checklist: &'a Checklist,
    expanded: bool,
    max_rows: u16,
    hint: Option<String>,
    style: &'a ChecklistStyle,
}

impl<'a> ChecklistSection<'a> {
    /// Show this list in this form with this row budget.
    pub fn new(
        checklist: &'a Checklist,
        expanded: bool,
        max_rows: u16,
        hint: Option<String>,
        style: &'a ChecklistStyle,
    ) -> Self {
        Self {
            checklist,
            expanded,
            max_rows,
            hint,
            style,
        }
    }

    /// The panel rows at this width.
    pub fn lines(&self, width: u16) -> Vec<Line<'static>> {
        let _ = width;
        unimplemented!("#226")
    }
}

impl LiveSection for ChecklistSection<'_> {
    fn height(&self, width: u16) -> u16 {
        let _ = width;
        unimplemented!("#226")
    }

    fn render(&self, area: Rect, buf: &mut Buffer) {
        let _ = (area, buf);
        unimplemented!("#226")
    }
}

/// The scrollback block for one commit.
pub fn committed_block_lines(
    commit: &ChecklistCommit,
    width: u16,
    style: &ChecklistStyle,
) -> Vec<Line<'static>> {
    let _ = (commit, width, style);
    unimplemented!("#226")
}

/// Cut text to this display width on grapheme boundaries, ending `…`.
pub fn cut_to_width(text: &str, width: u16) -> String {
    let _ = (text, width);
    unimplemented!("#226")
}
