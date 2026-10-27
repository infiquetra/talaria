//! Draw the model roles page into a buffer.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use talaria_core::model_roles::RolesView;

pub fn draw_model_roles(view: &RolesView, area: Rect, buf: &mut Buffer) {
    let _ = (view, area, buf);
    todo!("model roles stub")
}
