//! The model overlay and the confirmation card. Issue 230.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::Line;
use talaria_core::models::{PickerRow, PickerView};
use talaria_core::mutation::Confirmation;
use unicode_segmentation::UnicodeSegmentation;

use crate::history::{grapheme_width, sanitize, split_lines, wrap_lines};
use crate::live::LiveSection;

/// Paint the model overlay into the alternate-screen buffer.
pub fn draw_model_picker(view: &PickerView, area: Rect, buf: &mut Buffer) {
    let area = area.intersection(buf.area);
    if area.is_empty() {
        return;
    }
    for y in area.y..area.bottom() {
        for x in area.x..area.right() {
            buf[(x, y)].reset();
        }
    }
    let x = area.x + u16::from(area.width > 1);
    let width = area.right() - x;
    let title = if view.refreshing {
        format!("{} · refreshing…", view.title)
    } else {
        view.title.to_owned()
    };
    paint(&title, x, area.y, width, Style::default().bold(), buf);
    if area.height < 2 {
        return;
    }
    let filter = if view.filter.is_empty() {
        "> type to filter".to_string()
    } else {
        format!("> {}", view.filter)
    };
    paint(&filter, x, area.y + 1, width, Style::default(), buf);
    if area.height < 3 {
        return;
    }
    let banner = view
        .banner
        .filter(|_| !view.rows.is_empty())
        .map(|text| wrap_lines(vec![Line::from(sanitize(text).into_owned())], width))
        .unwrap_or_default();
    let banner_height = banner.len().min(usize::from(area.height.saturating_sub(4)));
    let list_height = usize::from(area.height - 3).saturating_sub(banner_height);
    let top = view
        .selected
        .filter(|&selected| selected < view.rows.len())
        .map_or(0, |selected| (selected + 1).saturating_sub(list_height));
    for (offset, row) in view.rows.iter().enumerate().skip(top).take(list_height) {
        let selected = view.selected == Some(offset) && !row.header;
        let mut style = Style::default();
        if row.dim {
            style = style.add_modifier(Modifier::DIM);
        }
        if row.header {
            style = style.bold();
        }
        if selected {
            style = style.reversed();
        }
        let y = area.y + 2 + u16::try_from(offset - top).unwrap_or(0);
        let line = row_line(row, width, area.width);
        paint(&line, x, y, width, style, buf);
    }
    if view.rows.is_empty()
        && list_height > 0
        && let Some(banner) = view.banner
    {
        paint(banner, x, area.y + 2, width, Style::default().dim(), buf);
    }
    let banner_top = area.bottom() - 1 - u16::try_from(banner_height).unwrap_or(0);
    for (offset, line) in banner.iter().take(banner_height).enumerate() {
        buf.set_line(
            x,
            banner_top + u16::try_from(offset).unwrap_or(0),
            line,
            width,
        );
    }
    let effort = sanitize(view.effort);
    let hints = sanitize(view.hints);
    let footer = if view.refreshing {
        if u32::from(columns(&effort)) + 3 + u32::from(columns(&hints)) <= u32::from(width) {
            format!("{effort}   {hints}")
        } else {
            hints.into_owned()
        }
    } else if u32::from(columns(&effort)) + 3 < u32::from(width) {
        format!("{effort}   {hints}")
    } else {
        effort.into_owned()
    };
    paint(&footer, x, area.bottom() - 1, width, Style::default(), buf);
    if footer.starts_with(view.effort)
        && matches!(
            view.effort,
            "effort: could not be read" | "effort: not offered by this model"
        )
    {
        paint(
            view.effort,
            x,
            area.bottom() - 1,
            width,
            Style::default().dim(),
            buf,
        );
    }
}

fn columns(text: &str) -> u16 {
    text.graphemes(true).fold(0_u16, |width, piece| {
        width.saturating_add(grapheme_width(piece))
    })
}

fn cut(text: &str, width: u16) -> String {
    if columns(text) <= width {
        return text.to_string();
    }
    if width == 0 {
        return String::new();
    }
    let mut result = String::new();
    let mut used = 0_u16;
    for piece in text.graphemes(true) {
        let next = used.saturating_add(grapheme_width(piece));
        if next > width - 1 {
            break;
        }
        result.push_str(piece);
        used = next;
    }
    result.push('…');
    result
}

fn paint(text: &str, x: u16, y: u16, width: u16, style: Style, buf: &mut Buffer) {
    buf.set_stringn(x, y, cut(&sanitize(text), width), usize::from(width), style);
}

fn row_line(row: &PickerRow, width: u16, screen_width: u16) -> String {
    let text = sanitize(&row.text);
    let mut suffix = Vec::new();
    if screen_width > 40 {
        suffix.extend(row.badges.iter().map(|badge| sanitize(badge).into_owned()));
    }
    if screen_width > 60
        && let Some(price) = &row.price
    {
        suffix.push(sanitize(price).into_owned());
    }
    let suffix = cut(&suffix.join("  "), width.saturating_sub(2));
    if suffix.is_empty() {
        return cut(&text, width);
    }
    let text = cut(&text, width.saturating_sub(columns(&suffix) + 2));
    let padding = width.saturating_sub(columns(&text) + columns(&suffix));
    format!("{text}{}{suffix}", " ".repeat(usize::from(padding)))
}

/// The live card that asks whether to switch anyway.
pub struct ModelConfirmSection<'a> {
    pub confirmation: &'a Confirmation,
    pub keep: &'a str,
    draft_waiting: bool,
}

impl<'a> ModelConfirmSection<'a> {
    pub fn new(confirmation: &'a Confirmation, keep: &'a str) -> Self {
        Self {
            confirmation,
            keep,
            draft_waiting: false,
        }
    }

    /// The composer still has a draft, so the card does not take Enter.
    pub fn waiting_on_draft(mut self) -> Self {
        self.draft_waiting = true;
        self
    }

    fn rows(&self, width: u16) -> Vec<Line<'static>> {
        let title = format!("Switch to {}?", sanitize(&self.confirmation.target_model));
        let message = if self.confirmation.message.is_empty() {
            "Hermes asks you to confirm this switch."
        } else {
            &self.confirmation.message
        };
        let keys = if self.draft_waiting {
            "clear the draft to answer".to_string()
        } else {
            format!("enter switch anyway · esc keep {}", sanitize(self.keep))
        };
        let mut lines = vec![Line::from(title).style(Style::default().bold())];
        lines.extend(split_lines(message).map(|line| Line::from(sanitize(line).into_owned())));
        lines.push(Line::from(keys));
        wrap_lines(lines, width)
    }
}

impl LiveSection for ModelConfirmSection<'_> {
    fn height(&self, width: u16) -> u16 {
        u16::try_from(self.rows(width).len()).unwrap_or(u16::MAX)
    }

    fn render(&self, area: Rect, buf: &mut Buffer) {
        let area = area.intersection(buf.area);
        if area.is_empty() {
            return;
        }
        let rows = self.rows(area.width);
        let skip = rows.len().saturating_sub(usize::from(area.height));
        for (offset, row) in (0..area.height).zip(rows.iter().skip(skip)) {
            buf.set_line(area.x, area.y + offset, row, area.width);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use talaria_core::models::RowKey;

    #[test]
    fn a_row_refusal_is_visible_without_hiding_the_selected_row_or_footer() {
        let rows = [PickerRow {
            key: Some(RowKey::NotSetUp {
                provider: "other".into(),
            }),
            text: "Other · set up with hermes model".into(),
            badges: Vec::new(),
            price: None,
            dim: true,
            header: false,
        }];
        let text = "Other is not set up. Set it up with hermes model in a terminal, then press ctrl+r here.";
        let view = PickerView {
            title: "Switch model · this session",
            filter: "",
            effort: "effort: medium",
            hints: "esc close",
            refreshing: false,
            rows: &rows,
            selected: Some(0),
            banner: Some(text),
        };
        let area = Rect::new(0, 0, 60, 12);
        let mut buffer = Buffer::empty(area);
        draw_model_picker(&view, area, &mut buffer);
        let rendered: Vec<String> = (0..area.height)
            .map(|y| (0..area.width).map(|x| buffer[(x, y)].symbol()).collect())
            .collect();
        assert!(rendered[2].contains("Other · set up with hermes model"));
        assert!(
            rendered
                .iter()
                .any(|line| line.contains("Other is not set up."))
        );
        assert!(
            rendered
                .iter()
                .any(|line| line.contains("then press ctrl+r here."))
        );
        assert!(rendered[11].contains("effort: medium"));
    }
}
