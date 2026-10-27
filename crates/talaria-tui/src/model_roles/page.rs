//! Draw the model roles page into a buffer.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::widgets::{Block, Clear, Widget};
use talaria_core::model_roles::{DiffLine, ModalView, RolesView, RowView};

use super::{Slot, columns, cut, paint, paint_rows, tail, window, wrap_words};
use crate::history::sanitize;

/// The most footer lines the page keeps, however many the view holds.
const FOOTER_LINES: usize = 4;
/// The widest a dialog grows, borders included.
const DIALOG_WIDTH: u16 = 80;
/// Shown at the foot when the view carries no footer of its own, so the page never leaves a
/// person without a way to act or to leave.
const FALLBACK_FOOTER: &str = "Enter open  Esc close  ? keys";
/// The tab label suffix for a section whose last read failed.
const STALE: &str = "(stale)";

/// Draw the page: title, tabs, notices, the section (or the preset list and its detail pane),
/// the footer, and any dialog or picker on top.
pub fn draw_model_roles(view: &RolesView, area: Rect, buf: &mut Buffer) {
    let area = area.intersection(buf.area);
    if area.is_empty() {
        return;
    }
    Clear.render(area, buf);
    let margin = u16::from(area.width > 2);
    let x = area.x + margin;
    let width = area.width.saturating_sub(2 * margin);
    paint(&view.title, x, area.y, width, Style::default().bold(), buf);
    if area.height < 4 {
        return;
    }
    draw_tabs(view, x, area.y + 1, width, buf);

    let footer = footer_lines(view, width, area.height);
    let footer_top = area.bottom() - u16::try_from(footer.len()).unwrap_or(0);
    for (offset, line) in footer.iter().enumerate() {
        let y = footer_top + u16::try_from(offset).unwrap_or(0);
        paint(line, x, y, width, Style::default(), buf);
    }

    let room = footer_top.saturating_sub(area.y + 2).saturating_sub(4);
    let mut notice: Vec<String> = view
        .notice
        .iter()
        .flat_map(|text| wrap_words(&sanitize(text), width))
        .collect();
    notice.truncate(usize::from(room));
    for (offset, line) in notice.iter().enumerate() {
        let y = area.y + 2 + u16::try_from(offset).unwrap_or(0);
        paint(line, x, y, width, Style::default(), buf);
    }
    let body_top = area.y + 2 + u16::try_from(notice.len()).unwrap_or(0) + 1;
    let body = Rect::new(
        x,
        body_top,
        width,
        footer_top.saturating_sub(1).saturating_sub(body_top),
    );
    draw_body(view, body, buf);

    let page = Rect::new(x, area.y, width, area.height);
    if let Some(picker) = &view.picker {
        draw_picker_over(picker, area, buf);
    }
    if let Some(modal) = &view.modal {
        draw_modal(modal, page, buf);
    }
}

fn draw_tabs(view: &RolesView, x: u16, y: u16, width: u16, buf: &mut Buffer) {
    let end = x.saturating_add(width);
    let mut at = x;
    for (index, tab) in view.tabs.iter().enumerate() {
        if index > 0 {
            at = at.saturating_add(2);
        }
        let remaining = end.saturating_sub(at);
        if remaining == 0 {
            break;
        }
        let mut label = sanitize(&tab.label).into_owned();
        if tab.stale && !label.contains(STALE) {
            label.push(' ');
            label.push_str(STALE);
        }
        let label = cut(&label, remaining);
        let style = if tab.current {
            Style::default().add_modifier(Modifier::REVERSED)
        } else {
            Style::default()
        };
        buf.set_stringn(at, y, &label, usize::from(remaining), style);
        at = at.saturating_add(columns(&label));
    }
}

/// The footer wrapped to the width, kept short enough to leave room for the page.
fn footer_lines(view: &RolesView, width: u16, height: u16) -> Vec<String> {
    let mut lines: Vec<String> = view
        .footer
        .iter()
        .flat_map(|text| wrap_words(&sanitize(text), width))
        .collect();
    if lines.is_empty() {
        lines.push(FALLBACK_FOOTER.to_owned());
    }
    lines.truncate(FOOTER_LINES.min(usize::from(height.saturating_sub(4)) / 2));
    lines
}

fn draw_body(view: &RolesView, body: Rect, buf: &mut Buffer) {
    if body.is_empty() {
        return;
    }
    let Some(detail) = &view.detail else {
        paint_rows(&view.rows, true, body, buf);
        return;
    };
    let left = (body.width / 3)
        .clamp(18, 36)
        .min(body.width.saturating_sub(8));
    let right_x = body.x + left + 3;
    let right = Rect::new(
        right_x,
        body.y,
        body.right().saturating_sub(right_x),
        body.height,
    );
    paint_rows(
        &view.rows,
        !view.focus_detail,
        Rect::new(body.x, body.y, left, body.height),
        buf,
    );
    for y in body.y..body.bottom() {
        buf.set_stringn(
            body.x + left + 1,
            y,
            "\u{2502}",
            1,
            Style::default().add_modifier(Modifier::DIM),
        );
    }
    paint_rows(detail, view.focus_detail, right, buf);
}

fn draw_picker_over(
    picker: &talaria_core::model_roles::RoutePickerView,
    area: Rect,
    buf: &mut Buffer,
) {
    let inset = u16::from(area.width > 8 && area.height > 8) * 2;
    let outer = Rect::new(
        area.x + inset,
        area.y + inset,
        area.width - 2 * inset,
        area.height - 2 * inset,
    );
    let block = Block::bordered();
    let inner = block.inner(outer);
    Clear.render(outer, buf);
    block.render(outer, buf);
    super::draw_route_picker(picker, inner, buf);
}

/// A centred box with a border, cleared first. Returns the room inside its padding.
fn dialog_box(title: Option<&str>, content: usize, page: Rect, buf: &mut Buffer) -> Rect {
    let width = page.width.min(DIALOG_WIDTH);
    let height = u16::try_from(content.saturating_add(2))
        .unwrap_or(u16::MAX)
        .min(page.height);
    let outer = Rect::new(
        page.x + (page.width - width) / 2,
        page.y + (page.height - height) / 2,
        width,
        height,
    );
    Clear.render(outer, buf);
    let mut block = Block::bordered();
    if let Some(title) = title {
        block = block.title(sanitize(title).into_owned());
    }
    let inner = block.inner(outer);
    block.render(outer, buf);
    Rect::new(
        inner.x + u16::from(inner.width > 2),
        inner.y,
        inner.width.saturating_sub(2 * u16::from(inner.width > 2)),
        inner.height,
    )
}

fn draw_modal(modal: &ModalView, page: Rect, buf: &mut Buffer) {
    // The most lines a dialog may show: the page less its border and a line above and below.
    let most = usize::from(page.height.saturating_sub(4));
    match modal {
        ModalView::Prompt { lines } => {
            let inner_width = page.width.min(DIALOG_WIDTH).saturating_sub(4);
            let mut text: Vec<String> = lines
                .iter()
                .flat_map(|line| wrap_words(&sanitize(line), inner_width))
                .collect();
            text.truncate(most);
            let inner = dialog_box(None, text.len(), page, buf);
            for (offset, line) in text.iter().enumerate() {
                let y = inner.y + u16::try_from(offset).unwrap_or(0);
                paint(line, inner.x, y, inner.width, Style::default(), buf);
            }
        }
        ModalView::Review {
            title,
            lines,
            footer,
        } => {
            let shown = lines.len().min(most.saturating_sub(1));
            let inner = dialog_box(Some(title), shown + 1, page, buf);
            let rows = diff_rows(lines);
            let slots = window(rows.len(), None, shown);
            for (offset, slot) in slots.into_iter().enumerate() {
                let y = inner.y + u16::try_from(offset).unwrap_or(0);
                match slot {
                    Slot::Row(index) => {
                        paint(&rows[index], inner.x, y, inner.width, Style::default(), buf);
                    }
                    Slot::Below(count) | Slot::Above(count) => paint(
                        &format!("\u{2193} {count} more"),
                        inner.x,
                        y,
                        inner.width,
                        Style::default().add_modifier(Modifier::DIM),
                        buf,
                    ),
                }
            }
            let y = inner.y + u16::try_from(shown).unwrap_or(0);
            paint(footer, inner.x, y, inner.width, Style::default(), buf);
        }
        ModalView::Input {
            label,
            value,
            error,
        } => {
            let inner = dialog_box(None, 1 + usize::from(error.is_some()), page, buf);
            let label = sanitize(label);
            let room = inner.width.saturating_sub(columns(&label) + 1);
            let value = tail(&sanitize(value), room);
            paint(
                &format!("{label}{value}"),
                inner.x,
                inner.y,
                inner.width,
                Style::default(),
                buf,
            );
            let cursor = inner.x + columns(&label) + columns(&value);
            if cursor < inner.right() {
                buf.set_stringn(
                    cursor,
                    inner.y,
                    " ",
                    1,
                    Style::default().add_modifier(Modifier::REVERSED),
                );
            }
            if let Some(error) = error {
                paint(
                    error,
                    inner.x,
                    inner.y + 1,
                    inner.width,
                    Style::default(),
                    buf,
                );
            }
        }
        ModalView::Choice {
            title,
            options,
            selected,
        } => {
            let rows: Vec<RowView> = options
                .iter()
                .enumerate()
                .map(|(index, option)| RowView {
                    text: option.clone(),
                    selected: index == *selected,
                    dimmed: false,
                    marker: None,
                })
                .collect();
            let shown = rows.len().min(most);
            let inner = dialog_box(Some(title), shown, page, buf);
            let list = Rect::new(
                inner.x,
                inner.y,
                inner.width,
                inner.height.min(u16::try_from(shown).unwrap_or(u16::MAX)),
            );
            paint_rows(&rows, true, list, buf);
        }
    }
}

/// One line per changed key. A line with neither a before nor an after value is a heading.
fn diff_rows(lines: &[DiffLine]) -> Vec<String> {
    lines
        .iter()
        .map(|line| {
            if line.before.is_empty() && line.after.is_empty() {
                line.key.clone()
            } else {
                format!("{}  {} -> {}", line.key, line.before, line.after)
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use talaria_core::model_roles::{RoutePickerView, TabView};

    fn view() -> RolesView {
        RolesView {
            title: "Model roles - profile fixture".to_owned(),
            tabs: vec![
                TabView {
                    label: "1 Main".to_owned(),
                    current: true,
                    stale: false,
                },
                TabView {
                    label: "2 Fallbacks".to_owned(),
                    current: false,
                    stale: true,
                },
            ],
            rows: (0..30)
                .map(|index| RowView {
                    text: format!("row {index}"),
                    selected: index == 20,
                    dimmed: false,
                    marker: None,
                })
                .collect(),
            detail: None,
            focus_detail: false,
            notice: Vec::new(),
            footer: vec!["Esc close".to_owned()],
            modal: None,
            picker: None,
            raw_keys: false,
        }
    }

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

    fn drawn(view: &RolesView, width: u16, height: u16) -> (Buffer, String) {
        let area = Rect::new(0, 0, width, height);
        let mut buf = Buffer::empty(area);
        draw_model_roles(view, area, &mut buf);
        let shown = text(&buf, area);
        (buf, shown)
    }

    #[test]
    fn the_current_tab_is_reversed_and_a_stale_tab_says_so() {
        let (buf, shown) = drawn(&view(), 80, 24);
        assert!(shown.contains("2 Fallbacks (stale)"));
        assert!(
            buf[(1, 1)]
                .style()
                .add_modifier
                .contains(Modifier::REVERSED)
        );
        assert!(
            !buf[(10, 1)]
                .style()
                .add_modifier
                .contains(Modifier::REVERSED)
        );
    }

    #[test]
    fn a_long_list_scrolls_to_the_selected_row_with_counts() {
        let (_, shown) = drawn(&view(), 80, 14);
        assert!(shown.contains("> row 20"));
        assert!(shown.contains("\u{2191} "));
        assert!(shown.contains("\u{2193} "));
    }

    #[test]
    fn dialogs_and_the_picker_draw_over_the_page() {
        let mut page = view();
        page.modal = Some(ModalView::Prompt {
            lines: vec!["Use this model anyway? y/n".to_owned()],
        });
        assert!(
            drawn(&page, 80, 24)
                .1
                .contains("Use this model anyway? y/n")
        );
        page.modal = Some(ModalView::Review {
            title: "Review changes to Main - profile fixture".to_owned(),
            lines: vec![DiffLine {
                key: "model.default".to_owned(),
                before: "a".to_owned(),
                after: "b".to_owned(),
            }],
            footer: "Enter apply  Esc back".to_owned(),
        });
        let shown = drawn(&page, 80, 24).1;
        assert!(shown.contains("model.default  a -> b"));
        assert!(shown.contains("Enter apply  Esc back"));
        page.modal = Some(ModalView::Input {
            label: "Name: ".to_owned(),
            value: "alpha".to_owned(),
            error: Some("Enter a name".to_owned()),
        });
        let shown = drawn(&page, 80, 24).1;
        assert!(shown.contains("Name: alpha"));
        assert!(shown.contains("Enter a name"));
        page.modal = Some(ModalView::Choice {
            title: "Effort".to_owned(),
            options: vec!["inherit".to_owned(), "low".to_owned()],
            selected: 1,
        });
        assert!(drawn(&page, 80, 24).1.contains("> low"));
        page.modal = None;
        page.picker = Some(RoutePickerView {
            title: "Choose a provider".to_owned(),
            filter: String::new(),
            rows: Vec::new(),
            status: None,
        });
        assert!(drawn(&page, 80, 24).1.contains("Choose a provider"));
    }

    #[test]
    fn small_areas_draw_without_panicking() {
        let mut page = view();
        page.notice = vec!["a long notice that has to wrap onto lines".to_owned(); 3];
        page.footer = vec!["a long footer line that wraps".to_owned(); 5];
        page.modal = Some(ModalView::Prompt {
            lines: vec!["Sure? y/n".to_owned(); 3],
        });
        page.picker = Some(RoutePickerView {
            title: "t".to_owned(),
            filter: String::new(),
            rows: Vec::new(),
            status: Some("s".to_owned()),
        });
        for width in 0..30 {
            for height in 0..14 {
                drawn(&page, width, height);
            }
        }
    }
}
