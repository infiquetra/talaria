use super::{columns, put};
use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Style},
};
use talaria_core::cron::ListView;

pub(super) fn draw(view: &ListView, area: Rect, buf: &mut Buffer) {
    let mut row = 0;
    if let Some(filter) = &view.filter {
        put(
            buf,
            area,
            row,
            &format!("Filter: {filter}"),
            Style::default(),
        );
        row += 1;
    }
    if let Some(status) = &view.status {
        put(buf, area, row, status, Style::default());
        row += 1;
    }
    let width = usize::from(area.width);
    let widths = if width >= 100 {
        vec![28, 22, 36, width - 89]
    } else {
        vec![width.saturating_sub(44), 18, 24]
    };
    let headers = ["Name", "Schedule", "State", "Last run"];
    put(
        buf,
        area,
        row,
        &columns(
            &headers
                .iter()
                .zip(&widths)
                .map(|(v, w)| (*v, *w))
                .collect::<Vec<_>>(),
        ),
        Style::default().fg(Color::Cyan),
    );
    row += 1;
    let capacity = usize::from(area.height.saturating_sub(row));
    let start = view
        .selected
        .unwrap_or(0)
        .saturating_sub(capacity.saturating_sub(1));
    for (index, job) in view.rows.iter().enumerate().skip(start).take(capacity) {
        let values = [&job.name, &job.schedule, &job.state, &job.last_run];
        let line = columns(
            &values
                .iter()
                .zip(&widths)
                .map(|(v, w)| (v.as_str(), *w))
                .collect::<Vec<_>>(),
        );
        let style = if view.selected == Some(index) {
            Style::default().bg(Color::DarkGray)
        } else {
            Style::default()
        };
        put(buf, area, row, &line, style);
        row += 1;
    }
}
