//! Draw the board overlay. The draw stands in until the view exists.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use talaria_core::kanban::KanbanFrame;

pub struct KanbanOverlay {
    open: bool,
    inline: bool,
}

impl KanbanOverlay {
    pub fn open() -> Self {
        Self {
            open: true,
            inline: false,
        }
    }

    pub fn close(&mut self) {
        self.open = false;
    }

    pub fn is_open(&self) -> bool {
        self.open
    }

    pub fn showing_inline(&self) -> bool {
        self.inline
    }
}

pub fn draw_kanban(frame: &KanbanFrame, area: Rect, buf: &mut Buffer) {
    let _ = (frame, area, buf);
    panic!("view missing");
}
