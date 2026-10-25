//! Transcript cell lines. Issue 222. The drawing is not written yet.

use ratatui::style::{Color, Modifier, Style};
use ratatui::text::Line;
use talaria_core::session::Cell;

use crate::history::CellLines;
use crate::markdown::MarkdownOptions;

/// Expanded tool bodies, or the compact header.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Details {
    /// Headers and bodies.
    Expanded,
    /// Headers, and a line count where a body was hidden.
    Compact,
}

/// Lines for one transcript cell.
pub struct PagerCellLines<'a> {
    /// Expanded or compact.
    pub details: Details,
    /// Markdown options for assistant and reasoning text.
    pub markdown: MarkdownOptions,
    /// Colours and modifiers for the transcript.
    pub theme: PagerTheme,
    /// Lines for a cell kind this screen does not know.
    pub other: &'a dyn CellLines,
}

impl CellLines for PagerCellLines<'_> {
    fn lines(&self, _cell: &Cell, _width: u16) -> Vec<Line<'static>> {
        todo!()
    }
}

/// Styles for the transcript. Colour is dropped later when the Markdown options disable it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PagerTheme {
    /// Header row.
    pub header: Style,
    /// Footer row.
    pub footer: Style,
    /// Banner row.
    pub banner: Style,
    /// A message the person sent.
    pub user: Style,
    /// Dimmed text.
    pub dim: Style,
    /// A tool that succeeded.
    pub tool_ok: Style,
    /// A tool that failed, and an error notice.
    pub tool_failed: Style,
    /// An added diff line.
    pub diff_add: Style,
    /// A removed diff line.
    pub diff_del: Style,
    /// A search match that is not the current one.
    pub search_match: Style,
    /// The current search match.
    pub search_current: Style,
    /// The gutter marker on a cell whose rendered text cannot host the current match.
    pub gutter_mark: Style,
}

impl Default for PagerTheme {
    fn default() -> Self {
        let reversed = Style::default().add_modifier(Modifier::REVERSED);
        let bold = Style::default().add_modifier(Modifier::BOLD);
        Self {
            header: reversed,
            footer: reversed,
            banner: bold,
            user: bold,
            dim: Style::default().add_modifier(Modifier::DIM),
            tool_ok: Style::default().fg(Color::Green),
            tool_failed: Style::default().fg(Color::Red),
            diff_add: Style::default().fg(Color::Green),
            diff_del: Style::default().fg(Color::Red),
            search_match: reversed,
            search_current: reversed.add_modifier(Modifier::BOLD | Modifier::UNDERLINED),
            gutter_mark: bold,
        }
    }
}

/// `0.4s` under a minute, `2m05s` at a minute and beyond.
///
/// Absent, negative and non-finite durations have no text. The formatting is not written yet.
#[must_use]
pub fn format_duration_fallback(_seconds: f64) -> Option<String> {
    todo!()
}
