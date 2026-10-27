use ratatui::{buffer::Buffer, layout::Rect};
use talaria_core::capabilities::{CapInput, CapView};
pub use talaria_core::capabilities::{CapTab, KeyAction};

use crate::keys::{Key, KeyMap};

pub fn draw_capabilities(_view: &CapView, _area: Rect, _buf: &mut Buffer) {
    todo!("capabilities stub")
}

pub fn route_capabilities(_view: &CapView, _map: &KeyMap, _key: &Key) -> Option<CapInput> {
    todo!("capabilities stub")
}
