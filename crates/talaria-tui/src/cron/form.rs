use super::put;
use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::Line,
};
use talaria_core::cron::{FormField, FormView};

pub(super) fn draw(view: &FormView, area: Rect, buf: &mut Buffer) {
    let mut lines: Vec<(String, Style)> = Vec::new();
    let mut cursor = None;
    let mut focused = 0;
    for field in &view.fields {
        let style = if field.field == view.focus {
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default()
        };
        let first_row = lines.len();
        if field.field == view.focus {
            focused = first_row;
        }
        if field.field == view.focus
            && let Some((line, column)) = view.cursor
        {
            let values: Vec<&str> = field.value.split('\n').collect();
            if let Some(value) = values.get(line) {
                let prefix = if line == 0 {
                    format!("{}: ", field.label)
                } else {
                    "  ".to_owned()
                };
                let before: String = value.chars().take(column).collect();
                let clean = crate::history::sanitize(&before);
                cursor = Some((
                    first_row + line,
                    Line::raw(prefix).width() + Line::raw(clean.as_ref()).width(),
                ));
            }
        }
        let mut values = field.value.split('\n');
        lines.push((
            format!("{}: {}", field.label, values.next().unwrap_or("")),
            style,
        ));
        lines.extend(values.map(|value| (format!("  {value}"), style)));
        if field.field == FormField::Deliver {
            if field.field == view.focus && !view.targets.is_empty() {
                focused = lines.len() + view.deliver_index.min(view.targets.len() - 1);
            }
            for (index, (target, selectable)) in view.targets.iter().enumerate() {
                lines.push((
                    format!(
                        "{} {}{}",
                        if index == view.deliver_index {
                            ">"
                        } else {
                            " "
                        },
                        target,
                        if *selectable { "" } else { " (unavailable)" }
                    ),
                    style,
                ));
            }
        }
        if let Some(hint) = &field.hint {
            lines.push((hint.clone(), Style::default().fg(Color::DarkGray)));
        }
        if let Some(error) = &field.error {
            lines.push((error.clone(), Style::default().fg(Color::Red)));
        }
    }
    for status in &view.status {
        lines.push((status.clone(), Style::default().fg(Color::Yellow)));
    }
    let focus_row = if view.focus == FormField::Deliver {
        focused
    } else {
        cursor.map_or(focused, |(row, _)| row)
    };
    let start = focus_row.saturating_sub(usize::from(area.height).saturating_sub(2));
    for (row, (line, style)) in lines
        .iter()
        .skip(start)
        .take(usize::from(area.height))
        .enumerate()
    {
        if let Some((cursor_row, cursor_col)) = cursor
            && cursor_row == row + start
        {
            let clean = crate::history::sanitize(line);
            let shift = cursor_col.saturating_sub(usize::from(area.width).saturating_sub(1));
            let mut offset = 0usize;
            let visible: String = clean
                .chars()
                .skip_while(|ch| {
                    let width = Line::raw(ch.to_string()).width();
                    if offset + width <= shift {
                        offset += width;
                        true
                    } else {
                        false
                    }
                })
                .collect();
            put(buf, area, row as u16, &visible, *style);
            let x = cursor_col.saturating_sub(offset);
            if x < usize::from(area.width) {
                buf[(area.x + x as u16, area.y + row as u16)].set_style(style.bg(Color::DarkGray));
            }
        } else {
            put(buf, area, row as u16, line, *style);
        }
    }
}
