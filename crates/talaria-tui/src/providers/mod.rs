//! Providers screen drawing and the secret field.

mod clipboard;
mod endpoint_form;
mod secret_field;
mod signin_card;

pub use clipboard::osc52;
pub use secret_field::{SecretField, SecretOutcome};

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use talaria_core::providers::ProvidersView;

/// Draw the providers overlay. The stub writes no cells.
pub fn draw_providers(view: &ProvidersView, secret_len: usize, area: Rect, buf: &mut Buffer) {
    let _ = (
        view,
        secret_len,
        area,
        buf,
        signin_card::waiting_lines(),
        endpoint_form::form_lines(),
    );
}
