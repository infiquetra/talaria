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
    fn height(&self, _width: u16) -> u16 {
        u16::try_from(self.tail.len()).unwrap_or(u16::MAX)
    }

    fn render(&self, area: Rect, buf: &mut Buffer) {
        let take = usize::from(area.height);
        let skip = self.tail.len().saturating_sub(take);
        for (index, line) in self.tail.iter().skip(skip).take(take).enumerate() {
            buf.set_line(area.x, area.y + index as u16, line, area.width);
        }
    }
}
