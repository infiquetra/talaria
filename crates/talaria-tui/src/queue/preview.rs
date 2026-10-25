//! Indicator, preview, and the composer title for a queue.
//!
//! Issue 232. Queue and steer.

use ratatui::text::{Line, Span};
use talaria_core::queue::QueueView;

/// The right-aligned count. No label when `count` is 0.
///
/// Issue 232. Queue and steer.
#[must_use]
pub fn render_indicator(count: usize, paused: bool, width: u16) -> Span<'static> {
    let _ = (count, paused, width);
    todo!("Issue 232. Queue and steer.")
}

/// At most five dimmed preview lines, or at most ten selection lines.
///
/// Issue 232. Queue and steer. `show_select_hint` adds ` · ↑ select` at 60 columns or more.
#[must_use]
pub fn render_preview(view: &QueueView, width: u16, show_select_hint: bool) -> Vec<Line<'static>> {
    let _ = (view, width, show_select_hint);
    todo!("Issue 232. Queue and steer.")
}

/// Composer title while an edit is open, or `Sending…` while a submit is unconfirmed.
///
/// Issue 232. Queue and steer.
#[must_use]
pub fn composer_title(view: &QueueView) -> Option<String> {
    let _ = view;
    todo!("Issue 232. Queue and steer.")
}
