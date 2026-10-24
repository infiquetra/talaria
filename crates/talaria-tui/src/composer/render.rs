//! Draw the composer into a live section. Styles stay on the default colours.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;

use crate::live::LiveSection;

/// A prepared view of the composer for one draw.
#[derive(Debug)]
pub struct ComposerSection<'a> {
    composer: &'a super::Composer,
}

impl<'a> ComposerSection<'a> {
    pub(crate) fn new(composer: &'a super::Composer) -> Self {
        Self { composer }
    }
}

impl LiveSection for ComposerSection<'_> {
    fn height(&self, _width: u16) -> u16 {
        let _ = self.composer;
        todo!()
    }

    fn render(&self, _area: Rect, _buf: &mut Buffer) {
        let _ = self.composer;
        todo!()
    }
}
