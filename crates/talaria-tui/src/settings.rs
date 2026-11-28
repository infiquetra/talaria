//! Settings projection and terminal key translation.
use crate::{
    history::{sanitize, word_wrapped, wrap_lines},
    keys::{Key, KeyCode, KeyContext, KeyKind, KeyMap},
    theme::surfaces::{ViewStyles, tokens as t},
};
use ratatui::{
    Frame,
    buffer::Buffer,
    layout::{Position, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Clear, Paragraph, Widget, Wrap},
};
pub use talaria_core::settings::SettingsAction;
use talaria_core::settings::{
    EditKey, ReviewRow, Screen, SettingsKey, SettingsTarget, SettingsView, contains_secret_key,
    is_secret_key,
};
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

const MIN_WIDTH: u16 = 40;
const MIN_HEIGHT: u16 = 12;

pub fn render(frame: &mut Frame<'_>, area: Rect, view: &SettingsView) {
    draw_settings(view, area, frame.buffer_mut());
}
fn at(area: Rect, row: u16, height: u16) -> Rect {
    Rect::new(
        area.x,
        area.y.saturating_add(row),
        area.width,
        height.min(area.height.saturating_sub(row)),
    )
}
fn line(text: &str, area: Rect, buf: &mut Buffer) {
    Paragraph::new(sanitize(text).into_owned()).render(area, buf);
}
fn lines(text: Vec<String>, area: Rect, buf: &mut Buffer) {
    let text: Vec<Line<'static>> = text
        .into_iter()
        .map(|s| Line::from(sanitize(&s).into_owned()))
        .collect();
    Paragraph::new(text)
        .wrap(Wrap { trim: false })
        .render(area, buf);
}

fn column(text: &str, width: usize) -> String {
    let clean = sanitize(text);
    let mut output = String::new();
    let mut used = 0;
    let truncated = Line::from(clean.as_ref()).width() > width;
    let budget = width.saturating_sub(usize::from(truncated));
    for grapheme in clean.graphemes(true) {
        let columns = UnicodeWidthStr::width(grapheme);
        if used + columns > budget {
            break;
        }
        output.push_str(grapheme);
        used += columns;
    }
    if truncated && width > 0 {
        output.push('…');
        used += 1;
    }
    output.push_str(&" ".repeat(width.saturating_sub(used)));
    output
}

/// Draw a settings snapshot without input or output.
pub fn draw_settings(view: &SettingsView, area: Rect, buf: &mut Buffer) {
    draw_settings_with_theme(view, area, buf, &ViewStyles::default());
}

/// Draw the settings overlay with the active view styles.
pub fn draw_settings_with_theme(
    view: &SettingsView,
    area: Rect,
    buf: &mut Buffer,
    views: &ViewStyles,
) {
    Clear.render(area, buf);
    views.finish(area, buf, t::LIST_BACKGROUND, true);
    if area.width < MIN_WIDTH || area.height < MIN_HEIGHT {
        line(
            "Settings needs 40 columns and 12 rows. Esc closes.",
            area,
            buf,
        );
        return;
    }
    if view.screen == Screen::Closed {
        return;
    }
    Paragraph::new(Line::from(vec![
        Span::styled(
            "Settings",
            views.style(t::LIST_HEADER, Style::default()).bold(),
        ),
        Span::styled(
            format!(" · profile {}", sanitize(&view.profile)),
            views.style(t::LIST_MUTED, Style::default()),
        ),
    ]))
    .render(at(area, 0, 1), buf);
    match view.screen {
        Screen::Loading => line(
            &format!("Loading settings for profile {}…", view.profile),
            at(area, 2, 1),
            buf,
        ),
        Screen::LoadError => lines(view.banner.clone(), at(area, 2, area.height - 2), buf),
        Screen::Reviewing | Screen::Checking | Screen::Applying | Screen::Verifying => {
            let banner_height = if view.banner.is_empty() {
                0
            } else {
                3.min(area.height.saturating_sub(4))
            };
            draw_review_scrolled(
                &view.review_rows,
                &format!(
                    "Review {} change(s) · profile {}",
                    view.review_rows.len(),
                    view.profile
                ),
                view.review_scroll,
                at(area, 0, area.height.saturating_sub(1 + banner_height)),
                buf,
            );
            views.finish(area, buf, t::LIST_BACKGROUND, true);
            buf.set_style(
                at(area, 0, 1),
                views.style(t::LIST_HEADER, Style::default()),
            );
            let footer = if view.screen == Screen::Reviewing {
                "↑ ↓ scroll · Enter apply · Esc back"
            } else {
                "Applying… (checking, saving, then reading back)"
            };
            line(
                &crate::footer::with_recovery(footer, area.width),
                at(area, area.height - 1, 1),
                buf,
            );
            if !view.banner.is_empty() {
                lines(
                    view.banner.clone(),
                    at(area, area.height - 1 - banner_height, banner_height),
                    buf,
                );
            }
        }
        Screen::Conflict => {
            scroll_text(
                conflict_text(view),
                view.review_scroll,
                at(area, 2, area.height - 3),
                buf,
            );
            line(
                &crate::footer::with_recovery(
                    "↑ ↓ scroll · o keep yours · t take theirs · Esc back",
                    area.width,
                ),
                at(area, area.height - 1, 1),
                buf,
            );
        }
        Screen::Help => {
            let mut text: Vec<String> = view
                .help
                .iter()
                .map(|(key, description)| format!("{key}  {description}"))
                .collect();
            text.push("* pending · ! not saved · ? not verified".into());
            text.push("Any key back".into());
            lines(text, at(area, 2, area.height - 2), buf);
        }
        _ => draw_browser(view, area, buf, views),
    }
}

struct BrowserLayout<'a> {
    category: &'a str,
    rail: u16,
    inspector_width: u16,
    fields: Rect,
    headings_y: u16,
    filter: Option<Rect>,
    top: u16,
    bottom: u16,
    detail_height: u16,
    start: usize,
    first_category: usize,
    compact_inspector: Option<Vec<Line<'static>>>,
}

impl<'a> BrowserLayout<'a> {
    fn new(view: &'a SettingsView, area: Rect, heading: Style, muted: Style) -> Self {
        let category = if view.category.is_empty() {
            view.rows
                .get(view.cursor)
                .and_then(|row| row.key.split_once('.').map(|(category, _)| category))
                .unwrap_or("general")
        } else {
            &view.category
        };
        let rail = if area.width < 96 { 0 } else { 18 };
        let inspector_width = if area.width >= 108
            && area.height >= 17
            && view.screen == Screen::Browsing
            && !view.rows.is_empty()
        {
            (area.width / 3).min(52)
        } else {
            0
        };
        let fields = Rect::new(
            area.x + rail,
            area.y,
            area.width - rail - inspector_width,
            area.height,
        );
        let headings_y = if rail == 0 { 2 } else { 1 };
        let filter = (!view.filter.is_empty()
            || view.text_focus && view.screen == Screen::Browsing)
            .then_some(at(fields, headings_y + 1, 1));
        let top = headings_y + 1 + u16::from(filter.is_some());
        let compact_inspector =
            (inspector_width == 0 && view.screen == Screen::Browsing && !view.rows.is_empty())
                .then(|| field_inspector_rows(view, category, area.width, true, heading, muted));
        let banners = view.banner.len().min(3) as u16;
        let detail_height = if inspector_width > 0 {
            0
        } else if let Some(rows) = &compact_inspector {
            let available = area
                .height
                .saturating_sub(top + 5 + banners)
                .min(area.height / 2);
            (rows.len().min(usize::from(available)) as u16).saturating_sub(1)
        } else if area.height >= 18 {
            4
        } else {
            2
        };
        let bottom = area.height.saturating_sub(detail_height + 3 + banners);
        let capacity = usize::from(bottom.saturating_sub(top));
        let start = view.cursor.saturating_sub(capacity.saturating_sub(1));
        let category_index = view
            .categories
            .iter()
            .position(|name| name == category)
            .unwrap_or(0);
        let first_category = category_index.saturating_sub(capacity.saturating_sub(1));
        Self {
            category,
            rail,
            inspector_width,
            fields,
            headings_y,
            filter,
            top,
            bottom,
            detail_height,
            start,
            first_category,
            compact_inspector,
        }
    }
}

fn compact_category(category: &str, width: u16) -> String {
    column(
        &category_label(category),
        usize::from(width.saturating_sub(18)),
    )
    .trim_end()
    .to_owned()
}

/// Settings takes pointer input only while the ordinary list is active.
pub fn pointer_enabled(view: &SettingsView, area: Rect) -> bool {
    view.screen == Screen::Browsing
        && !view.text_focus
        && area.width >= MIN_WIDTH
        && area.height >= MIN_HEIGHT
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum PointerMotion {
    Select,
    Up,
    Down,
}

/// Resolve a pointer event against the current browser geometry, including the compact inspector.
pub fn pointer_key(
    view: &SettingsView,
    area: Rect,
    column: u16,
    row: u16,
    motion: PointerMotion,
) -> Option<SettingsKey> {
    let pos = Position::new(column, row);
    if !pointer_enabled(view, area) || !area.contains(pos) {
        return None;
    }
    let layout = BrowserLayout::new(view, area, Style::default(), Style::default());
    if layout.filter.is_some_and(|filter| filter.contains(pos)) {
        return (motion == PointerMotion::Select)
            .then_some(SettingsKey::Action(SettingsAction::Filter));
    }
    let offset = row - area.y;
    if layout.rail == 0 && offset == 1 && view.filter.is_empty() {
        let label = compact_category(layout.category, area.width);
        let next = 3 + Line::raw(label).width();
        let x = usize::from(column - area.x);
        let action = match motion {
            PointerMotion::Up => SettingsAction::PrevCategory,
            PointerMotion::Down => SettingsAction::NextCategory,
            PointerMotion::Select if x == 0 => SettingsAction::PrevCategory,
            PointerMotion::Select if x == next => SettingsAction::NextCategory,
            PointerMotion::Select => return None,
        };
        return Some(SettingsKey::Action(action));
    }
    if !(layout.top..layout.bottom).contains(&offset) {
        return None;
    }
    let index = usize::from(offset - layout.top);
    if layout.rail > 0 && column < area.x + layout.rail - 2 && view.filter.is_empty() {
        return Some(match motion {
            PointerMotion::Select => SettingsKey::Select(SettingsTarget::Category(
                view.categories.get(layout.first_category + index)?.clone(),
            )),
            PointerMotion::Up => SettingsKey::Action(SettingsAction::PrevCategory),
            PointerMotion::Down => SettingsKey::Action(SettingsAction::NextCategory),
        });
    }
    if !layout.fields.contains(pos) {
        return None;
    }
    let index = layout.start + index;
    Some(match motion {
        PointerMotion::Up => SettingsKey::Action(SettingsAction::Up),
        PointerMotion::Down => SettingsKey::Action(SettingsAction::Down),
        PointerMotion::Select if !view.links.is_empty() => {
            SettingsKey::Select(SettingsTarget::Link(view.links.get(index)?.clone()))
        }
        PointerMotion::Select => {
            SettingsKey::Select(SettingsTarget::Field(view.rows.get(index)?.key.clone()))
        }
    })
}

fn draw_browser(view: &SettingsView, area: Rect, buf: &mut Buffer, views: &ViewStyles) {
    let muted = views.style(t::LIST_MUTED, Style::default());
    let heading = views.style(t::LIST_HEADER, Style::default()).bold();
    let layout = BrowserLayout::new(view, area, heading, muted);
    let BrowserLayout {
        category,
        rail,
        inspector_width,
        fields,
        headings_y,
        top,
        bottom,
        detail_height,
        start,
        compact_inspector,
        ..
    } = layout;
    let capacity = usize::from(bottom.saturating_sub(top));
    let origin_width = if fields.width >= 70 { 11 } else { 0 };
    let effect_width = if fields.width >= 120 { 23 } else { 0 };
    let name_space = fields.width - origin_width - effect_width - 5;
    let name_width = usize::from((name_space.saturating_mul(3)) / 5).min(44);
    let value_width =
        usize::from(fields.width - origin_width - effect_width).saturating_sub(name_width + 5);
    if rail == 0 {
        Paragraph::new(Line::from(vec![
            Span::styled("‹ ", muted),
            Span::styled(compact_category(category, area.width), heading),
            Span::styled(" ›  ← → category", muted),
        ]))
        .render(at(area, 1, 1), buf);
    } else {
        Paragraph::new("Categories ←→")
            .style(muted)
            .render(at(area, 1, 1), buf);
    }
    let headings = format!(
        "  {}  {} {}{}",
        column(
            if view.links.is_empty() {
                "Setting"
            } else {
                "Open"
            },
            name_width
        ),
        column(
            if view.links.is_empty() { "Value" } else { "" },
            value_width
        ),
        if origin_width > 0 && view.links.is_empty() {
            "Source     "
        } else {
            ""
        },
        if effect_width > 0 && view.links.is_empty() {
            "Effect"
        } else {
            ""
        }
    );
    Paragraph::new(headings)
        .style(heading)
        .render(at(fields, headings_y, 1), buf);
    if let Some(filter) = layout.filter {
        line("/ ", filter, buf);
        let query = Rect::new(filter.x + 2, filter.y, filter.width - 2, 1);
        if let Some(cursor) = view.filter_cursor {
            crate::editor::draw_with_theme(&view.filter, cursor, query, buf, views);
        } else {
            line(&column(&view.filter, usize::from(query.width)), query, buf);
        }
    }
    if rail > 0 {
        for (offset, name) in view
            .categories
            .iter()
            .skip(layout.first_category)
            .take(capacity)
            .enumerate()
        {
            let active = view.filter.is_empty() && name == category;
            Paragraph::new(format!(
                "{} {}",
                if active { "›" } else { " " },
                column(&category_label(name), usize::from(rail - 4)).trim_end()
            ))
            .style(if active { heading.underlined() } else { muted })
            .render(
                Rect::new(area.x, area.y + top + offset as u16, rail - 2, 1),
                buf,
            );
        }
        for y in area.y + 1..area.y + bottom {
            buf.set_string(
                area.x + rail - 2,
                y,
                views.divider_symbol("│"),
                views.style(t::LIST_DIVIDER, Style::default()),
            );
        }
    }
    for (offset, row) in view.rows.iter().enumerate().skip(start).take(capacity) {
        let y = top + (offset - start) as u16;
        let field_area = at(fields, y, 1);
        let value = if is_secret_key(&row.key) {
            "•••••••• (r reveals once)"
        } else {
            &row.value
        };
        let text = format!(
            "{} {} {}{} {}{}",
            if offset == view.cursor { "›" } else { " " },
            column(&setting_label(&row.key, category), name_width),
            column(&row.marker, 1),
            column(value, value_width),
            if origin_width > 0 {
                column(&row.origin, usize::from(origin_width))
            } else {
                String::new()
            },
            if effect_width > 0 {
                &row.effect_label
            } else {
                ""
            },
        );
        let style = if offset == view.cursor {
            views.style(
                t::LIST_SELECTED,
                Style::default().add_modifier(Modifier::BOLD),
            )
        } else {
            views.style(t::LIST_ROW, Style::default())
        };
        Paragraph::new(sanitize(&text).into_owned())
            .style(style)
            .render(field_area, buf);
    }
    if view.rows.is_empty() && view.links.is_empty() {
        line(
            &format!("No setting matches \"{}\".", view.filter),
            at(fields, top, 1),
            buf,
        );
    }
    for (i, link) in view.links.iter().enumerate().skip(start).take(capacity) {
        let region = at(fields, top + (i - start) as u16, 1);
        line(
            &format!("{} {link}  →", if i == view.cursor { ">" } else { " " }),
            region,
            buf,
        );
        if i == view.cursor {
            buf.set_style(region, views.style(t::LIST_SELECTED, Style::default()));
        }
    }
    if inspector_width > 0 {
        let x = fields.right();
        for y in area.y + 1..area.y + bottom {
            buf.set_string(
                x,
                y,
                views.divider_symbol("│"),
                views.style(t::LIST_DIVIDER, muted),
            );
        }
        let inner = Rect::new(x + 2, area.y + 1, inspector_width - 3, bottom - 1);
        let rows = field_inspector_rows(view, category, inner.width, false, heading, muted);
        draw_inspector_rows(&rows, inner, buf);
    } else if let Some(rows) = &compact_inspector {
        draw_inspector_rows(rows, at(area, bottom, detail_height + 1), buf);
    } else if let Some(row) = view.rows.get(view.cursor) {
        let value = if is_secret_key(&row.key) {
            "••••••••"
        } else {
            &row.value
        };
        let info = format!("{} · {}", row.origin, row.effect_label);
        let inspector = vec![
            Span::styled(sanitize(&row.key).into_owned(), heading),
            Span::styled(format!("   {}", sanitize(&info)), muted),
        ];
        Paragraph::new(Line::from(inspector)).render(at(area, bottom, 1), buf);
        let description = view
            .field_help
            .as_ref()
            .map(|help| help.description.as_str())
            .unwrap_or_else(|| {
                view.detail
                    .strip_prefix(&format!("{} · ", row.key))
                    .unwrap_or(&view.detail)
            });
        lines(
            vec![format!("Value  {value}"), description.to_owned()],
            at(area, bottom + 1, detail_height),
            buf,
        );
    } else {
        lines(
            vec![view.detail.clone()],
            at(area, bottom, detail_height),
            buf,
        );
    }
    let messages = view.banner.clone();
    if view.screen == Screen::Choosing {
        draw_chooser(view, at(area, top, bottom.saturating_sub(top)), buf, views);
    } else {
        let region = at(
            area,
            bottom + detail_height + 1,
            area.height.saturating_sub(bottom + detail_height + 2),
        );
        if let Some(editor) = &view.editor {
            lines(
                messages,
                at(region, 0, region.height.saturating_sub(1)),
                buf,
            );
            if region.height > 0 {
                let row = at(region, region.height - 1, 1);
                line("> ", Rect::new(row.x, row.y, row.width.min(2), 1), buf);
                let value = if editor.secret {
                    "•".repeat(editor.text.chars().count())
                } else {
                    editor.text.clone()
                };
                crate::editor::draw_with_theme(
                    &value,
                    editor.cursor,
                    Rect::new(
                        row.x + row.width.min(2),
                        row.y,
                        row.width.saturating_sub(2),
                        1,
                    ),
                    buf,
                    views,
                );
            }
        } else {
            lines(messages, region, buf);
        }
    }
    let footer = if view.filter_cursor.is_some() {
        "Enter or ↓ show matches · Esc clear filter"
    } else if view.screen == Screen::Editing
        && view.editor.as_ref().is_some_and(|editor| editor.secret)
    {
        "Enter accept · Alt+R reveal once · Esc cancel"
    } else if view.footer.is_empty() {
        "Enter edit · d default · u undo · r reveal · Ctrl+R reload · Esc close"
    } else {
        &view.footer
    };
    line(
        &crate::footer::with_recovery(footer, area.width),
        at(area, area.height - 1, 1),
        buf,
    );
    buf.set_style(
        at(area, area.height - 1, 1),
        views.style(t::LIST_MUTED, Style::default()),
    );
}

fn category_label(category: &str) -> String {
    match category {
        "tts" => "Speech output".into(),
        "stt" => "Speech input".into(),
        "mcp" => "Tool servers".into(),
        other => setting_label(&sanitize(other), ""),
    }
}

fn setting_label(key: &str, category: &str) -> String {
    let key = key
        .strip_prefix(category)
        .and_then(|rest| rest.strip_prefix('.'))
        .unwrap_or(key);
    let mut label = key.replace('_', " ").replace('.', " › ");
    if let Some(first) = label.get_mut(..1) {
        first.make_ascii_uppercase();
    }
    label
}

fn field_inspector_rows(
    view: &SettingsView,
    category: &str,
    width: u16,
    compact: bool,
    heading: Style,
    muted: Style,
) -> Vec<Line<'static>> {
    let Some(row) = view.rows.get(view.cursor) else {
        return Vec::new();
    };
    let mut content = Vec::new();
    if !compact {
        content.push(Line::styled("Selected setting", muted));
    }
    content.push(Line::styled(setting_label(&row.key, category), heading));
    if view.field_help.is_none() && view.detail.ends_with(" (hidden on the next key)") {
        content.push(Line::raw(view.detail.clone()));
        return wrap_inspector(content, width);
    }
    let value = if is_secret_key(&row.key) {
        "••••••••"
    } else {
        &row.value
    };
    let description = view.field_help.as_ref().map_or_else(
        || {
            view.detail
                .strip_prefix(&format!("{} · ", row.key))
                .unwrap_or(&view.detail)
        },
        |help| &help.description,
    );
    if !description.is_empty() {
        content.push(Line::raw(description.to_owned()));
    }
    if compact {
        content.push(Line::raw(format!("Value  {value}")));
    } else {
        content.push(Line::raw(""));
        content.push(Line::styled("Value", heading));
        content.push(Line::raw(value.to_owned()));
    }
    let source = if row.origin == "set" {
        "Profile setting"
    } else {
        &row.origin
    };
    if compact {
        content.push(Line::styled(
            format!("Key  {} · Source  {source}", row.key),
            muted,
        ));
    } else {
        content.push(Line::raw(""));
        content.push(Line::styled(format!("Key  {}", row.key), muted));
        content.push(Line::styled(format!("Source  {source}"), muted));
    }
    if !row.effect_label.is_empty() {
        content.push(Line::styled(row.effect_label.clone(), muted));
    }
    if let Some(help) = &view.field_help {
        if row.value != help.default_value && !is_secret_key(&row.key) {
            content.push(Line::styled(
                format!("Default  {}", help.default_value),
                muted,
            ));
        }
        if !help.note.is_empty() {
            if !compact {
                content.push(Line::raw(""));
            }
            content.push(Line::raw(help.note.clone()));
        }
    }
    wrap_inspector(content, width)
}

fn wrap_inspector(content: Vec<Line<'static>>, width: u16) -> Vec<Line<'static>> {
    content
        .into_iter()
        .flat_map(|line| {
            word_wrapped(&sanitize(&line.to_string()), width)
                .into_iter()
                .map(move |text| Line::styled(text, line.style))
        })
        .collect()
}

fn draw_inspector_rows(rows: &[Line<'_>], area: Rect, buf: &mut Buffer) {
    for (y, row) in (area.y..area.bottom()).zip(rows) {
        buf.set_line(area.x, y, row, area.width);
    }
}

/// The open chooser over the field list, above the detail line: the filter, then one row per
/// value with its marks and description, scrolled to keep the selection in view.
fn draw_chooser(view: &SettingsView, area: Rect, buf: &mut Buffer, views: &ViewStyles) {
    Clear.render(area, buf);
    views.finish(area, buf, t::LIST_BACKGROUND, true);
    let filter = if view.choice_filter.is_empty() {
        "type to filter".to_owned()
    } else {
        format!("filter: {}", view.choice_filter)
    };
    line(&filter, at(area, 0, 1), buf);
    let available = usize::from(area.height.saturating_sub(1));
    if view.choices.is_empty() {
        line(
            &format!("No match for \"{}\"", view.choice_filter),
            at(area, 1, 1),
            buf,
        );
        return;
    }
    let width = usize::from(area.width);
    let name_width = view
        .choices
        .iter()
        .filter(|choice| !choice.status)
        .map(|choice| Line::from(sanitize(&choice.name).as_ref()).width())
        .max()
        .unwrap_or(0)
        .min(width.saturating_sub(4) / 2);
    let first = view
        .choice_cursor
        .saturating_sub(available.saturating_sub(1));
    for (offset, (index, choice)) in view
        .choices
        .iter()
        .enumerate()
        .skip(first)
        .take(available)
        .enumerate()
    {
        let selected = index == view.choice_cursor;
        let text = if choice.status {
            // The list's loading or failure line takes the whole row.
            format!("{} {}", if selected { ">" } else { " " }, choice.name)
        } else {
            format!(
                "{} {}  {}",
                if selected { ">" } else { " " },
                column(&choice.name, name_width),
                column(
                    &choice.description,
                    width.saturating_sub(name_width + 4).max(1)
                )
            )
        };
        let style = if selected {
            views.style(
                t::LIST_SELECTED,
                Style::default().add_modifier(Modifier::BOLD),
            )
        } else {
            views.style(t::LIST_ROW, Style::default())
        };
        Paragraph::new(sanitize(&text).into_owned())
            .style(style)
            .render(at(area, 1 + offset as u16, 1), buf);
    }
}

/// Shared review component for formatted domain values.
pub fn draw_review(rows: &[ReviewRow], title: &str, area: Rect, buf: &mut Buffer) {
    draw_review_scrolled(rows, title, 0, area, buf);
}

/// Shared, scrollable review component used by settings and connectors.
pub fn draw_review_scrolled(
    rows: &[ReviewRow],
    title: &str,
    scroll: usize,
    area: Rect,
    buf: &mut Buffer,
) {
    Clear.render(area, buf);
    line(title, at(area, 0, 1), buf);
    scroll_text(
        review_text(rows),
        scroll,
        at(area, 2, area.height.saturating_sub(2)),
        buf,
    );
}

fn review_text(rows: &[ReviewRow]) -> Vec<String> {
    let mut text = Vec::new();
    for row in rows {
        let change = if is_secret_key(&row.key) {
            "•••• → •••• (changed)".into()
        } else {
            format!("{} → {}", row.old, row.new)
        };
        text.push(format!("{}  {change}  {}", row.key, row.effect_label));
        text.extend(row.warnings.iter().map(|warning| format!("  {warning}")));
    }
    text
}

fn conflict_text(view: &SettingsView) -> Vec<String> {
    let mut text = vec![format!(
        "{} setting(s) changed outside Talaria since you opened settings.",
        view.conflict_rows.len()
    )];
    for row in &view.conflict_rows {
        let secret = is_secret_key(&row.key)
            || contains_secret_key(&row.yours)
            || row.now.as_ref().is_some_and(contains_secret_key);
        let yours = if secret {
            "••••".into()
        } else {
            row.yours.to_string()
        };
        let now = if secret {
            "••••".into()
        } else {
            row.now
                .as_ref()
                .map_or_else(|| "(not set)".into(), ToString::to_string)
        };
        text.push(format!("{}  yours {yours}  now {now}", row.key));
    }
    text
}

fn wrapped_text(text: Vec<String>, width: u16) -> Vec<Line<'static>> {
    text.into_iter()
        .flat_map(|text| wrap_lines(vec![Line::from(sanitize(&text).into_owned())], width))
        .collect()
}

/// The last valid first row of a review component, given its full area.
pub fn review_scroll_limit(rows: &[ReviewRow], area: Rect) -> usize {
    wrapped_text(review_text(rows), area.width)
        .len()
        .saturating_sub(usize::from(area.height.saturating_sub(2)))
}

/// Resolve a review scroll key using the actual wrapped content and screen size.
pub fn review_scroll_target(view: &SettingsView, key: &SettingsKey, area: Rect) -> Option<usize> {
    let max = match view.screen {
        Screen::Reviewing => {
            let banner = if view.banner.is_empty() {
                0
            } else {
                3.min(area.height.saturating_sub(4))
            };
            review_scroll_limit(
                &view.review_rows,
                at(area, 0, area.height.saturating_sub(1 + banner)),
            )
        }
        Screen::Conflict => wrapped_text(conflict_text(view), area.width)
            .len()
            .saturating_sub(usize::from(area.height.saturating_sub(3))),
        _ => return None,
    };
    let current = view.review_scroll.min(max);
    Some(match key {
        SettingsKey::Edit(EditKey::Up) => current.saturating_sub(1),
        SettingsKey::Edit(EditKey::Down) => current.saturating_add(1).min(max),
        SettingsKey::Edit(EditKey::PageUp) => current.saturating_sub(5),
        SettingsKey::Edit(EditKey::PageDown) => current.saturating_add(5).min(max),
        SettingsKey::Edit(EditKey::Home) => 0,
        SettingsKey::Edit(EditKey::End) => max,
        _ => return None,
    })
}

fn scroll_text(text: Vec<String>, scroll: usize, area: Rect, buf: &mut Buffer) {
    if area.width == 0 || area.height == 0 {
        return;
    }
    let lines = wrapped_text(text, area.width);
    let max = lines.len().saturating_sub(usize::from(area.height));
    let start = scroll.min(max);
    for (offset, text) in lines
        .iter()
        .skip(start)
        .take(usize::from(area.height))
        .enumerate()
    {
        buf.set_line(area.x, area.y + offset as u16, text, area.width);
    }
    // The position is visible without taking space from the content or the confirmation footer.
    if lines.len() > usize::from(area.height) && area.y > 0 {
        let position = format!(
            "{}–{}/{}",
            start + 1,
            (start + usize::from(area.height)).min(lines.len()),
            lines.len()
        );
        let width = u16::try_from(Line::from(position.as_str()).width()).unwrap_or(u16::MAX);
        if width <= area.width {
            buf.set_string(area.right() - width, area.y - 1, position, Style::default());
        }
    }
}

/// Only browsing consults the key map; focused input uses the reducer's fixed keys.
pub fn route_settings(view: &SettingsView, map: &KeyMap, key: &Key) -> Option<SettingsKey> {
    if key.kind == KeyKind::Release {
        return None;
    }
    if view.screen == Screen::Editing
        && view.editor.as_ref().is_some_and(|editor| editor.secret)
        && key.code == KeyCode::Char('r')
        && key.mods.alt
        && !key.mods.ctrl
        && !key.mods.shift
        && !key.mods.super_key
        && !key.mods.other
    {
        return Some(SettingsKey::Action(
            talaria_core::settings::SettingsAction::Reveal,
        ));
    }
    if view.screen == Screen::Browsing && !view.text_focus {
        return map
            .resolve_settings(KeyContext::Settings, key)
            .map(SettingsKey::Action);
    }
    // A chooser takes the reload binding to ask for its fetched list again; a plain character
    // still types into the filter. `KeyChord::reaches_a_chooser` is the same test for the hint.
    if view.screen == Screen::Choosing
        && (!matches!(key.code, KeyCode::Char(_)) || key.mods.ctrl || key.mods.alt)
        && map.resolve_settings(KeyContext::Settings, key)
            == Some(talaria_core::settings::SettingsAction::Reload)
    {
        return Some(SettingsKey::Action(
            talaria_core::settings::SettingsAction::Reload,
        ));
    }
    if key.mods.ctrl && !key.mods.alt && !key.mods.super_key && !key.mods.other {
        return (key.code == KeyCode::Char('u')).then_some(SettingsKey::Edit(EditKey::ClearLine));
    }
    if key.mods.ctrl || key.mods.alt || key.mods.super_key || key.mods.other {
        return None;
    }
    let edit = match key.code {
        KeyCode::Char(c) => return Some(SettingsKey::Char(c)),
        KeyCode::Left => EditKey::Left,
        KeyCode::Right => EditKey::Right,
        KeyCode::Up => EditKey::Up,
        KeyCode::Down => EditKey::Down,
        KeyCode::PageUp => EditKey::PageUp,
        KeyCode::PageDown => EditKey::PageDown,
        KeyCode::Home => EditKey::Home,
        KeyCode::End => EditKey::End,
        KeyCode::Backspace => EditKey::Backspace,
        KeyCode::Delete => EditKey::Delete,
        KeyCode::Enter => EditKey::Enter,
        KeyCode::Esc => EditKey::Esc,
        _ => return None,
    };
    Some(SettingsKey::Edit(edit))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::keys::KeyChord;
    use talaria_core::settings::{SettingsAction, SettingsEditorView};

    fn editor(secret: bool) -> SettingsView {
        SettingsView {
            screen: Screen::Editing,
            text_focus: true,
            editor: Some(SettingsEditorView {
                text: String::new(),
                cursor: 0,
                secret,
            }),
            ..Default::default()
        }
    }

    fn chord(text: &str) -> Key {
        KeyChord::parse(text, KeyContext::Settings)
            .unwrap()
            .to_key()
    }

    #[test]
    fn secret_editor_keeps_plain_r_and_reveals_only_on_alt_r() {
        let view = editor(true);
        let map = KeyMap::defaults();
        assert!(matches!(
            route_settings(&view, &map, &chord("r")),
            Some(SettingsKey::Char('r'))
        ));
        assert!(matches!(
            route_settings(&view, &map, &chord("alt+r")),
            Some(SettingsKey::Action(SettingsAction::Reveal))
        ));
        for key in ["ctrl+r", "alt+shift+r", "ctrl+alt+r"] {
            assert!(route_settings(&view, &map, &chord(key)).is_none());
        }
        assert!(route_settings(&editor(false), &map, &chord("alt+r")).is_none());
        let browsing = SettingsView {
            screen: Screen::Browsing,
            ..Default::default()
        };
        assert!(route_settings(&browsing, &map, &chord("alt+r")).is_none());
    }

    #[test]
    fn settings_help_uses_overrides_and_marks_unbound_actions() {
        let entries = std::collections::BTreeMap::from([
            ("settings_down".to_owned(), vec!["j".to_owned()]),
            ("settings_reveal".to_owned(), Vec::new()),
        ]);
        let (map, notices) = KeyMap::from_entries(&entries);
        assert!(notices.is_empty());
        let help = map.settings_help();
        assert_eq!(help.len(), 16);
        assert_eq!(help[1], ("J".to_owned(), "Move down".to_owned()));
        assert_eq!(
            help[11],
            (
                "(unbound)".to_owned(),
                "Reveal selected secret once".to_owned()
            )
        );
    }
}
