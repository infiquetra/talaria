use super::{columns, put};
use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Style},
};
use talaria_core::cron::{CardFocus, DetailView, RunsView};

pub(super) fn draw(view: &DetailView, area: Rect, buf: &mut Buffer) {
    let accent = Style::default().fg(Color::Cyan);
    if let Some(card) = &view.run_card {
        for (row, line) in field_lines(card)
            .iter()
            .take(usize::from(area.height))
            .enumerate()
        {
            put(buf, area, row as u16, line, Style::default());
        }
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
