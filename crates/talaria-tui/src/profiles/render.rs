use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Style},
};
use talaria_core::profiles::{DetailView, Panel, ProfilesView, RowTone};
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

use super::form;
use crate::history::sanitize;

fn cut(text: &str, width: usize) -> String {
    let clean: String = sanitize(text).chars().take(300).collect();
    if UnicodeWidthStr::width(clean.as_str()) <= width {
        return clean;
    }
    if width == 0 {
        return String::new();
    }
    let mut out = String::new();
    let mut used = 0;
    for g in clean.graphemes(true) {
        let w = UnicodeWidthStr::width(g);
        if used + w > width - 1 {
            break;
        }
        out.push_str(g);
        used += w;
    }
    out.push('…');
    out
}

fn padded(text: &str, width: usize) -> String {
    let text = cut(text, width);
    let spaces = width.saturating_sub(UnicodeWidthStr::width(text.as_str()));
    format!("{text}{}", " ".repeat(spaces))
}

fn put(buffer: &mut Buffer, area: Rect, row: usize, text: &str, style: Style) {
    if row >= usize::from(area.height) {
        return;
    }
    let y = area
        .y
        .saturating_add(u16::try_from(row).unwrap_or(u16::MAX));
    buffer.set_stringn(
        area.x,
        y,
        cut(text, usize::from(area.width)),
        usize::from(area.width),
        style,
    );
}

fn wrapped(text: &str, width: usize) -> Vec<String> {
    if width == 0 {
        return Vec::new();
    }
    let mut lines = Vec::new();
    let clean: String = sanitize(text).chars().take(300).collect();
    let mut line = String::new();
    for word in clean.split_whitespace() {
        let word_width = UnicodeWidthStr::width(word);
        if !line.is_empty() && UnicodeWidthStr::width(line.as_str()) + 1 + word_width > width {
            lines.push(std::mem::take(&mut line));
        }
        if word_width <= width {
            if !line.is_empty() {
                line.push(' ');
            }
            line.push_str(word);
        } else {
            for g in word.graphemes(true) {
                if UnicodeWidthStr::width(line.as_str()) + UnicodeWidthStr::width(g) > width {
                    lines.push(std::mem::take(&mut line));
                }
                line.push_str(g);
            }
        }
    }
    lines.push(line);
    lines
}

fn list_height(view: &ProfilesView, area: Rect) -> usize {
    if area.width >= 100 {
        view.rows
            .len()
            .min(usize::from(area.height.saturating_sub(2) / 2))
            .max(1)
            .min(usize::from(area.height.saturating_sub(1)))
    } else {
        usize::from(area.height.saturating_sub(1))
    }
}

pub(super) fn notice_height(view: &ProfilesView, area: Rect) -> usize {
    view.confirm
        .as_ref()
        .or(view.notice.as_ref())
        .map_or(0, |text| wrapped(text, usize::from(area.width)).len())
        .min(usize::from(area.height.saturating_sub(2)))
}

/// Number of list rows visible above the bottom notice or confirmation.
#[must_use]
pub fn list_page_size(view: &ProfilesView, area: Rect) -> usize {
    if area.width == 0 || area.height == 0 {
        return 0;
    }
    let height = list_height(view, area);
    let below_list = usize::from(area.height).saturating_sub(1 + height);
    height.saturating_sub(notice_height(view, area).saturating_sub(below_list))
}

fn detail(detail: &DetailView, area: Rect, buffer: &mut Buffer) {
    let mut lines = vec![detail.title.clone()];
    lines.extend(wrapped(&detail.state_text, usize::from(area.width)));
    lines.extend(detail.lines.iter().cloned());
    for (i, line) in lines.iter().enumerate() {
        put(buffer, area, i, line, Style::default());
    }
}

fn list(view: &ProfilesView, area: Rect, buffer: &mut Buffer, reserved: usize, top: &mut usize) {
    if view.rows.is_empty() {
        if let Some(filter) = &view.filter {
            put(
                buffer,
                area,
                0,
                &format!("No profile matches \"{filter}\"."),
                Style::default(),
            );
        }
        return;
    }
    let width = usize::from(area.width);
    let name_width = view
        .rows
        .iter()
        .map(|r| UnicodeWidthStr::width(sanitize(&r.name_cell).as_ref()))
        .max()
        .unwrap_or(0)
        .min(24);
    let model_width = view
        .rows
        .iter()
        .map(|r| UnicodeWidthStr::width(sanitize(&r.model_cell).as_ref()))
        .max()
        .unwrap_or(0)
        .min(34);
    let height = usize::from(area.height).saturating_sub(reserved);
    *top = (*top).min(view.rows.len().saturating_sub(height));
    if let Some(selected) = view.selected {
        if selected < *top {
            *top = selected;
        }
        if height > 0 && selected >= top.saturating_add(height) {
            *top = selected + 1 - height;
        }
    }
    let start = *top;
    for (i, row) in view.rows.iter().enumerate().skip(start).take(height) {
        let marker = if view.selected == Some(i) {
            "▸ "
        } else {
            "  "
        };
        let mut line = format!(
            "{marker}{}  {}",
            padded(&row.name_cell, name_width),
            padded(&row.label, 14)
        );
        if width >= 80 {
            line.push_str(&format!("  {}", padded(&row.model_cell, model_width)));
        }
        if width >= 40 {
            line.push_str(&format!("  {}", row.next_step));
        }
        let style = if row.tone == RowTone::Warning {
            Style::default().fg(Color::Yellow)
        } else {
            Style::default()
        };
        put(buffer, area, i - start, &line, style);
    }
}

/// Draw the profiles overlay into the supplied area.
pub fn draw(view: &ProfilesView, area: Rect, buffer: &mut Buffer) -> Option<(u16, u16)> {
    ProfilesRenderer::default().draw(view, area, buffer);
    super::cursor_position(view, area)
}

/// Holds the list viewport between frames.
#[derive(Debug, Default)]
pub struct ProfilesRenderer {
    top: usize,
}

impl ProfilesRenderer {
    /// Draw while preserving the least list scroll needed to show the selection.
    pub fn draw(&mut self, view: &ProfilesView, area: Rect, buffer: &mut Buffer) {
        if area.width == 0 || area.height == 0 {
            return;
        }
        let width = usize::from(area.width);
        let narrow_detail = area.width < 100 && matches!(view.panel, Panel::NarrowDetail(_));
        let notice = view.confirm.as_ref().or(view.notice.as_ref());
        let mut notice_lines = notice.map(|n| wrapped(n, width)).unwrap_or_default();
        let max_notice = usize::from(area.height.saturating_sub(2));
        if notice_lines.len() > max_notice {
            notice_lines.truncate(max_notice);
            if let Some(last) = notice_lines.last_mut() {
                *last = cut(&format!("{last}…"), width);
            }
        }
        if !narrow_detail {
            let mut header = cut(&view.header, width);
            let hints = ["r refresh", "R rescan", "n new", "e describe", "s soul"];
            let right = if let Some(filter) = &view.filter {
                format!("/{filter}")
            } else if width >= 40 {
                let mut n = hints.len();
                while n > 0
                    && UnicodeWidthStr::width(header.as_str())
                        + 2
                        + UnicodeWidthStr::width(hints[..n].join(" · ").as_str())
                        > width
                {
                    n -= 1;
                }
                hints[..n].join(" · ")
            } else {
                String::new()
            };
            let available = width.saturating_sub(UnicodeWidthStr::width(header.as_str()) + 2);
            let right = if UnicodeWidthStr::width(right.as_str()) > available {
                let mut tail = String::new();
                for g in right.graphemes(true).rev() {
                    if UnicodeWidthStr::width(tail.as_str()) + UnicodeWidthStr::width(g)
                        >= available
                    {
                        break;
                    }
                    tail.insert_str(0, g);
                }
                if available > 0 {
                    format!("…{tail}")
                } else {
                    String::new()
                }
            } else {
                right
            };
            if !right.is_empty() {
                header.push_str(&" ".repeat(width.saturating_sub(
                    UnicodeWidthStr::width(header.as_str())
                        + UnicodeWidthStr::width(right.as_str()),
                )));
                header.push_str(&right);
            }
            put(buffer, area, 0, &header, Style::default());
        }
        if view.loading {
            put(buffer, area, 1, "Loading profiles...", Style::default());
        } else if narrow_detail {
            if let Panel::NarrowDetail(d) = &view.panel {
                detail(d, area, buffer);
            }
        } else if view.filter.is_some() && view.rows.is_empty() && matches!(view.panel, Panel::None)
        {
            list(
                view,
                Rect::new(
                    area.x,
                    area.y.saturating_add(1),
                    area.width,
                    area.height.saturating_sub(1),
                ),
                buffer,
                0,
                &mut self.top,
            );
        } else {
            let list_height = list_height(view, area);
            let list_area = Rect::new(
                area.x,
                area.y.saturating_add(1),
                area.width,
                u16::try_from(list_height)
                    .unwrap_or(u16::MAX)
                    .min(area.height.saturating_sub(1)),
            );
            let panel_open = matches!(
                view.panel,
                Panel::Form(_) | Panel::Review(_) | Panel::DescriptionInput { .. }
            );
            if area.width >= 100 || !panel_open {
                list(
                    view,
                    list_area,
                    buffer,
                    list_height.saturating_sub(list_page_size(view, area)),
                    &mut self.top,
                );
            }
            let panel_area = if area.width >= 100 {
                let separator = 1 + list_height;
                put(
                    buffer,
                    area,
                    separator,
                    &"─".repeat(width),
                    Style::default(),
                );
                let top = u16::try_from(separator + 1)
                    .unwrap_or(u16::MAX)
                    .min(area.height);
                Rect::new(
                    area.x,
                    area.y.saturating_add(top),
                    area.width,
                    area.height.saturating_sub(top),
                )
            } else {
                list_area
            };
            match &view.panel {
                Panel::Detail(d) if area.width >= 100 => detail(d, panel_area, buffer),
                Panel::Form(_) | Panel::Review(_) | Panel::DescriptionInput { .. } => {
                    let lines = form::lines(&view.panel);
                    for (i, line) in lines
                        .iter()
                        .enumerate()
                        .take(usize::from(panel_area.height))
                    {
                        put(buffer, panel_area, i, line, Style::default());
                    }
                }
                _ => {}
            }
        }
        let top = usize::from(area.height).saturating_sub(notice_lines.len());
        for (i, line) in notice_lines.iter().enumerate() {
            let y = area
                .y
                .saturating_add(u16::try_from(top + i).unwrap_or(u16::MAX));
            buffer.set_stringn(area.x, y, " ".repeat(width), width, Style::default());
            put(buffer, area, top + i, line, Style::default());
        }
    }
}
