//! Pure live region description: sections, layout and the height cap.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::text::Line;

use crate::history::{split_lines, wrap_lines};

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
    /// One rectangle per section, relative to the viewport. Over `max_height`, the later
    /// sections keep their rows and the first gives way; when the later sections alone are too
    /// tall, the first gets none and rows go to the sections from the last upward.
    pub fn layout(&self, width: u16, max_height: u16) -> Vec<Rect> {
        let wanted: Vec<u16> = self
            .sections
            .iter()
            .map(|section| section.height(width))
            .collect();
        let total: u32 = wanted.iter().map(|&height| u32::from(height)).sum();
        let rest: u32 = wanted.iter().skip(1).map(|&height| u32::from(height)).sum();
        let max = u32::from(max_height);

        let mut given = wanted.clone();
        if total > max {
            if let Some(first) = given.first_mut() {
                // At most `max_height`, so it fits.
                *first = u16::try_from(max.saturating_sub(rest)).unwrap_or(0);
            }
            if rest > max {
                let mut remaining = max_height;
                for (index, height) in given.iter_mut().enumerate().skip(1).rev() {
                    *height = wanted[index].min(remaining);
                    remaining -= *height;
                }
            }
        }

        let mut y: u16 = 0;
        given
            .into_iter()
            .map(|height| {
                let rect = Rect::new(0, y, width, height);
                y = y.saturating_add(height);
                rect
            })
            .collect()
    }
}

/// The mention list above the composer. A short screen keeps the composer first.
#[must_use]
pub fn mention_then_composer<'a>(
    mention: &'a dyn LiveSection,
    composer: &'a dyn LiveSection,
) -> LiveView<'a> {
    LiveView {
        sections: vec![mention, composer],
    }
}

/// `rows - 2`, and 0 below three rows.
pub fn max_live_height(rows: u16) -> u16 {
    rows.saturating_sub(2)
}

/// Plain text split on newlines and wrapped to the width.
pub struct PlainTextSection {
    lines: Vec<Line<'static>>,
    cached: std::sync::Mutex<Option<(u16, Vec<Line<'static>>)>>,
}

impl PlainTextSection {
    pub fn new(text: &str) -> Self {
        let lines = split_lines(text)
            .map(|piece| Line::from(crate::history::sanitize(piece).into_owned()))
            .collect();
        Self {
            lines,
            cached: std::sync::Mutex::new(None),
        }
    }

    fn with_rows<R>(&self, width: u16, f: impl FnOnce(&[Line<'static>]) -> R) -> R {
        let mut guard = self.cached.lock().unwrap_or_else(|e| e.into_inner());
        if guard.as_ref().is_none_or(|(w, _)| *w != width) {
            *guard = Some((width, wrap_lines(self.lines.clone(), width)));
        }
        // An empty cache is filled above before its rows are borrowed.
        let (_, rows) = guard.as_ref().expect("wrapped rows were initialized");
        f(rows)
    }
}

impl LiveSection for PlainTextSection {
    fn height(&self, width: u16) -> u16 {
        self.with_rows(width, |rows| u16::try_from(rows.len()).unwrap_or(u16::MAX))
    }

    /// Draws the last rows that fit, one buffer row each, so nothing is clipped.
    fn render(&self, area: Rect, buf: &mut Buffer) {
        self.with_rows(area.width, |rows| {
            let skip = rows.len().saturating_sub(usize::from(area.height));
            for (offset, row) in (0..area.height).zip(rows.iter().skip(skip)) {
                buf.set_line(area.x, area.y + offset, row, area.width);
            }
        });
    }
}

/// What one draw did.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct FrameStats {
    pub drew: bool,
    pub bytes: usize,
}
