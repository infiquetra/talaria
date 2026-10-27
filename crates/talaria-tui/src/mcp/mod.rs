use ratatui::{buffer::Buffer, layout::Rect};
use talaria_core::mcp::{McpView, SecretSlot};

pub fn draw_mcp(
    _view: &McpView,
    _secret_len: &dyn Fn(SecretSlot) -> usize,
    _area: Rect,
    _buf: &mut Buffer,
) {
    todo!("mcp stub")
}
