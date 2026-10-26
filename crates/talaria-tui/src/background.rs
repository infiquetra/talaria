//! The live rows and the process overlay for background work.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::Line;
use talaria_core::background::{BackgroundCell, BackgroundWork, ProcessId};

use crate::keys::Key;
use crate::live::LiveSection;

pub use talaria_core::background::ProcessesIntent;

/// Colours for the live rows, the overlay, and a result cell.
#[derive(Clone, Debug, PartialEq)]
pub struct BackgroundTheme {
    /// Dim text: footers, elapsed times, closed-view notes.
    pub dim: Style,
    /// A warning note.
    pub warning: Style,
    /// An error note or a failed result.
    pub error: Style,
    /// A finished process that exited cleanly.
    pub ok: Style,
    /// The selected overlay row.
    pub selected: Style,
}

impl Default for BackgroundTheme {
    fn default() -> Self {
        Self {
            dim: Style::default().add_modifier(Modifier::DIM),
            warning: Style::default().fg(Color::Yellow),
            error: Style::default().fg(Color::Red),
            ok: Style::default().fg(Color::Green),
            selected: Style::default().add_modifier(Modifier::REVERSED),
        }
    }
}

/// The live rows above the composer.
pub struct BackgroundSection<'a> {
    /// The session's background work.
    pub work: &'a BackgroundWork,
    /// Width used when a caller asks for height before the area is known.
    pub width_hint: u16,
    /// Colours.
    pub theme: &'a BackgroundTheme,
}

impl LiveSection for BackgroundSection<'_> {
    fn height(&self, width: u16) -> u16 {
        let _ = (self.work, self.width_hint, self.theme, width);
        todo!()
    }

    fn render(&self, area: Rect, buf: &mut Buffer) {
        let _ = (self.work, self.width_hint, self.theme, area, buf);
        todo!()
    }
}

/// Lines for one finished background-task cell.
#[must_use]
pub fn background_cell_lines(
    cell: &BackgroundCell,
    width: u16,
    theme: &BackgroundTheme,
) -> Vec<Line<'static>> {
    let _ = (cell, width, theme);
    todo!()
}

/// The process overlay: selection, scroll, follow, and a confirm that expires.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ProcessesView {
    selected: Option<ProcessId>,
    scroll: usize,
    follow: bool,
    confirm: Option<(char, u64)>,
    note: Option<String>,
}

impl ProcessesView {
    /// An overlay with no selection and follow on.
    #[must_use]
    pub fn new() -> Self {
        Self {
            follow: true,
            ..Self::default()
        }
    }

    /// Handle one key. `now_ms` is the app clock, used for the confirm window.
    #[must_use]
    pub fn handle_key(&mut self, key: Key, work: &BackgroundWork, now_ms: u64) -> ProcessesIntent {
        let _ = (self, key, work, now_ms);
        todo!()
    }

    /// Draw the overlay into `buf` for `area`.
    pub fn render(
        &self,
        work: &BackgroundWork,
        now_ms: u64,
        wall_s: f64,
        area: Rect,
        buf: &mut Buffer,
        theme: &BackgroundTheme,
    ) {
        let _ = (self, work, now_ms, wall_s, area, buf, theme);
        todo!()
    }
}
