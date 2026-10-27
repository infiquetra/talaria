//! Memory and curator presentation.

use ratatui::{buffer::Buffer, layout::Rect};
use talaria_core::memory::{MemoryKey, MemoryView};

use crate::keys::{Key, KeyMap};

/// Draw the memory overlay.
pub fn draw_memory(_view: &MemoryView, _area: Rect, _buf: &mut Buffer) {
    todo!("draw the memory overlay")
}

/// Route a key to the memory screen, preserving editor input and modal confirmations.
pub fn route_memory(_view: &MemoryView, _map: &KeyMap, _key: &Key) -> Option<MemoryKey> {
    todo!("route memory keys")
}
