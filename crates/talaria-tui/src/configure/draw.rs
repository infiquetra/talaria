use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Modifier, Style},
    text::Line,
    widgets::{Clear, Paragraph, Widget},
};
use talaria_core::configure::{ConfigureKey, ConfigureView};

use crate::history::{sanitize, wrap_lines};

/// Resolve a detail scroll key against the wrapped height of the selected item.
pub fn detail_scroll_target(view: &ConfigureView, key: &ConfigureKey, area: Rect) -> Option<usize> {
    let scroll = view.detail_scroll?;
    let selected = view.selected_row?;
    let rows = view
        .lines
        .iter()
        .filter(|line| line.row == Some(selected))
        .map(|line| {
            wrap_lines(
                vec![Line::from(sanitize(&line.text).into_owned())],
                area.width,
            )
            .len()
        })
        .sum::<usize>();
    let max = rows.saturating_sub(usize::from(area.height.saturating_sub(2)));
    let current = scroll.min(max);
    let page = usize::from(area.height.saturating_sub(2)).max(1);
    Some(match key {
        ConfigureKey::PageUp => current.saturating_sub(page),
        ConfigureKey::PageDown => current.saturating_add(page).min(max),
        _ => return None,
    })
}

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
    let confirmation = view
        .confirmation
        .as_deref()
        .or_else(|| {
            view.lines
                .iter()
                .find(|line| {
                    line.row.is_none()
                        && ((view.phase == talaria_core::configure::Phase::Confirming
                            && line.text.starts_with("Apply "))
                            || line.text.starts_with("Discard this proposal?")
                            || line.text.contains("y apply · n skip"))
                })
                .map(|line| line.text.as_str())
        })
        .map(|text| wrap_lines(vec![Line::from(sanitize(text).into_owned())], area.width))
        .unwrap_or_default();
    let confirm_height = confirmation
        .len()
        .min(2)
        .min(usize::from(area.height.saturating_sub(2)));
    let height = usize::from(area.height.saturating_sub(2)).saturating_sub(confirm_height);
    let mut rows = Vec::new();
    for line in &view.lines {
        if (view.phase == talaria_core::configure::Phase::Confirming
            && line.text.starts_with("Apply "))
            || line.text.starts_with("Discard this proposal?")
            || line.text.contains("y apply · n skip")
        {
            continue;
        }
        let style = if line.row.is_some() && line.row == view.selected_row {
            Style::default().add_modifier(Modifier::REVERSED)
        } else {
            Style::default()
        };
        let wrapped = wrap_lines(
            vec![Line::styled(sanitize(&line.text).into_owned(), style)],
            area.width,
        );
        rows.extend(wrapped.into_iter().map(|text| (text, line.row)));
    }
    let selected = view
        .selected_row
        .and_then(|row| rows.iter().position(|(_, owner)| *owner == Some(row)));
    let start = if let (Some(index), Some(scroll)) = (selected, view.detail_scroll) {
        let end = rows
            .iter()
            .skip(index)
            .take_while(|(_, owner)| *owner == view.selected_row)
            .count();
        index + scroll.min(end.saturating_sub(1))
    } else {
        selected.map_or(0, |index| index.saturating_add(1).saturating_sub(height))
    };
    for (offset, (text, _)) in rows.iter().skip(start).take(height).enumerate() {
        let rect = Rect::new(area.x, area.y + 1 + offset as u16, area.width, 1);
        Paragraph::new(text.clone()).render(rect, buf);
    }
    for (offset, text) in confirmation.iter().take(confirm_height).enumerate() {
        let rect = Rect::new(
            area.x,
            area.y + 1 + height as u16 + offset as u16,
            area.width,
            1,
        );
        Paragraph::new(text.clone()).render(rect, buf);
    }
}
