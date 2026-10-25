//! The mention list, drawn above the composer.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use talaria_core::attachments::MentionView;

use crate::live::LiveSection;

/// One draw of a [`MentionView`].
pub struct MentionSection<'a> {
    view: &'a MentionView,
}

impl<'a> MentionSection<'a> {
    /// Draw `view`.
    #[must_use]
    pub fn new(view: &'a MentionView) -> Self {
        Self { view }
    }
}

impl LiveSection for MentionSection<'_> {
    fn height(&self, width: u16) -> u16 {
        let _ = (self.view, width);
        unimplemented!()
    }

    fn render(&self, area: Rect, buf: &mut Buffer) {
        let _ = (self.view, area, buf);
        unimplemented!()
    }
}
