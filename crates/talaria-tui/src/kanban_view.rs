//! Literal board presentation; terminal modes remain owned by the terminal module.
use crate::history::sanitize;
use ratatui::{
    buffer::Buffer,
    layout::Rect,
    text::Line,
    widgets::{Clear, Paragraph, Widget},
};
use talaria_core::kanban::{KanbanBoardState, KanbanFrame};
pub struct KanbanOverlay {
    open: bool,
    inline: bool,
}
impl KanbanOverlay {
    pub fn open() -> Self {
        Self {
            open: true,
            inline: false,
        }
    }
    pub fn close(&mut self) {
        self.open = false;
        self.inline = true;
    }
    pub fn is_open(&self) -> bool {
        self.open
    }
    pub fn showing_inline(&self) -> bool {
        self.inline
    }
}
fn line(text: &str, x: u16, y: u16, width: u16, buf: &mut Buffer) {
    Paragraph::new(sanitize(text).into_owned()).render(Rect::new(x, y, width, 1), buf);
}
fn fitted(text: &str, width: usize) -> String {
    let text = sanitize(text);
    if Line::from(text.as_ref()).width() <= width {
        return text.into_owned();
    }
    let mut out = String::new();
    for c in text.chars() {
        let mut next = out.clone();
        next.push(c);
        if Line::from(next.as_str()).width() + 1 > width {
            break;
        }
        out = next;
    }
    if width > 0 {
        out.push('>');
    }
    out
}
/// The frame's optional card is the current selection; the state determines whether detail is open.
pub fn draw_kanban(frame: &KanbanFrame, area: Rect, buf: &mut Buffer) {
    draw_columns(frame, area, buf, 0);
}
fn draw_columns(frame: &KanbanFrame, area: Rect, buf: &mut Buffer, start: usize) {
    Clear.render(area, buf);
    if area.height == 0 || area.width == 0 {
        return;
    }
    line(
        &format!(
            "kanban: {}  {}/{} cards  live:{}",
            frame.board, frame.shown, frame.total, frame.live
        ),
        area.x,
        area.y,
        area.width,
        buf,
    );
    let count = frame.columns.len().max(1);
    let width = (usize::from(area.width) / count)
        .max(18)
        .min(usize::from(area.width));
    let visible = (usize::from(area.width) / width).max(1);
    for (i, col) in frame.columns.iter().skip(start).take(visible).enumerate() {
        let x = area.x + (i * width) as u16;
        if area.height > 2 {
            line(
                &format!("{} ({})", col.label, col.cards.len()),
                x,
                area.y + 1,
                width as u16,
                buf,
            );
        }
        if col.cards.is_empty() && area.height > 3 {
            line(
                if frame.total == 0 && i == 0 {
                    "(no cards on this board)"
                } else {
                    "(empty)"
                },
                x,
                area.y + 2,
                width as u16,
                buf,
            );
        }
        let room = area.height.saturating_sub(3) as usize;
        let first = col
            .cards
            .iter()
            .position(|c| c.selected)
            .unwrap_or(0)
            .saturating_sub(room.saturating_sub(1));
        for (row, card) in col.cards.iter().skip(first).take(room).enumerate() {
            let marker = if card.selected { "> " } else { "  " };
            let warning = if matches!(card.status.as_str(), "blocked" | "review") {
                "! "
            } else {
                ""
            };
            let suffix = format!(" {} {}", card.assignee, card.age);
            let available =
                width.saturating_sub(Line::from(format!("{marker}{warning}{suffix}")).width());
            let title = fitted(&card.title, available);
            line(
                &format!("{marker}{warning}{title}{suffix}"),
                x,
                area.y + 2 + row as u16,
                width as u16,
                buf,
            );
        }
    }
    if area.height > 1 {
        line(
            &frame.status_line,
            area.x,
            area.bottom() - 1,
            area.width,
            buf,
        );
    }
}
pub struct KanbanView;
impl KanbanView {
    pub fn draw(state: &KanbanBoardState, area: Rect, buf: &mut Buffer) {
        let frame = state.frame();
        let width = (usize::from(area.width) / frame.columns.len().max(1))
            .max(18)
            .min(usize::from(area.width).max(1));
        let visible = (usize::from(area.width) / width).max(1);
        let start = state.column.saturating_sub(visible.saturating_sub(1));
        draw_columns(&frame, area, buf, start);
        if area.height < 3 {
            return;
        }
        if state.loading {
            line(
                &format!("loading board {}...", state.board),
                area.x,
                area.y + 2,
                area.width,
                buf,
            );
        } else if let Some(error) = &state.error {
            line(error, area.x, area.y + 2, area.width, buf);
            if area.height > 4 {
                line(
                    "press r to retry, Esc to close",
                    area.x,
                    area.y + 3,
                    area.width,
                    buf,
                );
            }
        } else if state.detail_id.is_some() {
            let inner = Rect::new(
                area.x,
                area.y + 1,
                area.width,
                area.height.saturating_sub(2),
            );
            Clear.render(inner, buf);
            let lines = if state.detail_fields.is_empty() {
                vec!["loading card...".into()]
            } else {
                state.detail_fields.clone()
            };
            Paragraph::new(
                lines
                    .into_iter()
                    .map(|s| Line::from(sanitize(&s).into_owned()))
                    .collect::<Vec<_>>(),
            )
            .wrap(ratatui::widgets::Wrap { trim: false })
            .render(inner, buf);
        }
        let menu = state.menu_lines();
        if !menu.is_empty() {
            let height = (area.height / 2).max(1);
            let inner = Rect::new(area.x, area.bottom() - height - 1, area.width, height);
            Clear.render(inner, buf);
            let selected = match &state.mode {
                talaria_core::kanban::BoardMode::Moves { selected, .. }
                | talaria_core::kanban::BoardMode::Boards { selected, .. } => *selected,
                _ => 0,
            };
            let first = selected.saturating_sub(height.saturating_sub(1) as usize);
            for (i, text) in menu.iter().skip(first).take(height as usize).enumerate() {
                line(text, inner.x, inner.y + i as u16, inner.width, buf);
            }
        }
    }
}
