//! Memory and curator presentation.

use ratatui::{
    buffer::Buffer,
    layout::Rect,
    text::{Line, Span},
    widgets::{Clear, Paragraph, Widget, Wrap},
};
pub use talaria_core::memory::MemoryKey;
use talaria_core::memory::{EditOp, MemoryView, RowView, Section};

use crate::{
    history::sanitize,
    keys::{Key, KeyCode, KeyKind, KeyMap},
};

fn clean(text: &str) -> String {
    sanitize(text).into_owned()
}

// Bound each column by terminal cells, including wide characters.
fn column(text: &str, width: usize) -> String {
    let clean = clean(text);
    let truncated = Line::from(clean.as_str()).width() > width;
    let budget = width.saturating_sub(usize::from(truncated));
    let mut output = String::new();
    let mut used = 0;
    for ch in clean.chars() {
        let cells = Span::raw(ch.to_string()).width();
        if used + cells > budget {
            break;
        }
        output.push(ch);
        used += cells;
    }
    if truncated && width > 0 {
        output.push('…');
        used += 1;
    }
    output.push_str(&" ".repeat(width.saturating_sub(used)));
    output
}

fn right_column(text: &str, width: usize) -> String {
    let value = column(text, width);
    let value = value.trim_end();
    format!(
        "{}{}",
        " ".repeat(width.saturating_sub(Line::from(value).width())),
        value
    )
}

fn rows(
    lines: &mut Vec<String>,
    section: &Section<Vec<RowView>>,
    cursor: usize,
    base: &mut usize,
    width: u16,
    descriptions: bool,
) {
    match section {
        Section::Loading => lines.push("Loading…".into()),
        Section::Failed(text) => lines.push(clean(text)),
        Section::Ready(items) => {
            for row in items {
                let selected = if *base == cursor { ">" } else { " " };
                *base += 1;
                let marker = column(&row.marker.to_string(), 1);
                let width = usize::from(width);
                if width < 80 && descriptions {
                    lines.push(format!(
                        " {selected} {marker} {}",
                        column(&row.name, width - 5)
                    ));
                    lines.push(format!("     {}", column(&row.label, width - 5)));
                } else if descriptions && width >= 100 {
                    let name_width = 20;
                    let label_width = 24;
                    let desc_width = width - 5 - name_width - 1 - label_width - 1;
                    lines.push(format!(
                        " {selected} {marker} {} {} {}",
                        column(&row.name, name_width),
                        column(&row.description, desc_width),
                        right_column(&row.label, label_width)
                    ));
                } else {
                    let name_width = 32;
                    let label_width = width - 5 - name_width - 1;
                    lines.push(format!(
                        " {selected} {marker} {} {}",
                        column(&row.name, name_width),
                        right_column(&row.label, label_width)
                    ));
                }
            }
        }
    }
}

/// Draw the memory overlay.
pub fn draw_memory(view: &MemoryView, area: Rect, buf: &mut Buffer) {
    Clear.render(area, buf);
    if area.width < 60 || area.height < 16 {
        Paragraph::new(
            "Make the terminal at least 60 columns wide and 16 rows tall to show memory settings.",
        )
        .wrap(Wrap { trim: true })
        .render(area, buf);
        return;
    }
    let mut lines = Vec::new();
    let mut editor = None;
    if let Some(confirm) = &view.confirm {
        lines.extend(confirm.lines.iter().map(|s| clean(s)));
        lines.push(String::new());
        lines.push(format!(
            "{}[ {} ]  {}[ {} ]",
            if confirm.selected == 0 { "> " } else { "  " },
            clean(&confirm.buttons[0]),
            if confirm.selected == 1 { "> " } else { "  " },
            clean(&confirm.buttons[1])
        ));
    } else if let Some(card) = &view.card {
        let profile = view.title.strip_prefix("Memory · profile ").unwrap_or("");
        let active = view.active_text.strip_prefix("active: ") == Some(card.name.as_str());
        let heading = format!("{} · {}", clean(&card.name), clean(&card.label));
        let context = format!(
            "profile {} · {}",
            clean(profile),
            if active { "in use" } else { "not in use" }
        );
        let width = usize::from(area.width);
        let right_width = Line::from(context.as_str()).width().min(width / 2);
        lines.push(format!(
            "{} {}",
            column(&heading, width - right_width - 1),
            right_column(&context, right_width)
        ));

        lines.push(clean(&card.description));
        lines.extend(card.needs.iter().map(|s| clean(s)));
        lines.push("SETTINGS".into());
        for field in &card.fields {
            let width = usize::from(area.width);
            let label_width = (width / 3).min(28);
            if view.editing && field.selected {
                editor = Some((
                    lines.len(),
                    label_width + 4,
                    field.value.clone(),
                    field.cursor.unwrap_or_else(|| field.value.chars().count()),
                ));
            }
            lines.push(format!(
                "{} {}{} {}",
                if field.selected { ">" } else { " " },
                if field.pending { "~" } else { " " },
                column(&field.label, label_width),
                column(&field.value, width - label_width - 4)
            ));
        }
        if let Some(setup) = &card.setup {
            lines.push(clean(setup));
        }
        lines.extend(card.result_lines.iter().map(|s| clean(s)));
        if let Some(banner) = &view.banner {
            lines.push(clean(banner));
        }
    } else {
        lines.push(clean(&view.title));
        if let Some(banner) = &view.banner {
            lines.push(clean(banner));
        }
        lines.push(format!("MEMORY PROVIDER  {}", clean(&view.active_text)));
        let mut base = 0;
        rows(
            &mut lines,
            &view.providers,
            view.cursor,
            &mut base,
            area.width,
            true,
        );
        lines.push("BUILT-IN MEMORY".into());
        rows(
            &mut lines,
            &view.files,
            view.cursor,
            &mut base,
            area.width,
            false,
        );
        lines.push("SKILL CURATOR".into());
        match &view.curator {
            Section::Loading => lines.push("Loading…".into()),
            Section::Failed(text) => lines.push(clean(text)),
            Section::Ready(row) => {
                lines.push(format!(
                    " {} {}",
                    if view.cursor == base { ">" } else { " " },
                    clean(&row.status)
                ));
                if view.following.is_none() {
                    lines.push(clean(&row.schedule));
                    lines.push(clean(&row.last_run));
                }
            }
        }
        if let Some(follow) = &view.following {
            lines.push(format!("Curator running · {} s", follow.elapsed_s));
            lines.extend(follow.lines.iter().map(|s| clean(s)));
        }
    }
    // Keep the selected row reachable when a provider or its settings exceed the viewport.
    let selected = if view.confirm.is_none() {
        lines
            .iter()
            .position(|line| line.starts_with(" > ") || line.starts_with("> "))
    } else {
        None
    };
    let scroll = selected.map_or(0, |row| {
        let wrapped_rows: usize = lines[..row]
            .iter()
            .map(|line| {
                Paragraph::new(line.as_str())
                    .wrap(Wrap { trim: true })
                    .line_count(area.width)
            })
            .sum();
        wrapped_rows.saturating_sub(usize::from(area.height).saturating_sub(3))
    });
    let editor_row = editor.as_ref().map(|(row, _, _, _)| {
        lines[..*row]
            .iter()
            .map(|line| {
                Paragraph::new(line.as_str())
                    .wrap(Wrap { trim: true })
                    .line_count(area.width)
            })
            .sum::<usize>()
    });
    let text: Vec<Line<'static>> = lines.into_iter().map(Line::from).collect();
    let body = Rect::new(area.x, area.y, area.width, area.height.saturating_sub(1));
    Paragraph::new(text)
        .wrap(Wrap { trim: true })
        .scroll((u16::try_from(scroll).unwrap_or(u16::MAX), 0))
        .render(body, buf);
    if let (Some((_, x, value, cursor)), Some(row)) = (editor, editor_row)
        && let Some(y) = row.checked_sub(scroll)
        && y < usize::from(body.height)
        && x < usize::from(area.width)
    {
        crate::editor::draw(
            &value,
            cursor,
            Rect::new(
                area.x + x as u16,
                area.y + y as u16,
                area.width - x as u16,
                1,
            ),
            buf,
        );
    }
    Paragraph::new(footer(view)).render(Rect::new(area.x, area.bottom() - 1, area.width, 1), buf);
}

fn footer(view: &MemoryView) -> String {
    view.footer
        .iter()
        .map(|(key, action)| format!("{} {}", clean(key), clean(action)))
        .collect::<Vec<_>>()
        .join(" · ")
}

/// Route a key to the memory screen, preserving editor input and modal confirmations.
pub fn route_memory(view: &MemoryView, map: &KeyMap, key: &Key) -> Option<MemoryKey> {
    if key.kind == KeyKind::Release {
        return None;
    }
    if view.confirm.is_some() {
        if key.mods.ctrl || key.mods.alt || key.mods.super_key || key.mods.other {
            return None;
        }
        return match key.code {
            KeyCode::Left => Some(MemoryKey::Left),
            KeyCode::Right => Some(MemoryKey::Right),
            KeyCode::Tab => Some(MemoryKey::Tab),
            KeyCode::Enter => Some(MemoryKey::Open),
            KeyCode::Esc => Some(MemoryKey::Close),
            _ => None,
        };
    }
    if view.editing {
        if key.mods.ctrl || key.mods.alt || key.mods.super_key || key.mods.other {
            return None;
        }
        return match key.code {
            KeyCode::Char(c) => Some(MemoryKey::Char(c)),
            KeyCode::Enter => Some(MemoryKey::Open),
            KeyCode::Esc => Some(MemoryKey::Close),
            KeyCode::Up => Some(MemoryKey::Up),
            KeyCode::Down => Some(MemoryKey::Down),
            KeyCode::Backspace => Some(MemoryKey::Edit(EditOp::Backspace)),
            KeyCode::Delete => Some(MemoryKey::Edit(EditOp::Delete)),
            KeyCode::Left => Some(MemoryKey::Edit(EditOp::Left)),
            KeyCode::Right => Some(MemoryKey::Edit(EditOp::Right)),
            KeyCode::Home => Some(MemoryKey::Edit(EditOp::Home)),
            KeyCode::End => Some(MemoryKey::Edit(EditOp::End)),
            _ => None,
        };
    }
    map.resolve_memory(key).or_else(|| {
        if key.mods.ctrl || key.mods.alt || key.mods.super_key || key.mods.other {
            return None;
        }
        match key.code {
            KeyCode::Left => Some(MemoryKey::Left),
            KeyCode::Right => Some(MemoryKey::Right),
            KeyCode::Tab => Some(MemoryKey::Tab),
            KeyCode::Char(' ') => Some(MemoryKey::Space),
            _ => None,
        }
    })
}
