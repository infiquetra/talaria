use ratatui::{
    buffer::Buffer,
    layout::{Alignment, Rect},
    style::{Modifier, Style},
    text::Line,
    widgets::{Block, Clear, Paragraph, Widget, Wrap},
};
use talaria_core::mcp::{CardView, McpTab, McpView, SecretSlot};
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

use crate::history::sanitize;

fn clipped(text: &str, width: usize) -> String {
    let text = sanitize(text);
    if UnicodeWidthStr::width(text.as_ref()) <= width {
        return text.into_owned();
    }
    if width == 0 {
        return String::new();
    }
    let mut result = String::new();
    let mut used = 0;
    for grapheme in text.graphemes(true) {
        let size = UnicodeWidthStr::width(grapheme);
        if used + size > width - 1 {
            break;
        }
        result.push_str(grapheme);
        used += size;
    }
    result.push('…');
    result
}

fn line(buf: &mut Buffer, area: Rect, y: u16, text: &str, style: Style) {
    if y < area.height && area.width > 0 {
        let rect = Rect::new(area.x, area.y + y, area.width, 1);
        Paragraph::new(clipped(text, usize::from(area.width)))
            .style(style)
            .render(rect, buf);
    }
}

fn column(text: &str, width: usize) -> String {
    let text = clipped(text, width);
    let padding = width.saturating_sub(UnicodeWidthStr::width(text.as_str()));
    format!("{text}{}", " ".repeat(padding))
}

pub fn draw_mcp(
    view: &McpView,
    secret_len: &dyn Fn(SecretSlot) -> usize,
    area: Rect,
    buf: &mut Buffer,
) {
    if area.width == 0 || area.height == 0 {
        return;
    }
    Clear.render(area, buf);
    let tabs = match view.tab {
        McpTab::Servers => "[Servers]  Catalogue",
        McpTab::Catalogue => "Servers  [Catalogue]",
    };
    let title_width = usize::from(area.width).saturating_sub(UnicodeWidthStr::width(tabs));
    let title = column(&view.title, title_width);
    line(buf, area, 0, &format!("{title}{tabs}"), Style::default());
    line(
        buf,
        area,
        1,
        &"─".repeat(usize::from(area.width)),
        Style::default(),
    );
    let filter = if view.filter.is_empty() {
        "type to filter".to_owned()
    } else {
        format!("filter: {}", view.filter)
    };
    line(buf, area, 2, &filter, Style::default());
    let mut start = 3;
    if let Some(banner) = &view.banner {
        line(buf, area, start, banner, Style::default());
        start += 1;
    }
    let end = area.height.saturating_sub(4);
    if let Some(placeholder) = &view.placeholder {
        if start < end {
            line(buf, area, start, placeholder, Style::default());
        }
    } else {
        let capacity = usize::from(end.saturating_sub(start));
        let offset = view
            .selected
            .unwrap_or(0)
            .saturating_add(1)
            .saturating_sub(capacity);
        for (index, row) in view.rows.iter().enumerate().skip(offset).take(capacity) {
            let marker = if view.selected == Some(index) {
                "› "
            } else {
                "  "
            };
            let name_width = if view.tab == McpTab::Servers { 28 } else { 24 };
            let (name, label) = if view.tab == McpTab::Servers {
                match row
                    .label
                    .strip_prefix("plugin ")
                    .and_then(|rest| rest.split_once(" · "))
                {
                    Some((plugin, label)) => (format!("plugin {plugin} · {}", row.name), label),
                    None => (row.name.clone(), row.label.as_str()),
                }
            } else {
                (row.name.clone(), row.label.as_str())
            };
            let name = column(&name, name_width);
            let kind = if area.width >= 100 {
                format!("{}  ", column(&row.kind, 6))
            } else {
                String::new()
            };
            let text = format!("{marker}{name} {kind}{label}");
            let style = if row.dim {
                Style::default().add_modifier(Modifier::DIM)
            } else {
                Style::default()
            };
            line(buf, area, start + (index - offset) as u16, &text, style);
        }
    }
    for (position, text) in [
        view.status_line.as_deref(),
        view.pending_line.as_deref(),
        view.notice.as_deref(),
        Some(view.footer.as_str()),
    ]
    .into_iter()
    .enumerate()
    {
        if let Some(text) = text {
            line(
                buf,
                area,
                area.height.saturating_sub(4) + position as u16,
                text,
                Style::default(),
            );
        }
    }
    if let Some(card) = &view.card {
        draw_card(card, secret_len, area, buf);
    }
}

fn draw_card(
    card: &CardView,
    secret_len: &dyn Fn(SecretSlot) -> usize,
    area: Rect,
    buf: &mut Buffer,
) {
    let width = if area.width < 60 {
        area.width
    } else {
        area.width.saturating_sub(4).min(90)
    };
    if width < 2 || area.height < 2 {
        return;
    }
    let mut lines: Vec<String> = card
        .lines
        .iter()
        .map(|text| sanitize(text).into_owned())
        .collect();
    for field in &card.fields {
        let value = if let Some(slot) = field.secret {
            let len = secret_len(slot);
            format!(
                "{}{}",
                "•".repeat(len.min(40)),
                if len > 40 { "…" } else { "" }
            )
        } else {
            sanitize(&field.text).into_owned()
        };
        lines.push(format!(
            "{}{}: {value}",
            if field.focused { "› " } else { "  " },
            sanitize(&field.label)
        ));
        if let Some(error) = &field.error {
            lines.push(format!("! {}", sanitize(error)));
        }
        if let Some(warning) = &field.warning {
            lines.push(format!("~ {}", sanitize(warning)));
        }
    }
    let inner_width = usize::from(width.saturating_sub(2)).max(1);
    let content_height: usize = lines
        .iter()
        .map(|text| {
            UnicodeWidthStr::width(text.as_str())
                .max(1)
                .div_ceil(inner_width)
        })
        .sum();
    let height = (content_height.saturating_add(2)).min(usize::from(area.height)) as u16;
    let rect = Rect::new(
        area.x + (area.width - width) / 2,
        area.y + (area.height - height) / 2,
        width,
        height,
    );
    Clear.render(rect, buf);
    let title = format!(" {} ", sanitize(&card.title));
    let footer = format!(" {} ", sanitize(&card.footer));
    let block = Block::bordered()
        .title(Line::from(title))
        .title_bottom(Line::from(footer).alignment(Alignment::Left));
    Paragraph::new(lines.join("\n"))
        .block(block)
        .wrap(Wrap { trim: false })
        .render(rect, buf);
}
