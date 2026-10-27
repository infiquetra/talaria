use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Modifier, Style},
    text::Line,
    widgets::{Clear, Paragraph, Widget},
};
use talaria_core::configure::ConfigureView;

use crate::history::sanitize;

/// Draw a configuration assistant snapshot, keeping the selected row visible.
pub fn draw_configure(view: &ConfigureView, area: Rect, buf: &mut Buffer) {
    if area.width == 0 || area.height == 0 {
        return;
    }
    Clear.render(area, buf);
    let title = Rect::new(area.x, area.y, area.width, 1);
    Paragraph::new(sanitize(&view.title).into_owned()).render(title, buf);
    if area.height < 2 {
        return;
    }
    let footer = Rect::new(area.x, area.bottom() - 1, area.width, 1);
    Paragraph::new(sanitize(&view.footer).into_owned()).render(footer, buf);
    let height = usize::from(area.height.saturating_sub(2));
    let selected = view
        .selected_row
        .and_then(|row| view.lines.iter().position(|line| line.row == Some(row)));
    let start = selected.map_or(0, |index| index.saturating_add(1).saturating_sub(height));
    for (offset, line) in view.lines.iter().skip(start).take(height).enumerate() {
        let style = if line.row.is_some() && line.row == view.selected_row {
            Style::default().add_modifier(Modifier::REVERSED)
        } else {
            Style::default()
        };
        let rect = Rect::new(area.x, area.y + 1 + offset as u16, area.width, 1);
        Paragraph::new(Line::styled(sanitize(&line.text).into_owned(), style)).render(rect, buf);
    }
}
