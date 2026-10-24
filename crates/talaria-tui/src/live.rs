//! Pure live region description: sections, layout and the height cap.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;

/// One block of the live region.
pub trait LiveSection {
    fn height(&self, width: u16) -> u16;
    fn render(&self, area: Rect, buf: &mut Buffer);
}

/// The sections of the live region, first (the streaming tail) to last.
pub struct LiveView<'a> {
    pub sections: Vec<&'a dyn LiveSection>,
}

impl LiveView<'_> {
    /// One rectangle per section, relative to the viewport.
    pub fn layout(&self, width: u16, max_height: u16) -> Vec<Rect> {
        let _ = (width, max_height);
        todo!("live")
    }
}

/// `rows - 2`, and 0 below three rows.
pub fn max_live_height(rows: u16) -> u16 {
    let _ = rows;
    todo!("live")
}

/// Plain text split on newlines and wrapped to the width.
pub struct PlainTextSection {
    _private: (),
}

impl PlainTextSection {
    pub fn new(text: &str) -> Self {
        let _ = text;
        todo!("live")
    }
}

impl LiveSection for PlainTextSection {
    fn height(&self, width: u16) -> u16 {
        let _ = width;
        todo!("live")
    }

    fn render(&self, area: Rect, buf: &mut Buffer) {
        let _ = (area, buf);
        todo!("live")
    }
}

/// What one draw did.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct FrameStats {
    pub drew: bool,
    pub bytes: usize,
}
