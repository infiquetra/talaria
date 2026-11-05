//! Shared, terminal-independent picker surface.

mod details;
pub mod layout;
mod list;

use ratatui::{buffer::Buffer, layout::Rect};
use talaria_core::models::PickerView;

pub use layout::{PickerLayout, PickerMode, list_height};

/// Draw a picker inside the supplied area.
pub fn draw_picker_frame(_view: &PickerView<'_>, _area: Rect, _buffer: &mut Buffer) {}
