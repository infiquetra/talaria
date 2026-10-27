//! Draw the scheduled jobs overlay from a pure view.

use ratatui::{buffer::Buffer, layout::Rect};
use talaria_core::cron::CronView;

#[allow(dead_code)]
mod detail;
#[allow(dead_code)]
mod form;
#[allow(dead_code)]
mod list;

pub struct CronFrameGate {
    last: Option<CronView>,
}

impl CronFrameGate {
    pub fn new() -> CronFrameGate {
        CronFrameGate { last: None }
    }

    pub fn needs_draw(&mut self, view: &CronView) -> bool {
        let _ = (self, view);
        todo!()
    }
}

impl Default for CronFrameGate {
    fn default() -> Self {
        Self::new()
    }
}

pub fn draw_cron(view: &CronView, area: Rect, buf: &mut Buffer) {
    let _ = (view, area, buf);
    todo!()
}
