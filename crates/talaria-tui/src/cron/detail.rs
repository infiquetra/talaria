use super::{columns, put};
use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Style},
    text::Line,
    widgets::{Paragraph, Widget, Wrap},
};
use talaria_core::cron::{CardFocus, DetailView, RunsView};

pub(super) fn draw(view: &DetailView, area: Rect, buf: &mut Buffer) {
    let accent = Style::default().fg(Color::Cyan);
    if let Some(card) = &view.run_card {
        let text = field_lines(card)
            .into_iter()
            .map(|line| crate::history::sanitize(&line).into_owned())
            .collect::<Vec<_>>();
        let selected = view.run_scroll.min(text.len().saturating_sub(1));
        let rows_through_selection: usize = text
            .iter()
            .take(selected + 1)
            .map(|line| {
                Paragraph::new(line.as_str())
                    .wrap(Wrap { trim: false })
                    .line_count(area.width)
            })
            .sum();
        let scroll = rows_through_selection.saturating_sub(usize::from(area.height));
        let lines = text
            .into_iter()
            .enumerate()
            .map(|(index, line)| {
                let style = if index == selected {
                    Style::default().bg(Color::DarkGray)
                } else {
                    Style::default()
                };
                Line::styled(line, style)
            })
            .collect::<Vec<_>>();
        Paragraph::new(lines)
            .wrap(Wrap { trim: false })
            .scroll((u16::try_from(scroll).unwrap_or(u16::MAX), 0))
            .render(area, buf);
        return;
    }
    let lines = field_lines(&view.fields);
    let field_height = usize::from(area.height.saturating_sub(3)) / 2;
    put(
        buf,
        area,
        0,
        "Details",
        if view.focus == CardFocus::Fields {
            accent
        } else {
            Style::default()
        },
    );
    for (row, line) in lines
        .iter()
        .skip(view.fields_scroll)
        .take(field_height)
        .enumerate()
    {
        put(buf, area, row as u16 + 1, line, Style::default());
    }
    let mut row = field_height as u16 + 1;
    put(
        buf,
        area,
        row,
        "Recent runs",
        if view.focus == CardFocus::History {
            accent
        } else {
            Style::default()
        },
    );
    row += 1;
    match &view.runs {
        RunsView::Loading => put(buf, area, row, "Loading runs…", Style::default()),
        RunsView::Empty => put(buf, area, row, "No runs yet.", Style::default()),
        RunsView::Unavailable(text) | RunsView::Failed(text) => {
            put(buf, area, row, text, Style::default())
        }
        RunsView::Rows {
            pending,
            rows,
            selected,
        } => {
            if let Some(text) = pending {
                put(buf, area, row, text, Style::default());
                row += 1;
            }
            let capacity = usize::from(area.height.saturating_sub(row));
            let start = selected
                .unwrap_or(0)
                .saturating_sub(capacity.saturating_sub(1));
            for (index, run) in rows.iter().enumerate().skip(start).take(capacity) {
                let line = columns(&[
                    (&run.start, 12),
                    (&run.status, 24),
                    (&run.duration, 9),
                    (&run.tokens, 18),
                    (&run.title, usize::from(area.width).saturating_sub(67)),
                ]);
                let style = if *selected == Some(index) && view.focus == CardFocus::History {
                    Style::default().bg(Color::DarkGray)
                } else {
                    Style::default()
                };
                put(buf, area, row, &line, style);
                row += 1;
            }
        }
    }
}
fn field_lines(fields: &[(String, Vec<String>)]) -> Vec<String> {
    fields
        .iter()
        .flat_map(|(label, values)| {
            if values.is_empty() {
                vec![format!("{label}:")]
            } else {
                values
                    .iter()
                    .enumerate()
                    .map(|(i, v)| {
                        if i == 0 {
                            format!("{label}: {v}")
                        } else {
                            format!("  {v}")
                        }
                    })
                    .collect()
            }
        })
        .collect()
}
