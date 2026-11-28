//! Summary and full details painting for the shared picker frame.

use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::Style,
    text::{Line, Span},
};
use talaria_core::models::PickerView;

use super::{PickerLayout, PickerMode, cut, paint, style, width};
use crate::history::{sanitize, word_wrapped};
use crate::theme::surfaces::{ViewStyles, tokens as t};

fn wrapped(view: &PickerView<'_>, width: u16, views: Option<&ViewStyles>) -> Vec<Line<'static>> {
    let details = view.browse.map(|browse| browse.details).unwrap_or(&[]);
    details
        .iter()
        .flat_map(|text| {
            let token = if text.starts_with("Warning:") {
                t::PICKER_DETAIL_WARNING
            } else {
                t::PICKER_DETAIL_VALUE
            };
            let value = style(views, token, Style::default());
            let label_style = style(views, t::PICKER_DETAIL_LABEL, Style::default().dim());
            let help = style(views, t::PICKER_DETAIL_HELP, Style::default().dim());
            let label = text
                .split_once(": ")
                .map(|(label, _)| sanitize(label).into_owned() + ":");
            word_wrapped(text, width)
                .into_iter()
                .enumerate()
                .map(move |(i, line)| {
                    if i == 0
                        && token != t::PICKER_DETAIL_WARNING
                        && let Some(label) = &label
                        && let Some(rest) = line.strip_prefix(label)
                    {
                        return Line::from(vec![
                            Span::styled(label.clone(), label_style),
                            Span::styled(rest.to_owned(), value),
                        ]);
                    }
                    Line::styled(line, if label.is_none() { help } else { value })
                })
        })
        .collect()
}

/// Count wrapped full-details rows for a terminal width, including the frame border.
pub fn details_line_count(view: &PickerView<'_>, width: u16) -> usize {
    if width <= 2 {
        return 0;
    }
    wrapped(view, width - 2, None).len()
}

pub(super) fn draw_wrapped(text: &str, area: Rect, buffer: &mut Buffer, style: Style) {
    if area.is_empty() {
        return;
    }
    for (offset, line) in word_wrapped(text, area.width)
        .into_iter()
        .take(usize::from(area.height))
        .enumerate()
    {
        buffer.set_line(
            area.x,
            area.y + u16::try_from(offset).unwrap_or(0),
            &Line::styled(line, style),
            area.width,
        );
    }
}

pub(super) fn draw_summary(
    view: &PickerView<'_>,
    layout: &PickerLayout,
    buffer: &mut Buffer,
    views: Option<&ViewStyles>,
) {
    let warning = style(views, t::PICKER_DETAIL_WARNING, Style::default());
    if layout.mode == PickerMode::Compact {
        if let Some(banner) = view.banner {
            draw_wrapped(banner, layout.details, buffer, warning);
        }
        return;
    }
    let mut area = layout.details;
    if area.is_empty() || view.rows.is_empty() {
        return;
    }
    let divider = style(views, t::PICKER_DETAIL_DIVIDER, Style::default().dim());
    let title_style = style(views, t::PICKER_DETAIL_LABEL, Style::default()).bold();
    let label = view
        .selected
        .and_then(|index| view.rows.get(index))
        .filter(|row| row.key.is_some())
        .map(|row| row.text.trim_start_matches(' '));
    if layout.mode == PickerMode::Side {
        for y in area.y..area.bottom() {
            paint(
                views.map_or("│", |v| v.divider_symbol("│")),
                Rect::new(area.x, y, 1, 1),
                divider,
                buffer,
            );
        }
        area.x += 1;
        area.width = area.width.saturating_sub(1);
        buffer.set_style(
            area,
            style(views, t::PICKER_DETAIL_BACKGROUND, Style::default()),
        );
        paint("Details", area, title_style, buffer);
        if let Some(label) = label {
            area.y += 1;
            area.height = area.height.saturating_sub(1);
            let identity = style(views, t::PICKER_TITLE, Style::default().bold());
            paint(label, area, identity, buffer);
        }
    } else {
        buffer.set_style(
            area,
            style(views, t::PICKER_DETAIL_BACKGROUND, Style::default()),
        );
        let heading = label.map_or_else(|| "Details".into(), |label| format!("Details · {label}"));
        let heading = cut(&heading, area.width);
        let remaining = area.width.saturating_sub(width(&heading) + 1);
        let rule = views
            .map_or("─", |v| v.divider_symbol("─"))
            .repeat(usize::from(remaining));
        let line = Line::from(vec![
            Span::styled(heading, title_style),
            Span::raw(" "),
            Span::styled(rule, divider),
        ]);
        buffer.set_line(area.x, area.y, &line, area.width);
    }
    area.y += 1;
    area.height = area.height.saturating_sub(1);
    if let Some(banner) = view.banner.filter(|text| !text.is_empty()) {
        let rows = word_wrapped(banner, area.width);
        let height = u16::try_from(rows.len())
            .unwrap_or(u16::MAX)
            .min(area.height);
        draw_wrapped(
            banner,
            Rect::new(area.x, area.y, area.width, height),
            buffer,
            warning,
        );
        area.y += height;
        area.height -= height;
    }
    // Full details can be opened when the inline summary runs out of rows.
    for (offset, line) in wrapped(view, area.width, views)
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

pub(super) fn draw_full(
    view: &PickerView<'_>,
    area: Rect,
    buffer: &mut Buffer,
    views: Option<&ViewStyles>,
) {
    if area.width < 3 || area.height < 3 {
        return;
    }
    let inner = Rect::new(area.x + 1, area.y + 1, area.width - 2, area.height - 2);
    for y in inner.y..inner.bottom() {
        for x in inner.x..inner.right() {
            buffer[(x, y)].reset();
        }
    }
    buffer.set_style(
        inner,
        style(views, t::PICKER_DETAIL_BACKGROUND, Style::default()),
    );
    let rows = wrapped(view, inner.width, views);
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
        style(views, t::PICKER_FOOTER, Style::default()),
        buffer,
    );
}
