use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Modifier, Style},
    text::Line,
    widgets::{Clear, Paragraph, Widget, Wrap},
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
    let available = area.height.saturating_sub(2);
    let question = view
        .confirmation
        .as_ref()
        .map(|text| sanitize(text).into_owned());
    let question_height = question.as_ref().map_or(0, |text| {
        let bounds = Rect::new(0, 0, area.width, available);
        let mut measured = Buffer::empty(bounds);
        Paragraph::new(text.as_str())
            .wrap(Wrap { trim: true })
            .render(bounds, &mut measured);
        (0..available)
            .rev()
            .find(|y| (0..area.width).any(|x| !measured[(x, *y)].symbol().trim().is_empty()))
            .map_or(0, |y| y + 1)
    });
    if let Some(question) = question {
        let rect = Rect::new(
            area.x,
            footer.y.saturating_sub(question_height),
            area.width,
            question_height,
        );
        Paragraph::new(question)
            .wrap(Wrap { trim: true })
            .render(rect, buf);
    }
    let height = usize::from(available.saturating_sub(question_height));
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
