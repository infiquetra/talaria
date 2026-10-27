//! Draw the provider and model picker into a buffer.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use talaria_core::model_roles::RoutePickerView;

pub fn draw_route_picker(view: &RoutePickerView, area: Rect, buf: &mut Buffer) {
    let _ = (view, area, buf);
    todo!("model roles stub")
}
