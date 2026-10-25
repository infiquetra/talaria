//! The mention list, drawn above the composer.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use talaria_core::attachments::MentionView;
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

use crate::history::sanitize;
use crate::live::LiveSection;

/// One draw of a [`MentionView`].
pub struct MentionSection<'a> {
    view: &'a MentionView,
}

impl<'a> MentionSection<'a> {
    /// Draw `view`.
    #[must_use]
    pub fn new(view: &'a MentionView) -> Self {
        Self { view }
    }
}

impl LiveSection for MentionSection<'_> {
    fn height(&self, width: u16) -> u16 {
        if width == 0 {
            return 0;
        }
        (self.view.rows.len().min(8) + usize::from(self.view.status.is_some()) + 1) as u16
    }

    fn render(&self, area: Rect, buf: &mut Buffer) {
        if area.width == 0 || area.height == 0 {
            return;
        }
        let width = usize::from(area.width);
        let status_rows = usize::from(self.view.status.is_some());
        let count = self
            .view
            .rows
            .len()
            .min(8)
            .min(usize::from(area.height).saturating_sub(status_rows + 1));
        let selected = self
            .view
            .selected
            .filter(|index| *index < self.view.rows.len());
        let first = selected
            .map_or(0, |index| index.saturating_sub(count.saturating_sub(1)))
            .min(self.view.rows.len().saturating_sub(count));
        let mut y = area.y;
        for (index, (display, meta)) in self.view.rows.iter().enumerate().skip(first).take(count) {
            let style = if Some(index) == selected {
                Style::default().add_modifier(Modifier::REVERSED)
            } else {
                Style::default()
            };
            let prefix = if Some(index) == selected {
                "› "
            } else {
                "  "
            };
            let label = format!("{prefix}{}", sanitize(display));
            let meta = sanitize(meta);
            let line = if area.width >= 40 && !meta.is_empty() {
                let meta = fit(&meta, width.saturating_sub(3));
                let meta_width = UnicodeWidthStr::width(meta.as_str());
                let label = fit(&label, width.saturating_sub(meta_width + 1));
                let gap = width.saturating_sub(UnicodeWidthStr::width(label.as_str()) + meta_width);
                Line::from(vec![
                    Span::styled(label, style),
                    Span::styled(" ".repeat(gap), style),
                    Span::styled(meta, style.add_modifier(Modifier::DIM)),
                ])
            } else {
                Line::styled(fit(&label, width), style)
            };
            buf.set_line(area.x, y, &line, area.width);
            y += 1;
        }
        if let Some(status) = &self.view.status
            && y < area.bottom().saturating_sub(1)
        {
            buf.set_line(
                area.x,
                y,
                &Line::from(fit(&sanitize(status), width)),
                area.width,
            );
            y += 1;
        }
        if y < area.bottom() {
            buf.set_line(
                area.x,
                y,
                &Line::from(fit(&sanitize(self.view.footer), width)),
                area.width,
            );
        }
    }
}

/// Clip to terminal columns without splitting a grapheme or wrapping a row.
fn fit(text: &str, width: usize) -> String {
    if UnicodeWidthStr::width(text) <= width {
        return text.to_string();
    }
    if width == 0 {
        return String::new();
    }
    let mut out = String::new();
    let mut used = 0;
    for cluster in text.graphemes(true) {
        let cells = UnicodeWidthStr::width(cluster);
        if used + cells > width - 1 {
            break;
        }
        out.push_str(cluster);
        used += cells;
    }
    out.push('…');
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn selection_scrolls_in_a_bounded_safe_list() {
        let view = MentionView {
            rows: (0..30)
                .map(|index| {
                    (
                        format!("row {index}{}\u{202e}", char::from(27)),
                        "folder".into(),
                    )
                })
                .collect(),
            selected: Some(29),
            status: None,
            footer: "tab complete · enter insert · esc close",
        };
        let section = MentionSection::new(&view);
        assert_eq!(section.height(80), 9);
        assert_eq!(section.height(0), 0);
        let area = Rect::new(0, 0, 80, 9);
        let mut buffer = Buffer::empty(area);
        section.render(area, &mut buffer);
        assert_eq!(buffer[(0, 7)].symbol(), "›");
        let row: String = (0..80).map(|x| buffer[(x, 7)].symbol()).collect();
        assert!(row.contains("row 29␛‹U+202E›"));
        assert!(buffer[(0, 7)].modifier.contains(Modifier::REVERSED));
        assert!(buffer[(79, 7)].modifier.contains(Modifier::DIM));
        for width in [0, 1, 2, 36, 40] {
            for height in [0, 1, 2, 4] {
                let area = Rect::new(0, 0, width, height);
                section.render(area, &mut Buffer::empty(area));
            }
        }
    }

    #[test]
    fn clipping_uses_terminal_columns() {
        assert_eq!(fit("会話x", 4), "会…");
        assert_eq!(fit("e\u{301}xyz", 3), "e\u{301}x…");
        assert_eq!(fit("word", 1), "…");
        assert_eq!(fit("word", 0), "");
        assert_eq!(fit("word", 4), "word");
    }
}
