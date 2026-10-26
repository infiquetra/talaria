//! Draw the review overlay. Step 0 draws nothing. Issue #245.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::text::Span;
use talaria_core::review::{PrLabel, ReviewView};

use crate::tool_cell::ToolTheme;

/// Draw the review into `buf`. Step 0 draws nothing.
pub fn draw_review(view: &ReviewView<'_>, theme: &ToolTheme, area: Rect, buf: &mut Buffer) {
    let _ = (view, theme, area, buf);
}

/// Draw one pull-request label, cut to `width` columns.
pub fn draw_pr_label(label: &PrLabel, width: u16) -> Span<'static> {
    let _ = (label, width);
    todo!()
}
