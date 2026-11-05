//! Pure picker geometry shared with the application viewport calculation.

use ratatui::layout::Rect;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PickerMode {
    #[default]
    SizeMessage,
    Compact,
    Below,
    Side,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PickerLayout {
    pub content: Rect,
    pub filter: Rect,
    pub chips: Rect,
    pub count: Rect,
    pub list: Rect,
    pub details: Rect,
    pub footer: Rect,
    pub mode: PickerMode,
}

impl PickerLayout {
    pub fn new(_area: Rect) -> Self {
        Self::default()
    }
}

/// Number of list rows available after reserving the scroll indicator.
pub fn list_height(_area: Rect) -> usize {
    0
}
