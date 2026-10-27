//! Provider overlay presentation and secret input.

mod clipboard;
mod endpoint_form;
mod secret_field;
mod signin_card;

pub use clipboard::osc52;
pub use secret_field::{SecretField, SecretOutcome};

use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Modifier, Style},
    text::Line,
    widgets::{Block, Borders, Clear, Paragraph, Widget, Wrap},
};
use talaria_core::providers::{CardKind, ProvidersTab, ProvidersView};

fn safe(text: &str) -> String {
    crate::history::sanitize(text).into_owned()
}

fn mask(len: usize) -> String {
    let mut text = "•".repeat(len.min(40));
    if len > 40 {
        text.push('…');
    }
    text
}

/// Draw the providers overlay without exposing the secret value.
pub fn draw_providers(view: &ProvidersView, secret_len: usize, area: Rect, buf: &mut Buffer) {
    Clear.render(area, buf);
    if area.width == 0 || area.height == 0 {
        return;
    }
    let outer = Block::default()
        .title(safe(&view.title))
        .borders(Borders::ALL);
    let inner = outer.inner(area);
    outer.render(area, buf);
    if inner.width == 0 || inner.height == 0 {
        return;
    }
    let mut lines = Vec::new();
    let tabs = [
        ProvidersTab::Keys,
        ProvidersTab::Accounts,
        ProvidersTab::Endpoints,
    ];
    lines.push(Line::from(
        tabs.into_iter()
            .map(|tab| {
                let name = match tab {
                    ProvidersTab::Keys => "Keys",
                    ProvidersTab::Accounts => "Accounts",
                    ProvidersTab::Endpoints => "Endpoints",
                };
                if tab == view.tab {
                    format!("[{name}] ")
                } else {
                    format!(" {name}  ")
                }
            })
            .collect::<String>(),
    ));
    if let Some(banner) = &view.banner {
        lines.push(Line::from(safe(banner)));
    }
    if !view.filter.is_empty() {
        lines.push(Line::from(format!("Filter: {}", safe(&view.filter))));
    }
    let available = usize::from(inner.height)
        .saturating_sub(lines.len() + usize::from(view.notice.is_some()) + 1);
    let first = view
        .selected
        .unwrap_or(0)
        .saturating_add(1)
        .saturating_sub(available);
    for (index, row) in view.rows.iter().enumerate().skip(first).take(available) {
        let marker = if view.selected == Some(index) {
            "› "
        } else {
            "  "
        };
        lines.push(Line::from(format!("{marker}{}", safe(row))));
    }
    if let Some(notice) = &view.notice {
        lines.push(Line::from(safe(notice)));
    }
    Paragraph::new(lines).render(inner, buf);
    if inner.height > 0 {
        Paragraph::new(safe(&view.footer))
            .render(Rect::new(inner.x, inner.bottom() - 1, inner.width, 1), buf);
    }
    if let Some(card) = &view.card {
        let width = inner.width.min(76);
        let is_form = card.kind == CardKind::EndpointForm;
        let mut lines = if is_form {
            endpoint_form::lines(card, secret_len)
        } else {
            signin_card::lines(card, secret_len)
        };
        let title = if is_form {
            "Endpoint".to_string()
        } else {
            card.lines
                .first()
                .map_or_else(String::new, |line| safe(line))
        };
        if Line::from(title.as_str()).width() > usize::from(width.saturating_sub(2)) {
            lines.insert(0, title.clone());
        }
        let body_width = usize::from(width.saturating_sub(2)).max(1);
        let body_height: usize = lines
            .iter()
            .map(|line| {
                Line::from(line.as_str())
                    .width()
                    .max(1)
                    .div_ceil(body_width)
            })
            .sum();
        let height = inner.height.min(
            u16::try_from(body_height.saturating_add(4))
                .unwrap_or(u16::MAX)
                .max(6),
        );
        let rect = Rect::new(
            inner.x + inner.width.saturating_sub(width) / 2,
            inner.y + inner.height.saturating_sub(height) / 2,
            width,
            height,
        );
        Clear.render(rect, buf);
        let block = Block::default()
            .title(title)
            .borders(Borders::ALL)
            .style(Style::default().add_modifier(Modifier::BOLD));
        let content = block.inner(rect);
        block.render(rect, buf);
        if content.height > 0 {
            let body = Rect::new(
                content.x,
                content.y,
                content.width,
                content.height.saturating_sub(1),
            );
            Paragraph::new(lines.into_iter().map(Line::from).collect::<Vec<_>>())
                .wrap(Wrap { trim: false })
                .render(body, buf);
            Paragraph::new(safe(&card.footer)).render(
                Rect::new(content.x, content.bottom() - 1, content.width, 1),
                buf,
            );
        }
    }
}
