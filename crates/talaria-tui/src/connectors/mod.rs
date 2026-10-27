//! Connectors and messaging overlay drawing.

#![allow(dead_code)]

mod cards;
mod connectors_tab;
mod list_editor;
mod messaging_tab;

use std::collections::BTreeMap;

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use talaria_core::connectors::ConnectorsView;

pub use talaria_core::connectors::ConnectorsKey;

/// Paint the overlay. The lens holds lengths of typed secrets, never the characters.
pub fn draw_connectors(
    view: &ConnectorsView,
    secret_lens: &BTreeMap<String, usize>,
    area: Rect,
    buf: &mut Buffer,
) {
    let _ = (view, secret_lens, area, buf);
    todo!("connectors renderer stub")
}
