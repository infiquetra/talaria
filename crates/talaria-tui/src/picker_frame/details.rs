//! Summary and full details painting for the shared picker frame.

use ratatui::{buffer::Buffer, layout::Rect, style::Style, text::Line};
use talaria_core::models::PickerView;

use super::{PickerLayout, PickerMode, paint};
use crate::history::{sanitize, split_lines, wrap_lines};

fn wrapped(view: &PickerView<'_>, width: u16) -> Vec<Line<'static>> {
    let details = view.browse.map(|browse| browse.details).unwrap_or(&[]);
    wrap_lines(
        details
            .iter()
            .flat_map(|line| split_lines(line))
            .map(|line| Line::from(sanitize(line).into_owned()))
            .collect(),
        width,
    )
}

/// Count wrapped full-details rows for a terminal width, including the frame border.
pub fn details_line_count(view: &PickerView<'_>, width: u16) -> usize {
    if width <= 2 {
        return 0;
    }
    wrapped(view, width - 2).len()
}

pub(super) fn draw_wrapped(text: &str, area: Rect, buffer: &mut Buffer) {
    if area.is_empty() {
        return;
    }
    for (offset, line) in wrap_lines(
        split_lines(text)
            .map(|line| Line::from(sanitize(line).into_owned()))
            .collect(),
        area.width,
    )
    .iter()
    .take(usize::from(area.height))
    .enumerate()
    {
        buffer.set_line(
            area.x,
            area.y + u16::try_from(offset).unwrap_or(0),
            line,
            area.width,
        );
    }
}

pub(super) fn draw_summary(view: &PickerView<'_>, layout: &PickerLayout, buffer: &mut Buffer) {
    if layout.mode == PickerMode::Compact {
        if let Some(banner) = view.banner.filter(|_| !view.rows.is_empty()) {
            let start = layout.list.y
                + u16::try_from(view.rows.len())
                    .unwrap_or(u16::MAX)
                    .min(layout.list.height.saturating_sub(2));
            draw_wrapped(
                banner,
                Rect::new(
                    layout.list.x,
                    start,
                    layout.list.width,
                    layout.list.bottom().saturating_sub(start + 1),
                ),
                buffer,
            );
        }
        return;
    }
    let mut area = layout.details;
    if area.is_empty() {
        return;
    }
    if layout.mode == PickerMode::Side {
        for y in area.y..area.bottom() {
            paint("│", Rect::new(area.x, y, 1, 1), Style::default(), buffer);
        }
        area.x += 1;
        area.width = area.width.saturating_sub(1);
        paint("Details", area, Style::default().bold(), buffer);
    } else {
        paint(
            &format!(
                "Details{}",
                "─".repeat(usize::from(area.width.saturating_sub(7)))
            ),
            area,
            Style::default().bold(),
            buffer,
        );
    }
    area.y += 1;
    area.height = area.height.saturating_sub(1);
    if let Some(banner) = view.banner.filter(|text| !text.is_empty()) {
        let rows = wrap_lines(vec![Line::from(sanitize(banner).into_owned())], area.width);
        let height = u16::try_from(rows.len())
            .unwrap_or(u16::MAX)
            .min(area.height);
        draw_wrapped(
            banner,
            Rect::new(area.x, area.y, area.width, height),
            buffer,
        );
        area.y += height;
        area.height -= height;
    }
    if let Some(browse) = view.browse {
        for (offset, text) in browse
            .details
            .iter()
            .take(usize::from(area.height))
            .enumerate()
        {
            paint(
                text,
                Rect::new(
                    area.x,
                    area.y + u16::try_from(offset).unwrap_or(0),
                    area.width,
                    1,
                ),
                Style::default(),
                buffer,
            );
        }
    }
}

pub(super) fn draw_full(view: &PickerView<'_>, area: Rect, buffer: &mut Buffer) {
    if area.width < 3 || area.height < 3 {
        return;
    }
    let inner = Rect::new(area.x + 1, area.y + 1, area.width - 2, area.height - 2);
    for y in inner.y..inner.bottom() {
        for x in inner.x..inner.right() {
            buffer[(x, y)].reset();
        }
    }
    let rows = wrapped(view, inner.width);
    let capacity = usize::from(inner.height.saturating_sub(1));
    let scroll = view.browse.map_or(0, |browse| browse.details_scroll);
    let max_scroll = rows.len().saturating_sub(capacity);
    let top = if scroll > usize::MAX / 2 {
        max_scroll.saturating_sub(usize::MAX - scroll)
    } else {
        scroll.min(max_scroll)
    };
    for (offset, line) in rows.iter().skip(top).take(capacity).enumerate() {
        buffer.set_line(
            inner.x,
            inner.y + u16::try_from(offset).unwrap_or(0),
            line,
            inner.width,
        );
    }
    paint(
        "Up/Down scroll · Home/End · Esc back",
        Rect::new(inner.x, inner.bottom() - 1, inner.width, 1),
        Style::default(),
        buffer,
    );
}
