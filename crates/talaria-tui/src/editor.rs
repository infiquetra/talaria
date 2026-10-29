//! A single-line, caret-following viewport for editable values.

use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Modifier, Style},
    text::Line,
    widgets::{Clear, Paragraph, Widget},
};
use unicode_segmentation::UnicodeSegmentation;

pub(crate) fn draw(text: &str, cursor: usize, area: Rect, buf: &mut Buffer) {
    if area.width == 0 || area.height == 0 {
        return;
    }
    Clear.render(area, buf);
    let before: String = text.chars().take(cursor).collect();
    let clean = crate::history::sanitize(text);
    let cursor_col = Line::raw(crate::history::sanitize(&before).as_ref()).width();
    let width = usize::from(area.width);
    let mut skipped = 0;
    let mut start = 0;
    for (at, cluster) in clean.grapheme_indices(true) {
        if cursor_col.saturating_sub(skipped) < width {
            break;
        }
        skipped += Line::raw(cluster).width();
        start = at + cluster.len();
    }
    Paragraph::new(&clean[start..]).render(area, buf);
    let x = cursor_col.saturating_sub(skipped).min(width - 1) as u16;
    buf[(area.x + x, area.y)].set_style(Style::default().add_modifier(Modifier::REVERSED));
}
