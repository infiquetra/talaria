//! Draw the profiles overlay. Issue #253.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use talaria_core::profiles::ProfilesView;

/// Draw one profiles view into the overlay area. Returns the zero-based absolute
/// `(column, row)` for the terminal cursor, or `None` to hide it. Issue #253.
pub fn draw(_view: &ProfilesView, _area: Rect, _buf: &mut Buffer) -> Option<(u16, u16)> {
    todo!("profiles stub")
}
