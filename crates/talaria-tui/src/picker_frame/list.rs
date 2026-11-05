//! List painting for the shared picker frame.

use ratatui::{buffer::Buffer, layout::Rect, style::Style};
use talaria_core::models::PickerView;

use super::{cut, paint, width};

pub(super) fn draw(view: &PickerView<'_>, area: Rect, buffer: &mut Buffer) {
    let capacity = usize::from(area.height.saturating_sub(1));
    if capacity == 0 {
        return;
    }
    let selected = view.selected.filter(|&i| i < view.rows.len());
    let top = selected.map_or(0, |i| (i + 1).saturating_sub(capacity));
    for (offset, row) in view.rows.iter().enumerate().skip(top).take(capacity) {
        let chosen = selected == Some(offset) && row.key.is_some() && !row.header;
        let mut style = Style::default();
        if row.dim {
            style = style.dim();
        }
        if row.header {
            style = style.bold();
        }
        if chosen {
            style = style.reversed();
        }
        let label = if row.header || row.key.is_none() {
            row.text.clone()
        } else {
            format!("{}{}", if chosen { "> " } else { "  " }, row.text)
        };
        let badge = row
            .badges
            .first()
            .map(|badge| cut(badge, area.width))
            .unwrap_or_default();
        let text = if badge.is_empty() {
            cut(&label, area.width)
        } else {
            let label = cut(
                &label,
                area.width.saturating_sub(width(&badge).saturating_add(2)),
            );
            let padding = area
                .width
                .saturating_sub(width(&label).saturating_add(width(&badge)));
            format!("{label}{}{badge}", " ".repeat(usize::from(padding)))
        };
        paint(
            &text,
            Rect::new(
                area.x,
                area.y + u16::try_from(offset - top).unwrap_or(0),
                area.width,
                1,
            ),
            style,
            buffer,
        );
    }
    let first = if view.rows.is_empty() { 0 } else { top + 1 };
    let last = (top + capacity).min(view.rows.len());
    let mut indicator = format!("Rows {first}–{last} of {}", view.rows.len());
    if top > 0 {
        indicator.push_str(" · More above");
    }
    if last < view.rows.len() {
        indicator.push_str(" · More below");
    }
    paint(
        &indicator,
        Rect::new(area.x, area.bottom() - 1, area.width, 1),
        Style::default().dim(),
        buffer,
    );
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
        );
    }
}
