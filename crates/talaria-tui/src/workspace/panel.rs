//! The work panel's frame and hosted checklist.

use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Modifier, Style},
    widgets::{Clear, Widget},
};
use talaria_core::checklist::Checklist;

use crate::{
    checklist::{ChecklistSection, ChecklistStyle, PANEL_MAX_ROWS},
    live::LiveSection,
};

/// Draw an opaque panel over the transcript when necessary. Content starts after
/// its one-column divider; the footer never belongs to this rectangle.
pub fn draw(
    area: Rect,
    buffer: &mut Buffer,
    checklist: Option<&Checklist>,
    expanded: bool,
    style: &ChecklistStyle,
    hint: Option<String>,
) {
    if area.is_empty() {
        return;
    }
    Clear.render(area, buffer);
    for row in area.y..area.bottom() {
        buffer.set_stringn(area.x, row, "│", 1, Style::default());
    }
    let content = Rect::new(
        area.x + 1,
        area.y,
        area.width.saturating_sub(1),
        area.height,
    );
    if content.is_empty() {
        return;
    }
    buffer.set_stringn(
        content.x,
        content.y,
        "Work",
        usize::from(content.width),
        Style::default().add_modifier(Modifier::BOLD),
    );
    if let Some(checklist) = checklist {
        let offset = if content.height > 2 { 2 } else { 1 };
        let height = content.height.saturating_sub(offset).min(PANEL_MAX_ROWS);
        if height > 0 {
            ChecklistSection::new(checklist, expanded, height, hint, style).render(
                Rect::new(content.x, content.y + offset, content.width, height),
                buffer,
            );
        }
    }
}
