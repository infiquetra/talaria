//! Rewind overlay drawing and keys. Issue 233. Checkpoint rewind and diff.
//!
//! The diff render options from the diff view are not on this branch. This draw takes the
//! rewind view, which already carries the diff document.

use crossterm::event::KeyEvent;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Style;
use talaria_core::rewind::{RewindKey, RewindScreen, RewindView};

/// Colours for the rewind overlay.
pub struct RewindTheme {
    /// The selected row.
    pub selected: Style,
    /// A dimmed row.
    pub dim: Style,
    /// A warning line.
    pub warning: Style,
    /// An accent line.
    pub accent: Style,
}

/// Draw the rewind overlay into `buf`.
pub fn draw_rewind(view: &RewindView<'_>, area: Rect, buf: &mut Buffer, theme: &RewindTheme) {
    let _ = (view, area, buf, theme);
    todo!("Issue 233. Checkpoint rewind and diff.")
}

/// How many list rows fit in `area`.
#[must_use]
pub fn visible_rows(area: Rect) -> u16 {
    let _ = area;
    todo!("Issue 233. Checkpoint rewind and diff.")
}

/// Map one key on `screen` to a rewind input. `None` means the key is ignored.
#[must_use]
pub fn key_to_rewind(key: &KeyEvent, screen: RewindScreen) -> Option<RewindKey> {
    let _ = (key, screen);
    todo!("Issue 233. Checkpoint rewind and diff.")
}
