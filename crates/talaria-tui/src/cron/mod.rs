//! Projection of scheduled jobs into the terminal buffer.

use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Style},
    text::Line,
    widgets::{Paragraph, Widget, Wrap},
};
pub use talaria_core::cron::CronAction;
use talaria_core::cron::{CronBody, CronView};

mod detail;
mod form;
mod list;

#[derive(Default)]
pub struct CronFrameGate {
    last: Option<CronView>,
}
impl CronFrameGate {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }
    pub fn needs_draw(&mut self, view: &CronView) -> bool {
        if self.last.as_ref() == Some(view) {
            false
        } else {
            self.last = Some(view.clone());
            true
        }
    }
}

/// Draw the overlay without changing domain state.
pub fn draw_cron(view: &CronView, area: Rect, buf: &mut Buffer) {
    if area.width == 0 || area.height == 0 {
        return;
    }
    ratatui::widgets::Clear.render(area, buf);
    if area.width < 60 || area.height < 12 {
        Paragraph::new("Make the terminal at least 60 by 12 to manage scheduled jobs.")
            .wrap(Wrap { trim: false })
            .render(area, buf);
        return;
    }
    put(buf, area, 0, &view.title, Style::default().fg(Color::Cyan));
    let mut top = 1;
    if let Some(banner) = &view.banner {
        put(buf, area, top, banner, Style::default().fg(Color::Yellow));
        top += 1;
    }
    let question_height = view.question.as_ref().map_or(0, |text| {
        let width = usize::from(area.width);
        let words = crate::history::sanitize(text);
        let mut lines = 1usize;
        let mut used = 0usize;
        for word in words.split_whitespace() {
            let size = Line::raw(word).width();
            if used > 0 && used + 1 + size > width {
                lines += 1;
                used = 0;
            }
            used += usize::from(used > 0) + size;
        }
        u16::try_from(lines)
            .unwrap_or(u16::MAX)
            .min(area.height.saturating_sub(top + 2))
    });
    let bottom = area.height.saturating_sub(2 + question_height);
    let body = Rect::new(area.x, area.y + top, area.width, bottom.saturating_sub(top));
    match &view.body {
        CronBody::Message(text) => put(buf, body, 0, text, Style::default()),
        CronBody::List(view) => list::draw(view, body, buf),
        CronBody::Detail(view) => detail::draw(view, body, buf),
        CronBody::Form(view) => form::draw(view, body, buf),
    }
    if let Some(notice) = &view.notice {
        put(
            buf,
            area,
            bottom,
            notice,
            Style::default().fg(Color::Yellow),
        );
    }
    if let Some(question) = &view.question {
        let clean = crate::history::sanitize(question);
        Paragraph::new(clean.as_ref())
            .wrap(Wrap { trim: true })
            .style(Style::default().fg(Color::Yellow))
            .render(
                Rect::new(area.x, area.y + bottom + 1, area.width, question_height),
                buf,
            );
    }
    put(buf, area, area.height - 1, &view.hints, Style::default());
}

fn clipped(text: &str, width: usize) -> String {
    let clean = crate::history::sanitize(text.replace('\n', " ").as_str()).into_owned();
    if Line::raw(clean.as_str()).width() <= width {
        return clean;
    }
    if width == 0 {
        return String::new();
    }
    let mut result = String::new();
    for ch in clean.chars() {
        let mut candidate = result.clone();
        candidate.push(ch);
        if Line::raw(candidate.as_str()).width() > width - 1 {
            break;
        }
        result.push(ch);
    }
    result.push('…');
    result
}

fn put(buf: &mut Buffer, area: Rect, row: u16, text: &str, style: Style) {
    if row >= area.height {
        return;
    }
    let line = clipped(text, usize::from(area.width));
    Paragraph::new(line)
        .style(style)
        .render(Rect::new(area.x, area.y + row, area.width, 1), buf);
}

fn columns(values: &[(&str, usize)]) -> String {
    values
        .iter()
        .map(|(value, width)| {
            let text = clipped(value, *width);
            let pad = width.saturating_sub(Line::raw(text.as_str()).width());
            format!("{text}{}", " ".repeat(pad))
        })
        .collect::<Vec<_>>()
        .join(" ")
}
