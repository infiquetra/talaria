use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::text::Line;

use crate::live::LiveSection;

pub struct MarkdownTailSection {
    tail: Vec<Line<'static>>,
}

impl MarkdownTailSection {
    #[must_use]
    pub fn new(tail: Vec<Line<'static>>) -> Self {
        Self { tail }
    }
}

impl LiveSection for MarkdownTailSection {
    fn height(&self, width: u16) -> u16 {
        let _ = (self.tail.len(), width);
        todo!()
    }

    fn render(&self, area: Rect, buf: &mut Buffer) {
        let _ = (self.tail.len(), area, buf.area());
        todo!()
    }
}
