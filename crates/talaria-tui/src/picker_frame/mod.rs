//! Shared, terminal-independent picker surface.

mod details;
pub mod layout;
mod list;

use ratatui::{
    buffer::Buffer,
    layout::{Position, Rect},
    style::Style,
    text::{Line, Span},
    widgets::{Block, Widget},
};
use talaria_core::models::PickerView;
use unicode_segmentation::UnicodeSegmentation;

use crate::history::{grapheme_width, sanitize, word_wrapped};
use crate::theme::surfaces::{Token, ViewStyles, tokens as t};

pub use details::details_line_count;
pub use layout::{PickerLayout, PickerMode, list_height};

/// Resolve a terminal coordinate to the selectable row drawn at that position.
pub fn hit_test(view: &PickerView<'_>, area: Rect, column: u16, row: u16) -> Option<usize> {
    if view.browse.is_some_and(|browse| browse.details_open) {
        return None;
    }
    let list = browse_layout(view, area).list;
    if !list.contains(Position::new(column, row)) {
        return None;
    }
    let visible = list::visible_rows(view, list);
    let index = visible.start + usize::from(row - list.y);
    (visible.contains(&index) && view.rows[index].key.is_some()).then_some(index)
}

/// Draw a picker inside the supplied area.
pub fn draw_picker_frame(view: &PickerView<'_>, area: Rect, buffer: &mut Buffer) {
    draw_frame(view, None, area, buffer, None, t::PICKER_TITLE);
}

/// Draw a picker with resolved view styles.
pub fn draw_picker_frame_with_theme(
    view: &PickerView<'_>,
    area: Rect,
    buffer: &mut Buffer,
    views: &ViewStyles,
) {
    draw_frame(view, None, area, buffer, Some(views), t::PICKER_TITLE);
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
    draw_frame(view, Some(labels), area, buffer, None, t::PICKER_TITLE);
}

/// Draw shared picker regions with caller labels and a provider title.
pub fn draw_picker_frame_with_labels_and_theme(
    view: &PickerView<'_>,
    labels: FrameLabels<'_>,
    area: Rect,
    buffer: &mut Buffer,
    views: &ViewStyles,
) {
    draw_frame(
        view,
        Some(labels),
        area,
        buffer,
        Some(views),
        t::PROVIDERS_TITLE,
    );
}

fn draw_frame(
    view: &PickerView<'_>,
    labels: Option<FrameLabels<'_>>,
    area: Rect,
    buffer: &mut Buffer,
    views: Option<&ViewStyles>,
    title_token: Token,
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
    let layout = browse_layout(view, area);
    if layout.mode == PickerMode::SizeMessage {
        paint(
            "Picker needs at least 40×10 · Esc close",
            area,
            style(views, t::PICKER_EMPTY, Style::default()),
            buffer,
        );
        if let Some(v) = views {
            v.finish(area, buffer, t::PICKER_BACKGROUND, true);
        }
        return;
    }
    let title = if view.browse.is_some_and(|browse| browse.details_open)
        && let Some(row) = view.selected.and_then(|index| view.rows.get(index))
    {
        format!("Details · {}", row.text.trim_start_matches(' '))
    } else if view.refreshing && !view.title.contains("refreshing") {
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
            views.map_or(Style::default().bold(), |v| {
                v.style(title_token, Style::default().bold())
            }),
            buffer,
        );
    } else {
        let title = cut(&title, area.width - 4);
        let mut block = Block::bordered().title(format!(" {title} "));
        if let Some(v) = views {
            block = block
                .border_set(v.border_set())
                .border_style(v.style(t::PICKER_BORDER, Style::default()))
                .title_style(v.style(title_token, Style::default().bold()));
        }
        block.render(area, buffer);
    }
    if view.browse.is_some_and(|browse| browse.details_open) {
        details::draw_full(view, area, buffer, views);
        if let Some(v) = views {
            v.finish(area, buffer, t::PICKER_BACKGROUND, true);
        }
        return;
    }
    buffer.set_style(
        layout.filter,
        style(views, t::PICKER_FILTER_BACKGROUND, Style::default()),
    );
    let (filter, filter_style) = if view.filter.is_empty() {
        (
            "type to filter",
            style(views, t::PICKER_FILTER_PLACEHOLDER, Style::default().dim()),
        )
    } else {
        (
            view.filter,
            style(views, t::PICKER_FILTER_TEXT, Style::default()),
        )
    };
    buffer.set_line(
        layout.filter.x,
        layout.filter.y,
        &Line::from(vec![
            Span::styled(
                "> ",
                style(views, t::PICKER_FILTER_TEXT, Style::default().bold()),
            ),
            Span::styled(
                cut(filter, layout.filter.width.saturating_sub(2)),
                filter_style,
            ),
        ]),
        layout.filter.width,
    );
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
            "Needs setup",
        ]);
        let mut chips = vec![Span::styled(
            "View   ",
            style(views, t::PICKER_ROW_DESCRIPTION, Style::default().dim()),
        )];
        for (index, &label) in scopes.iter().enumerate() {
            if index > 0 {
                chips.push(Span::raw("   "));
            }
            chips.push(if label == scope {
                Span::styled(
                    format!("[{label}]"),
                    style(views, t::PICKER_GROUP_TAB_SELECTED, Style::default().bold()),
                )
            } else {
                Span::styled(
                    label,
                    style(views, t::PICKER_GROUP_TAB, Style::default().dim()),
                )
            });
        }
        buffer.set_line(
            layout.chips.x,
            layout.chips.y,
            &Line::from(chips),
            layout.chips.width,
        );
        let count_style = style(views, t::PICKER_ROW_DESCRIPTION, Style::default().dim());
        if let Some(labels) = &labels {
            paint(labels.count, layout.count, count_style, buffer);
        } else if let Some(browse) = view.browse {
            let current = browse
                .current
                .map(|route| format!(" · Current: {route}"))
                .unwrap_or_default();
            let counts = browse.counts;
            let plural = |count| if count == 1 { "" } else { "s" };
            paint(
                &format!(
                    "{} model{} · {} provider{}{current}",
                    counts.models,
                    plural(counts.models),
                    counts.providers,
                    plural(counts.providers)
                ),
                layout.count,
                count_style,
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
        list::draw(&list_view, layout.list, buffer, views);
    } else {
        list::draw(view, layout.list, buffer, views);
    }
    details::draw_summary(view, &layout, buffer, views);
    let (first_hint, second_hint) = footer(view, labels.is_some(), layout.footer.width);
    let footer_style = style(views, t::PICKER_FOOTER, Style::default().dim());
    paint(
        &first_hint,
        Rect::new(layout.footer.x, layout.footer.y, layout.footer.width, 1),
        footer_style,
        buffer,
    );
    paint(
        &second_hint,
        Rect::new(layout.footer.x, layout.footer.y + 1, layout.footer.width, 1),
        footer_style,
        buffer,
    );
    if let Some(v) = views {
        v.finish(area, buffer, t::PICKER_BACKGROUND, true);
    }
}

fn browse_layout(view: &PickerView<'_>, area: Rect) -> PickerLayout {
    let mut layout = PickerLayout::new(area);
    if layout.mode == PickerMode::Compact
        && !view.rows.is_empty()
        && let Some(banner) = view.banner.filter(|text| !text.is_empty())
    {
        let banner_height = u16::try_from(word_wrapped(banner, layout.list.width).len())
            .unwrap_or(u16::MAX)
            .min(layout.list.height.saturating_sub(2))
            .min(3);
        layout.list.height -= banner_height;
        layout.details = Rect::new(
            layout.list.x,
            layout.list.bottom(),
            layout.list.width,
            banner_height,
        );
    }
    layout
}

fn footer(view: &PickerView<'_>, provider: bool, width: u16) -> (String, String) {
    let (first, second) = view
        .hints
        .split_once('\n')
        .map_or((view.hints, None), |(first, second)| (first, Some(second)));
    // The live model picker supplies a dynamic effort value and the complete key list.
    // Keep the actions needed to switch, inspect and leave visible in a split pane.
    if !provider
        && width < 78
        && let Some(second) = second
    {
        if let Some(talaria_core::models::RowKey::Provider { expanded, .. }) = view
            .selected
            .and_then(|i| view.rows.get(i))
            .and_then(|row| row.key.as_ref())
        {
            let action = if *expanded { "fold" } else { "expand" };
            return (
                format!("Enter {action} · ↑↓ move · Esc"),
                "Tab view · Type to search · F2 details".into(),
            );
        }
        let actions = if width >= 54 && first.contains("Shift+← fold") {
            "Enter switch · Shift+↑↓ group · Shift+← fold · Esc"
        } else if width >= 54 {
            "Enter switch · Shift+↑↓ group · ←→ effort · Esc"
        } else {
            "Enter switch · Shift+↑↓ group · Esc"
        };
        let options = if width >= 54 {
            format!("{} · Tab view · Ctrl+F star · F2 details", view.effort)
        } else {
            "Tab view · Ctrl+F star · F2 details".to_owned()
        };
        if second.starts_with(view.effort) && !view.effort.is_empty() {
            return (actions.into(), options);
        }
    }
    (
        first.into(),
        second.map_or_else(
            || match (provider, view.effort.is_empty()) {
                (true, _) => "F2/Ctrl+E details · Esc close".to_owned(),
                (_, true) => "F2/Ctrl+E details".to_owned(),
                _ => format!("{} · F2/Ctrl+E details", view.effort),
            },
            str::to_owned,
        ),
    )
}

fn style(views: Option<&ViewStyles>, token: Token, fallback: Style) -> Style {
    views.map_or(fallback, |views| views.style(token, fallback))
}

fn selection(views: Option<&ViewStyles>) -> Style {
    let selected = style(views, t::PICKER_ROW_SELECTED, Style::default().reversed());
    if views.is_some_and(|views| !views.color()) {
        selected.reversed()
    } else {
        selected
    }
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
