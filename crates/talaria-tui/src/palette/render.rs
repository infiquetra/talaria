//! Draw the slash palette. Issue #223. The body draws nothing until the layout lands.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;

use super::PaletteSection;
use crate::live::LiveSection;

/// The draw used by [`LiveSection::render`]. Issue #223. It draws nothing yet.
pub fn render_palette(section: &PaletteSection, area: Rect, buf: &mut Buffer) {
    let _ = (section.state(), section.terminal_rows(), area, buf);
}

impl LiveSection for PaletteSection {
    fn height(&self, width: u16) -> u16 {
        let _ = (width, self.terminal_rows(), self.state());
        0
    }

    fn render(&self, area: Rect, buf: &mut Buffer) {
        render_palette(self, area, buf);
    }
}
