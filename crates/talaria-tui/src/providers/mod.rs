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
use talaria_core::models::{BrowseCounts, BrowseView, ListScope, PickerRow, PickerView, RowKey};
use talaria_core::providers::{CardKind, ProvidersTab, ProvidersView};

use crate::picker_frame::{FrameLabels, draw_picker_frame_with_labels};

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
    let area = area.intersection(buf.area);
    Clear.render(area, buf);
    if area.width == 0 || area.height == 0 {
        return;
    }
    if view.card.is_none() {
        draw_browse(view, area, buf);
        return;
    }
    let outer = Block::bordered().title(safe(&view.title));
    let inner = outer.inner(area);
    outer.render(area, buf);
    if inner.is_empty() {
        return;
    }
    if let Some(card) = &view.card {
        let width = inner.width.min(76);
        let is_form = card.kind == CardKind::EndpointForm;
        let mut lines = if is_form {
            endpoint_form::lines(card, secret_len)
        } else {
            signin_card::lines(card, secret_len)
        };
        if let Some(message) = view.banner.as_deref().or(view.notice.as_deref()) {
            lines.push(safe(message));
        }
        let title = if is_form {
            "Endpoint".to_string()
        } else {
            card.lines
                .first()
                .map_or_else(String::new, |line| safe(line))
        };
        let title = card
            .step
            .as_deref()
            .map_or(title.clone(), |step| format!("{} · {title}", safe(step)));
        if Line::from(title.as_str()).width() > usize::from(width.saturating_sub(2)) {
            lines.insert(0, title.clone());
        }
        let body_width = width.saturating_sub(2).max(1);
        let body_height: usize = lines
            .iter()
            .map(|line| {
                Paragraph::new(line.as_str())
                    .wrap(Wrap { trim: false })
                    .line_count(body_width)
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
            let scroll = if is_form {
                lines
                    .iter()
                    .position(|line| line.starts_with("› "))
                    .map_or(0, |selected| {
                        let count = |line: &String| {
                            Paragraph::new(line.as_str())
                                .wrap(Wrap { trim: false })
                                .line_count(body.width)
                        };
                        let start: usize = lines[..selected].iter().map(count).sum();
                        let end = start + count(&lines[selected]);
                        end.saturating_sub(usize::from(body.height)).min(start)
                    })
            } else {
                0
            };
            Paragraph::new(lines.into_iter().map(Line::from).collect::<Vec<_>>())
                .wrap(Wrap { trim: false })
                .scroll((u16::try_from(scroll).unwrap_or(u16::MAX), 0))
                .render(body, buf);
            Paragraph::new(safe(&card.footer)).render(
                Rect::new(content.x, content.bottom() - 1, content.width, 1),
                buf,
            );
        }
    }
}

fn draw_browse(view: &ProvidersView, area: Rect, buf: &mut Buffer) {
    let scope = match view.tab {
        ProvidersTab::All => "All",
        ProvidersTab::Keys => "Keys",
        ProvidersTab::Accounts => "Accounts",
        ProvidersTab::Endpoints => "Endpoints",
    };
    let browse = view.browse.as_ref();
    // Row identities here only mark selectable rows for the shared painter.
    let rows: Vec<_> = if let Some(browse) = browse {
        browse
            .rows
            .iter()
            .enumerate()
            .map(|(index, row)| PickerRow {
                key: row.key.as_ref().map(|_| RowKey::NotSetUp {
                    provider: index.to_string(),
                }),
                text: row.label.clone(),
                badges: row
                    .status
                    .map(|status| vec![status.label().into()])
                    .unwrap_or_default(),
                price: None,
                dim: false,
                header: row.key.is_none(),
            })
            .collect()
    } else {
        view.rows
            .iter()
            .enumerate()
            .map(|(index, row)| PickerRow {
                key: Some(RowKey::NotSetUp {
                    provider: index.to_string(),
                }),
                text: row.clone(),
                badges: Vec::new(),
                price: None,
                dim: false,
                header: false,
            })
            .collect()
    };
    let selected = browse.map_or(view.selected, |browse| browse.selected);
    let details = browse
        .and_then(|browse| selected.and_then(|index| browse.rows.get(index)))
        .map_or(&[][..], |row| row.details.as_slice());
    let banner = [view.banner.as_deref(), view.notice.as_deref()]
        .into_iter()
        .flatten()
        .collect::<Vec<_>>()
        .join("\n");
    let count = browse.map_or_else(String::new, |browse| {
        format!(
            "{} providers{}",
            browse.counts.providers,
            if browse.counts.complete {
                ""
            } else {
                " · count incomplete"
            }
        )
    });
    let hints = if view.footer.is_empty() {
        "Enter set up · Esc close · F2/Ctrl+E details\nTab view · Ctrl+R reload · Ctrl+U clear"
    } else {
        &view.footer
    };
    let picker = PickerView {
        browse: Some(BrowseView {
            scope: ListScope::All,
            counts: BrowseCounts::default(),
            details,
            details_open: browse.is_some_and(|browse| browse.details_open),
            details_scroll: browse.map_or(0, |browse| browse.details_scroll),
        }),
        title: &view.title,
        filter: &view.filter,
        effort: "",
        hints,
        refreshing: false,
        rows: &rows,
        selected,
        banner: (!banner.is_empty()).then_some(banner.as_str()),
    };
    draw_picker_frame_with_labels(
        &picker,
        FrameLabels {
            scope,
            scopes: &["All", "Keys", "Accounts", "Endpoints"],
            count: &count,
        },
        area,
        buf,
    );
}
