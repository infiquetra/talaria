//! Connectors and messaging overlay drawing.

use crate::{
    history::sanitize,
    status::{StatusPalette, Tone as StatusTone},
};
use ratatui::{
    buffer::Buffer,
    layout::Rect,
    text::Line,
    widgets::{Clear, Paragraph, Widget, Wrap},
};
use std::collections::BTreeMap;
pub use talaria_core::connectors::ConnectorsKey;
use talaria_core::connectors::{
    ConnectorsTab, ConnectorsView, FieldValue, Row, Screen, Tone, ToolState,
};

fn rows(out: &mut Vec<String>, entries: &[Row], selected: usize) {
    for (index, row) in entries.iter().enumerate() {
        out.push(format!(
            "{} {} {} · {}",
            if index == selected { ">" } else { " " },
            row.marker.unwrap_or(' '),
            row.label,
            row.state
        ));
    }
}

/// Paint the overlay. The lens holds lengths of typed secrets, never the characters.
pub fn draw_connectors(
    view: &ConnectorsView,
    secret_lens: &BTreeMap<String, usize>,
    area: Rect,
    buf: &mut Buffer,
) {
    draw_connectors_with_palette(view, secret_lens, &StatusPalette::default(), area, buf);
}

/// Paint the overlay using the caller's resolved status palette.
pub fn draw_connectors_with_palette(
    view: &ConnectorsView,
    secret_lens: &BTreeMap<String, usize>,
    palette: &StatusPalette,
    area: Rect,
    buf: &mut Buffer,
) {
    Clear.render(area, buf);
    if area.height == 0 || area.width == 0 {
        return;
    }
    let mut body = vec![
        view.title.clone(),
        match view.tab {
            ConnectorsTab::Connectors => "[Connectors]   Messaging",
            ConnectorsTab::Messaging => "Connectors   [Messaging]",
        }
        .into(),
        String::new(),
    ];
    let mut selected_line = None;
    let mut tones = BTreeMap::new();
    match &view.screen {
        Screen::Consent { text } => body.push(text.clone()),
        Screen::Loading => body.push("Loading…".into()),
        Screen::Unavailable { text, command } => {
            body.push(text.clone());
            if let Some(command) = command {
                body.push(command.clone());
            }
        }
        Screen::ConnectorList {
            rows: entries,
            selected,
            filter,
        } => {
            if let Some(filter) = filter {
                body.push(format!("Filter: {filter}"));
            }
            selected_line = (*selected < entries.len()).then_some(body.len() + selected);
            rows(&mut body, entries, *selected);
        }
        Screen::ConnectorCard {
            title,
            lines,
            accounts,
            retired,
            selected,
        } => {
            body.push(title.clone());
            for line in lines {
                tones.insert(body.len(), line.tone);
                body.push(line.text.clone());
            }
            selected_line = (*selected < accounts.len()).then_some(body.len() + selected);
            rows(&mut body, accounts, *selected);
            if *retired > 0 {
                body.push(format!("{retired} retired account(s) hidden · h show"));
            }
        }
        Screen::LinkCard {
            app,
            link,
            left,
            ssh,
            starting,
        } => {
            body.push(format!("Connect {app}"));
            if *starting {
                body.push("Starting…".into());
            }
            if let Some(link) = link {
                body.push(link.clone());
            }
            if let Some(left) = left {
                body.push(left.clone());
            }
            if *ssh {
                body.push(
                    "This is a remote shell: open the link on any device where you can sign in."
                        .into(),
                );
            }
        }
        Screen::Tools {
            header,
            cached,
            rows: entries,
            selected,
        } => {
            body.push(header.clone());
            if *cached {
                body.push("Cached tools".into());
            }
            selected_line = (*selected < entries.len()).then_some(body.len() + selected);
            for (index, row) in entries.iter().enumerate() {
                let state = match row.state {
                    ToolState::On => "on",
                    ToolState::Off => "off",
                    ToolState::Locked => "locked",
                };
                body.push(format!(
                    "{} {} · {} · {}{}{}",
                    if index == *selected { ">" } else { " " },
                    row.name,
                    row.facet,
                    state,
                    if row.draft { " · changed" } else { "" },
                    if row.deprecated { " · deprecated" } else { "" }
                ));
            }
        }
        Screen::PlatformList {
            rows: entries,
            selected,
            restart_line,
            other_pairing,
        } => {
            selected_line = (*selected < entries.len()).then_some(body.len() + selected);
            rows(&mut body, entries, *selected);
            if !other_pairing.is_empty() {
                body.push("Other platforms".into());
                body.extend(other_pairing.clone());
            }
            if *restart_line {
                body.push("R opens the doctor".into());
            }
        }
        Screen::PlatformCard {
            lines,
            fields,
            pairing,
            selected,
        } => {
            for line in lines {
                tones.insert(body.len(), line.tone);
                body.push(line.text.clone());
            }
            selected_line =
                (*selected < fields.len() + pairing.len()).then_some(body.len() + selected);
            for (index, field) in fields.iter().enumerate() {
                let value = match &field.value {
                    FieldValue::Set => "set".into(),
                    FieldValue::NotSet => "not set".into(),
                    FieldValue::List(values) => {
                        let mut text = values
                            .iter()
                            .take(3)
                            .cloned()
                            .collect::<Vec<_>>()
                            .join(", ");
                        if values.len() > 3 {
                            text.push_str(", …");
                        }
                        format!("{} entries: {text}", values.len())
                    }
                    FieldValue::Draft => "new value (hidden)".into(),
                    FieldValue::WillClear => "will clear".into(),
                };
                body.push(format!(
                    "{} {} ({}){}: {}",
                    if index == *selected { ">" } else { " " },
                    field.prompt,
                    field.key,
                    if field.required { " · required" } else { "" },
                    value
                ));
            }
            rows(
                &mut body,
                pairing,
                selected.checked_sub(fields.len()).unwrap_or(usize::MAX),
            );
        }
        Screen::Review { .. } => {}
        Screen::Confirm { text } => body.push(text.clone()),
        Screen::Prompt {
            title,
            text,
            help,
            problem,
        } => {
            body.push(title.clone());
            body.push(text.clone());
            if let Some(help) = help {
                body.push(help.clone());
            }
            if let Some(problem) = problem {
                body.push(problem.clone());
            }
        }
        Screen::SecretEditor { key, title, help } => {
            body.push(if key.is_empty() || title.contains(key) {
                title.clone()
            } else {
                format!("{title} ({key})")
            });
            let len = secret_lens.get(key).copied().unwrap_or(0);
            let mut mask = "•".repeat(len.min(40));
            if len > 40 {
                mask.push('…');
            }
            body.push(mask);
            if let Some(help) = help {
                body.push(help.clone());
            }
        }
    }
    let footer_height = u16::from(area.height > 1);
    let notice_height = u16::from(view.notice.is_some() && area.height > 2);
    let body_area = Rect::new(
        area.x,
        area.y,
        area.width,
        area.height.saturating_sub(footer_height + notice_height),
    );
    let clean: Vec<Line<'static>> = body
        .iter()
        .enumerate()
        .map(|(index, s)| {
            Line::styled(
                sanitize(s).into_owned(),
                tone_style(palette, tones.get(&index).copied().unwrap_or(Tone::Normal)),
            )
        })
        .collect();
    if let Some(selected) = selected_line {
        let scroll = selected
            .saturating_add(1)
            .saturating_sub(usize::from(body_area.height));
        Paragraph::new(clean)
            .scroll((u16::try_from(scroll).unwrap_or(u16::MAX), 0))
            .render(body_area, buf);
    } else {
        Paragraph::new(clean)
            .wrap(Wrap { trim: false })
            .render(body_area, buf);
    }
    if let Screen::Review { title, rows } = &view.screen {
        let review_area = Rect::new(
            area.x,
            area.y.saturating_add(3),
            area.width,
            body_area.height.saturating_sub(3),
        );
        crate::settings::draw_review(rows, title, review_area, buf);
    }
    if notice_height > 0
        && let Some(notice) = &view.notice
    {
        Paragraph::new(sanitize(&notice.text).into_owned())
            .style(tone_style(palette, notice.tone))
            .render(
                Rect::new(area.x, area.y + area.height - 2, area.width, 1),
                buf,
            );
    }
    if footer_height > 0 {
        let footer = view
            .footer
            .iter()
            .map(|(key, label)| format!("{key} {label}"))
            .collect::<Vec<_>>()
            .join(" · ");
        Paragraph::new(sanitize(&footer).into_owned()).render(
            Rect::new(area.x, area.y + area.height - 1, area.width, 1),
            buf,
        );
    }
}

fn tone_style(palette: &StatusPalette, tone: Tone) -> ratatui::style::Style {
    palette.style(match tone {
        Tone::Normal => StatusTone::Text,
        Tone::Warning => StatusTone::Warning,
        Tone::Error => StatusTone::Error,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::style::{Color, Style};
    use talaria_core::connectors::{Line as CoreLine, Tone};

    fn view(screen: Screen) -> ConnectorsView {
        ConnectorsView {
            title: "Title".into(),
            tab: ConnectorsTab::Messaging,
            screen,
            notice: None,
            footer: vec![("esc".into(), "close".into())],
        }
    }
    fn row(buf: &Buffer, y: u16) -> String {
        (0..buf.area.width).map(|x| buf[(x, y)].symbol()).collect()
    }
    #[test]
    fn secret_mask_is_bounded_and_title_is_not_duplicated() {
        let area = Rect::new(0, 0, 80, 12);
        let mut buf = Buffer::empty(area);
        let v = view(Screen::SecretEditor {
            key: "KEY".into(),
            title: "Secret (KEY)".into(),
            help: None,
        });
        draw_connectors(&v, &BTreeMap::from([("KEY".into(), 50)]), area, &mut buf);
        assert_eq!(row(&buf, 3).trim(), "Secret (KEY)");
        assert_eq!(row(&buf, 4).trim(), format!("{}…", "•".repeat(40)));
    }
    #[test]
    fn notices_keep_footer_and_use_resolved_palette() {
        let area = Rect::new(0, 0, 80, 12);
        let mut buf = Buffer::empty(area);
        let mut v = view(Screen::PlatformCard {
            lines: vec![CoreLine {
                text: "warning".into(),
                tone: Tone::Warning,
            }],
            fields: vec![],
            pairing: vec![],
            selected: 0,
        });
        v.notice = Some(CoreLine {
            text: "error".into(),
            tone: Tone::Error,
        });
        draw_connectors(&v, &BTreeMap::new(), area, &mut buf);
        assert_eq!(buf[(0, 3)].fg, Color::Yellow);
        assert_eq!(buf[(0, 10)].fg, Color::Red);
        assert!(row(&buf, 11).contains("esc close"));
        let plain = StatusPalette::from_styles([Style::new(); 7]);
        draw_connectors_with_palette(&v, &BTreeMap::new(), &plain, area, &mut buf);
        assert_eq!(buf[(0, 3)].fg, Color::Reset);
        assert_eq!(buf[(0, 10)].fg, Color::Reset);
    }
}
