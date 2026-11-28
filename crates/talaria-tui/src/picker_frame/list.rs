//! List painting for the shared picker frame.

use std::ops::Range;

use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::Style,
    text::{Line, Span},
};
use talaria_core::models::{PickerView, RowKey};

use super::{cut, paint, selection, style, width};
use crate::theme::surfaces::{ViewStyles, tokens as t};

pub(super) fn visible_rows(view: &PickerView<'_>, area: Rect) -> Range<usize> {
    let capacity = usize::from(area.height.saturating_sub(1));
    let top = view
        .selected
        .filter(|&i| i < view.rows.len())
        .map_or(0, |i| {
            // Keep room below a heading so expanding it immediately reveals its children.
            let preview = if matches!(view.rows[i].key, Some(RowKey::Provider { .. })) {
                2.min(capacity.saturating_sub(1))
            } else {
                0
            };
            (i + 1 + preview)
                .min(view.rows.len())
                .saturating_sub(capacity)
        });
    top..(top + capacity).min(view.rows.len())
}

pub(super) fn draw(
    view: &PickerView<'_>,
    area: Rect,
    buffer: &mut Buffer,
    views: Option<&ViewStyles>,
) {
    if area.height < 2 {
        return;
    }
    let visible = visible_rows(view, area);
    let top = visible.start;
    let last = visible.end;
    for offset in visible {
        let row = &view.rows[offset];
        let chosen = view.selected == Some(offset) && row.key.is_some();
        let row_style = if chosen {
            selection(views)
        } else if row.header {
            style(views, t::PICKER_GROUP_HEADER, Style::default().bold())
        } else if row.dim {
            style(views, t::PICKER_ROW_DISABLED, Style::default().dim())
        } else if row.key.is_none() {
            style(views, t::PICKER_EMPTY, Style::default().dim())
        } else {
            style(views, t::PICKER_ROW_TEXT, Style::default())
        };
        let y = area.y + u16::try_from(offset - top).unwrap_or(0);
        if chosen {
            buffer.set_style(Rect::new(area.x, y, area.width, 1), row_style);
        }
        let prefix = if let Some(RowKey::Provider { expanded, .. }) = row.key {
            match (chosen, expanded) {
                (true, true) => "> ▾ ",
                (true, false) => "> ▸ ",
                (false, true) => "  ▾ ",
                (false, false) => "  ▸ ",
            }
        } else if chosen {
            "> "
        } else if row.key.is_some() {
            "  "
        } else {
            ""
        };
        let mut spans = vec![Span::raw(prefix)];
        // The catalogue reserves one column each for the star and current-route markers.
        let label = if matches!(
            row.key,
            Some(RowKey::Model { .. } | RowKey::Favourite { .. })
        ) {
            row.text.as_str()
        } else {
            row.text.trim_start_matches(' ')
        };
        if let Some(badge) = row.badges.first().filter(|badge| !badge.is_empty()) {
            let badge_text = cut(badge, area.width / 2);
            let badge_width = width(&badge_text);
            let label = cut(
                label,
                area.width.saturating_sub(width(prefix) + badge_width + 2),
            );
            let padding = area
                .width
                .saturating_sub(width(prefix) + width(&label) + badge_width);
            let badge_style = if chosen {
                row_style
            } else {
                style(views, badge_token(badge), Style::default().dim())
            };
            spans.extend([
                Span::raw(label),
                Span::raw(" ".repeat(usize::from(padding))),
                Span::styled(badge_text, badge_style),
            ]);
        } else {
            spans.push(Span::raw(cut(
                label,
                area.width.saturating_sub(width(prefix)),
            )));
        }
        buffer.set_line(area.x, y, &Line::from(spans).style(row_style), area.width);
    }
    let first = if view.rows.is_empty() { 0 } else { top + 1 };
    if top > 0 || last < view.rows.len() {
        let mut indicator = format!("Rows {first}–{last} of {}", view.rows.len());
        if let Some(selected) = view.selected.filter(|i| top > 0 && (top..last).contains(i))
            && let Some(RowKey::Model { provider, .. }) = &view.rows[selected].key
            && !view.rows[top..selected].iter().any(|row| row.header)
        {
            indicator.push_str(" · ");
            indicator.push_str(provider);
        }
        if top > 0 {
            indicator.push_str(" · More above");
        }
        if last < view.rows.len() {
            indicator.push_str(" · More below");
        }
        paint(
            &indicator,
            Rect::new(area.x, area.bottom() - 1, area.width, 1),
            style(views, t::PICKER_SCROLLBAR, Style::default().dim()),
            buffer,
        );
    }
    if view.rows.is_empty()
        && let Some(banner) = view.banner
    {
        let recovery = view.browse.map(|browse| {
            format!(
                "View: {} · Ctrl+U clears · Tab changes view · Esc clears/closes",
                browse.scope.label()
            )
        });
        let message = if !view.filter.is_empty() {
            recovery.map_or_else(
                || banner.to_owned(),
                |recovery| format!("{banner}\n{recovery}"),
            )
        } else {
            banner.to_owned()
        };
        super::details::draw_wrapped(
            &message,
            Rect::new(area.x, area.y, area.width, area.height - 1),
            buffer,
            style(views, t::PICKER_EMPTY, Style::default().dim()),
        );
    }
}

fn badge_token(badge: &str) -> crate::theme::surfaces::Token {
    match badge {
        "current" | "listed ready" => t::PICKER_BADGE_CURRENT,
        "unavailable" => t::PICKER_BADGE_UNAVAILABLE,
        "unknown" | "checking" | "needs setup" | "not listed" => t::PICKER_BADGE_UNKNOWN,
        text if text.starts_with("rate-limited") => t::PICKER_BADGE_UNKNOWN,
        _ => t::PICKER_BADGE,
    }
}
