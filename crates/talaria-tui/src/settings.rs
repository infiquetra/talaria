//! Settings projection and terminal key translation.
use crate::{
    history::{sanitize, wrap_lines},
    keys::{Key, KeyCode, KeyContext, KeyKind, KeyMap},
};
use ratatui::{
    Frame,
    buffer::Buffer,
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Clear, Paragraph, Widget, Wrap},
};
pub use talaria_core::settings::SettingsAction;
use talaria_core::settings::{
    EditKey, ReviewRow, Screen, SettingsKey, SettingsView, contains_secret_key, is_secret_key,
};

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
    for ch in clean.chars() {
        let columns = Span::raw(ch.to_string()).width();
        if used + columns > budget {
            break;
        }
        output.push(ch);
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
    Clear.render(area, buf);
    if area.width < 60 || area.height < 12 {
        line(
            "Make the terminal at least 60 by 12 to use settings.",
            area,
            buf,
        );
        return;
    }
    if view.screen == Screen::Closed {
        return;
    }
    line(
        &format!("Settings · profile {}", view.profile),
        at(area, 0, 1),
        buf,
    );
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
            let footer = if view.screen == Screen::Reviewing {
                "↑ ↓ scroll · Enter apply · Esc back"
            } else {
                "Applying… (checking, saving, then reading back)"
            };
            line(footer, at(area, area.height - 1, 1), buf);
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
                "↑ ↓ scroll · o keep yours · t take theirs · Esc back",
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
        _ => draw_browser(view, area, buf),
    }
}

fn draw_browser(view: &SettingsView, area: Rect, buf: &mut Buffer) {
    let narrow = area.width < 100;
    let category = if view.category.is_empty() {
        view.rows
            .get(view.cursor)
            .and_then(|row| row.key.split_once('.').map(|(category, _)| category))
            .unwrap_or("general")
    } else {
        &view.category
    };
    let mut top = 2;
    if narrow {
        line(&format!("‹ {category} ›"), at(area, top, 1), buf);
        top += 1;
    }
    if !view.filter.is_empty() || view.text_focus && view.screen == Screen::Browsing {
        line(&format!("/ {}", view.filter), at(area, top, 1), buf);
        top += 1;
    }
    let bottom = area
        .height
        .saturating_sub(5 + view.banner.len().min(3) as u16);
    let capacity = usize::from(bottom.saturating_sub(top));
    let start = view.cursor.saturating_sub(capacity.saturating_sub(1));
    if !narrow {
        let selected = view
            .categories
            .iter()
            .position(|name| name == category)
            .unwrap_or(0);
        let first = selected.saturating_sub(capacity.saturating_sub(1));
        for (offset, name) in view
            .categories
            .iter()
            .skip(first)
            .take(capacity)
            .enumerate()
        {
            let active = view.filter.is_empty() && name == category;
            line(
                &format!("{} {name}", if active { ">" } else { " " }),
                Rect::new(area.x, area.y + top + offset as u16, 18, 1),
                buf,
            );
        }
    }
    for (offset, row) in view.rows.iter().enumerate().skip(start).take(capacity) {
        let y = top + (offset - start) as u16;
        let field_area = if narrow {
            at(area, y, 1)
        } else {
            Rect::new(area.x + 19, area.y + y, area.width - 19, 1)
        };
        let value = if is_secret_key(&row.key) {
            "•••••••• (r reveals once)"
        } else {
            &row.value
        };
        let text = if narrow {
            format!(
                "{} {}  {}  {}  {}",
                if offset == view.cursor { ">" } else { " " },
                row.key,
                value,
                row.marker,
                row.origin
            )
        } else {
            format!(
                "{} {}  {}  {}  {}  {}",
                if offset == view.cursor { ">" } else { " " },
                column(
                    &row.key,
                    usize::from(field_area.width).saturating_sub(60).min(34)
                ),
                column(value, 25),
                column(&row.marker, 1),
                column(&row.origin, 7),
                row.effect_label
            )
        };
        let style = if offset == view.cursor {
            Style::default().add_modifier(Modifier::BOLD)
        } else {
            Style::default()
        };
        Paragraph::new(sanitize(&text).into_owned())
            .style(style)
            .render(field_area, buf);
    }
    if view.rows.is_empty() && view.links.is_empty() {
        line(
            &format!("No setting matches \"{}\".", view.filter),
            at(area, top, 1),
            buf,
        );
    }
    for (i, link) in view.links.iter().enumerate().take(capacity) {
        line(
            &format!("{} {link}  →", if i == view.cursor { ">" } else { " " }),
            at(area, top + i as u16, 1),
            buf,
        );
    }
    let mut detail = view.detail.clone();
    if narrow && let Some(row) = view.rows.get(view.cursor) {
        detail = format!("{} · {detail}", row.effect_label);
    }
    lines(vec![detail], at(area, bottom, 2), buf);
    let messages = view.banner.clone();
    if view.screen == Screen::Choosing {
        let available = usize::from(area.height.saturating_sub(top + 1));
        let first = view
            .choice_cursor
            .saturating_sub(available.saturating_sub(1));
        let options = view
            .choices
            .iter()
            .enumerate()
            .skip(first)
            .take(available)
            .map(|(i, choice)| {
                format!(
                    "{} {choice}",
                    if i == view.choice_cursor { ">" } else { " " }
                )
            })
            .collect();
        Clear.render(at(area, top, area.height.saturating_sub(top + 1)), buf);
        lines(
            options,
            at(area, top, area.height.saturating_sub(top + 1)),
            buf,
        );
    } else {
        let region = at(area, bottom + 2, area.height.saturating_sub(bottom + 3));
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
                crate::editor::draw(
                    &value,
                    editor.cursor,
                    Rect::new(
                        row.x + row.width.min(2),
                        row.y,
                        row.width.saturating_sub(2),
                        1,
                    ),
                    buf,
                );
            }
        } else {
            lines(messages, region, buf);
        }
    }
    let footer = if view.screen == Screen::Editing
        && view.editor.as_ref().is_some_and(|editor| editor.secret)
    {
        "Enter accept · Esc cancel · Alt+R reveal once"
    } else if view.footer.is_empty() {
        "Enter edit · d default · u undo · r reveal · Ctrl+R reload · Esc close"
    } else {
        &view.footer
    };
    line(footer, at(area, area.height - 1, 1), buf);
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
