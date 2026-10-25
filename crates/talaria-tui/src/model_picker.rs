//! The model overlay and the confirmation card. Issue 230.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use talaria_core::models::PickerView;
use talaria_core::mutation::Confirmation;

use crate::live::LiveSection;

/// Paint the model overlay into the alternate-screen buffer.
pub fn draw_model_picker(view: &PickerView, area: Rect, buf: &mut Buffer) {
    let _ = (view, area, buf);
    todo!()
}

/// The live card that asks whether to switch anyway.
pub struct ModelConfirmSection<'a> {
    pub confirmation: &'a Confirmation,
    pub keep: &'a str,
    draft_waiting: bool,
}

impl<'a> ModelConfirmSection<'a> {
    pub fn new(confirmation: &'a Confirmation, keep: &'a str) -> Self {
        Self {
            confirmation,
            keep,
            draft_waiting: false,
        }
    }

    /// The composer still has a draft, so the card does not take Enter.
    pub fn waiting_on_draft(mut self) -> Self {
        self.draft_waiting = true;
        self
    }
}

impl LiveSection for ModelConfirmSection<'_> {
    fn height(&self, width: u16) -> u16 {
        let _ = (self.confirmation, self.keep, self.draft_waiting, width);
        todo!()
    }

    fn render(&self, area: Rect, buf: &mut Buffer) {
        let _ = (self.confirmation, self.keep, self.draft_waiting, area, buf);
        todo!()
    }
}
