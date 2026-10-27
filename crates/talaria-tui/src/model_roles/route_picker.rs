//! Draw the provider and model picker into a buffer.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::widgets::{Clear, Widget};
use talaria_core::model_roles::RoutePickerView;

use super::{paint, paint_rows};

/// Draw the picker: its title, the typed filter, the rows and, at the foot, a status line.
pub fn draw_route_picker(view: &RoutePickerView, area: Rect, buf: &mut Buffer) {
    let area = area.intersection(buf.area);
    if area.is_empty() {
        return;
    }
    Clear.render(area, buf);
    let margin = u16::from(area.width > 2);
    let x = area.x + margin;
    let width = area.width.saturating_sub(2 * margin);
    paint(&view.title, x, area.y, width, Style::default().bold(), buf);
    if area.height < 2 {
        return;
    }
    paint(
        &format!("> {}", view.filter),
        x,
        area.y + 1,
        width,
        Style::default(),
        buf,
    );
    let mut bottom = area.bottom();
    if let Some(status) = &view.status
        && area.height > 2
    {
        bottom -= 1;
        paint(
            status,
            x,
            bottom,
            width,
            Style::default().add_modifier(Modifier::DIM),
            buf,
        );
    }
    let top = area.y + 2;
    let list = Rect::new(x, top, width, bottom.saturating_sub(top));
    paint_rows(&view.rows, true, list, buf);
}

#[cfg(test)]
mod tests {
    use super::*;
    use talaria_core::model_roles::RowView;

    fn text(buf: &Buffer, area: Rect) -> String {
        (0..area.height)
            .map(|y| {
                (0..area.width)
                    .map(|x| buf[(x, y)].symbol())
                    .collect::<String>()
                    .trim_end()
                    .to_owned()
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    #[test]
    fn dimmed_rows_status_and_filter_are_drawn() {
        let view = RoutePickerView {
            title: "Choose a provider".to_owned(),
            filter: "sk".to_owned(),
            rows: vec![
                RowView {
                    text: "Skeleton  not set up: add it under Providers".to_owned(),
                    selected: true,
                    dimmed: true,
                    marker: None,
                },
                RowView {
                    text: "stored-model (not in the model list)".to_owned(),
                    selected: false,
                    dimmed: false,
                    marker: Some('!'),
                },
            ],
            status: Some("loading models...".to_owned()),
        };
        let area = Rect::new(0, 0, 100, 30);
        let mut buf = Buffer::empty(area);
        draw_route_picker(&view, area, &mut buf);
        let shown = text(&buf, area);
        assert!(shown.contains("Choose a provider"));
        assert!(shown.contains("> sk"));
        assert!(shown.contains("not set up: add it under Providers"));
        assert!(shown.contains("! stored-model (not in the model list)"));
        assert!(
            shown
                .lines()
                .nth(29)
                .is_some_and(|line| line.contains("loading models..."))
        );
        assert!(buf[(5, 2)].style().add_modifier.contains(Modifier::DIM));
        assert!(!buf[(5, 3)].style().add_modifier.contains(Modifier::DIM));
    }

    #[test]
    fn a_tiny_area_draws_without_panicking() {
        let view = RoutePickerView {
            title: "t".to_owned(),
            filter: String::new(),
            rows: vec![RowView {
                text: "row".to_owned(),
                selected: true,
                dimmed: false,
                marker: None,
            }],
            status: Some("s".to_owned()),
        };
        for (w, h) in [(0, 0), (1, 1), (3, 2), (5, 3), (10, 4)] {
            let area = Rect::new(0, 0, w, h);
            let mut buf = Buffer::empty(area);
            draw_route_picker(&view, area, &mut buf);
        }
    }
}
