//! Drawing the session picker overlay (#231).

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use talaria_core::directory::state::PickerState;

/// The picker's styles. A later unit fills them.
#[derive(Clone, Debug)]
pub struct PickerTheme {
    _held: (),
}

impl PickerTheme {
    /// The default theme.
    pub fn new() -> Self {
        Self { _held: () }
    }
}

impl Default for PickerTheme {
    fn default() -> Self {
        Self::new()
    }
}

/// Draws the picker state inside the overlay closure.
pub struct SessionPickerView;

impl SessionPickerView {
    /// Draw the picker. The stub draws nothing.
    pub fn draw(
        _state: &PickerState,
        _area: Rect,
        _buf: &mut Buffer,
        _now: f64,
        _theme: &PickerTheme,
    ) {
    }
}
