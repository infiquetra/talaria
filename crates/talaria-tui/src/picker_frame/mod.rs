//! Shared, terminal-independent picker surface.

mod details;
pub mod layout;
mod list;

use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::Style,
    widgets::{Block, Widget},
};
use talaria_core::models::PickerView;
use unicode_segmentation::UnicodeSegmentation;

use crate::history::{grapheme_width, sanitize};

pub use details::details_line_count;
pub use layout::{PickerLayout, PickerMode, list_height};

/// Draw a picker inside the supplied area.
pub fn draw_picker_frame(view: &PickerView<'_>, area: Rect, buffer: &mut Buffer) {
    draw_frame(view, None, area, buffer);
}

/// Labels supplied by a picker that shares the model picker's regions.
pub struct FrameLabels<'a> {
    pub scope: &'a str,
    pub scopes: &'a [&'a str],
    pub count: &'a str,
}

/// Draw the shared regions with caller-provided scope and count labels.
pub fn draw_picker_frame_with_labels(
    view: &PickerView<'_>,
    labels: FrameLabels<'_>,
    area: Rect,
    buffer: &mut Buffer,
) {
    draw_frame(view, Some(labels), area, buffer);
}

fn draw_frame(
    view: &PickerView<'_>,
    labels: Option<FrameLabels<'_>>,
    area: Rect,
    buffer: &mut Buffer,
) {
    let area = area.intersection(buffer.area);
    if area.is_empty() {
        return;
    }
    for y in area.y..area.bottom() {
        for x in area.x..area.right() {
            buffer[(x, y)].reset();
        }
    }
    let layout = PickerLayout::new(area);
    if layout.mode == PickerMode::SizeMessage {
        paint(
            "Picker needs at least 40×10 · Esc close",
            area,
            Style::default(),
            buffer,
        );
        return;
    }
    let title = if view.refreshing && !view.title.contains("refreshing") {
        format!("{} · refreshing…", view.title)
    } else {
        view.title.to_owned()
    };
    if layout.mode == PickerMode::Compact {
        let scope = labels
            .as_ref()
            .map(|labels| labels.scope)
            .or_else(|| view.browse.map(|browse| browse.scope.label()));
        let title = scope.map_or(title.clone(), |scope| format!("[{scope}] {title}"));
        paint(
            &title,
            Rect::new(area.x, area.y, area.width, 1),
            Style::default().bold(),
            buffer,
        );
    } else {
        Block::bordered()
            .title(sanitize(&title).into_owned())
            .render(area, buffer);
    }
    if view.browse.is_some_and(|browse| browse.details_open) {
        details::draw_full(view, area, buffer);
        return;
    }
    let filter = if view.filter.is_empty() {
        "> type to filter".into()
    } else {
        format!("> {}", view.filter)
    };
    paint(&filter, layout.filter, Style::default(), buffer);
    if !layout.chips.is_empty() {
        let scope = labels
            .as_ref()
            .map(|labels| labels.scope)
            .unwrap_or_else(|| {
                view.browse
                    .map(|browse| browse.scope.label())
                    .unwrap_or("All")
            });
        let scopes = labels.as_ref().map(|labels| labels.scopes).unwrap_or(&[
            "All",
            "Starred",
            "Configured",
            "Set up",
        ]);
        let chips = scopes
            .iter()
            .map(|&label| {
                if label == scope {
                    format!("[{label}]")
                } else {
                    label.into()
                }
            })
            .collect::<Vec<String>>()
            .join(" ");
        paint(&chips, layout.chips, Style::default(), buffer);
        if let Some(labels) = &labels {
            paint(labels.count, layout.count, Style::default().dim(), buffer);
        } else if let Some(browse) = view.browse {
            paint(
                &format!(
                    "{} models · {} providers",
                    browse.counts.models, browse.counts.providers
                ),
                layout.count,
                Style::default().dim(),
                buffer,
            );
        }
    }
    if let Some(labels) = &labels
        && view.rows.is_empty()
        && !view.filter.is_empty()
        && let Some(banner) = view.banner
    {
        let message = format!(
            "{banner}\nView: {} · Ctrl+U clears · Tab changes view · Esc clears/closes",
            labels.scope
        );
        let mut list_view = *view;
        list_view.filter = "";
        list_view.banner = Some(&message);
        list::draw(&list_view, layout.list, buffer);
    } else {
        list::draw(view, layout.list, buffer);
    }
    details::draw_summary(view, &layout, buffer);
    let (first_hint, second_hint) = view
        .hints
        .split_once('\n')
        .map_or((view.hints, None), |(first, second)| (first, Some(second)));
    paint(
        first_hint,
        Rect::new(layout.footer.x, layout.footer.y, layout.footer.width, 1),
        Style::default(),
        buffer,
    );
    let effort = if let Some(second) = second_hint {
        second.to_owned()
    } else {
        format!("{} · F2/Ctrl+E details", view.effort)
    };
    paint(
        &effort,
        Rect::new(layout.footer.x, layout.footer.y + 1, layout.footer.width, 1),
        Style::default(),
        buffer,
    );
}

fn width(text: &str) -> u16 {
    text.graphemes(true)
        .fold(0_u16, |total, g| total.saturating_add(grapheme_width(g)))
}

fn cut(text: &str, limit: u16) -> String {
    let text = sanitize(text);
    if width(&text) <= limit {
        return text.into_owned();
    }
    if limit == 0 {
        return String::new();
    }
    let mut result = String::new();
    let mut used = 0_u16;
    for grapheme in text.graphemes(true) {
        let next = used.saturating_add(grapheme_width(grapheme));
        if next > limit - 1 {
            break;
        }
        result.push_str(grapheme);
        used = next;
    }
    result.push('…');
    result
}

fn paint(text: &str, area: Rect, style: Style, buffer: &mut Buffer) {
    let area = area.intersection(buffer.area);
    if !area.is_empty() {
        buffer.set_stringn(
            area.x,
            area.y,
            cut(text, area.width),
            usize::from(area.width),
            style,
        );
    }
}
