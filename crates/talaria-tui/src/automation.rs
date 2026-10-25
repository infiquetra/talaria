//! The automation section and presented command cells.
//!
//! Behavior contract: issue 236. Detail and tail carry Hermes text, so each is made single-line
//! and sanitised before it is measured; labels are fixed text.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use talaria_core::automation::{AutomationRow, AutomationState, HermesCommandCell, Tone, rows};
use unicode_width::UnicodeWidthChar;

use crate::live::LiveSection;
use crate::markdown::{MarkdownOptions, render_markdown, sanitize_untrusted};

const SEPARATOR: &str = " · ";
const ELLIPSIS: &str = "…";

/// The automation section's colours.
#[derive(Clone, Debug)]
pub struct AutomationTheme {
    /// The accent colour.
    pub accent: Style,
    /// The warning colour.
    pub warning: Style,
    /// The dim modifier.
    pub dim: Style,
}

impl Default for AutomationTheme {
    fn default() -> Self {
        Self {
            accent: Style::default().fg(Color::Cyan),
            warning: Style::default().fg(Color::Yellow),
            dim: Style::default().add_modifier(Modifier::DIM),
        }
    }
}

/// The standing automation rows above the composer.
pub struct AutomationSection<'a> {
    /// The state to draw.
    pub state: &'a AutomationState,
    /// The clock in epoch milliseconds.
    pub now_ms: u64,
    /// The colours to draw with.
    pub theme: &'a AutomationTheme,
}

impl LiveSection for AutomationSection<'_> {
    fn height(&self, _width: u16) -> u16 {
        u16::try_from(rows(self.state, self.now_ms).len()).unwrap_or(u16::MAX)
    }

    fn render(&self, area: Rect, buf: &mut Buffer) {
        for (offset, row) in rows(self.state, self.now_ms).iter().enumerate() {
            let Ok(offset) = u16::try_from(offset) else {
                break;
            };
            if offset >= area.height {
                break;
            }
            let line = row_line(row, usize::from(area.width), self.theme);
            buf.set_line(area.x, area.y + offset, &line, area.width);
        }
    }
}

/// One row fitted to `width` cells: the tail is cut first; when the label and detail alone do
/// not fit, the row is cut at `width - 1` cells and ends with `…`.
fn row_line(row: &AutomationRow, width: usize, theme: &AutomationTheme) -> Line<'static> {
    let tone = match row.tone {
        Tone::Accent => theme.accent,
        Tone::Warning => theme.warning,
    };
    let mut head = vec![Span::styled(
        row.label.clone(),
        tone.add_modifier(Modifier::BOLD),
    )];
    let detail = untrusted_line(&row.detail);
    if !detail.is_empty() {
        head.push(Span::styled(SEPARATOR, theme.dim));
        head.push(Span::styled(detail, theme.dim));
    }
    let tail = untrusted_line(&row.tail);
    let head_width: usize = head.iter().map(|span| text_width(&span.content)).sum();
    let separator = text_width(SEPARATOR);
    if tail.is_empty() || head_width + separator + text_width(&tail) <= width {
        if !tail.is_empty() {
            head.push(Span::raw(SEPARATOR));
            head.push(Span::raw(tail));
        }
        return cut_spans(head, width);
    }
    if head_width + separator < width {
        let room = width - head_width - separator;
        head.push(Span::raw(SEPARATOR));
        head.push(Span::raw(format!("{}{ELLIPSIS}", cut(&tail, room - 1))));
        return Line::from(head);
    }
    head.push(Span::raw(SEPARATOR));
    head.push(Span::raw(tail));
    cut_spans(head, width)
}

/// Spans unchanged when they fit; otherwise the first `width - 1` cells and `…`.
fn cut_spans(spans: Vec<Span<'static>>, width: usize) -> Line<'static> {
    let total: usize = spans.iter().map(|span| text_width(&span.content)).sum();
    if total <= width {
        return Line::from(spans);
    }
    let Some(mut room) = width.checked_sub(1) else {
        return Line::default();
    };
    let mut out = Vec::new();
    for span in spans {
        if room == 0 {
            break;
        }
        let kept = cut(&span.content, room);
        room -= text_width(&kept);
        out.push(Span::styled(kept, span.style));
    }
    out.push(Span::raw(ELLIPSIS));
    Line::from(out)
}

/// The longest prefix of `text` at most `room` cells wide.
fn cut(text: &str, room: usize) -> String {
    let mut used = 0;
    let mut out = String::new();
    for ch in text.chars() {
        let width = ch.width().unwrap_or(0);
        if used + width > room {
            break;
        }
        used += width;
        out.push(ch);
    }
    out
}

fn text_width(text: &str) -> usize {
    text.chars().map(|ch| ch.width().unwrap_or(0)).sum()
}

/// Hermes text as one line: every run of `\r`, `\n` and `\t` becomes one space, then control
/// and direction-override characters become visible stand-ins.
fn untrusted_line(text: &str) -> String {
    let mut single = String::with_capacity(text.len());
    let mut in_break = false;
    for ch in text.chars() {
        if matches!(ch, '\r' | '\n' | '\t') {
            if !in_break {
                single.push(' ');
            }
            in_break = true;
        } else {
            single.push(ch);
            in_break = false;
        }
    }
    sanitize_untrusted(&single).into_owned()
}

/// The lines of a presented command cell at `width`.
///
/// The typed line, dim bold, then the reply rendered as Markdown two cells in. A warning reply's
/// first glyph is drawn in the warning colour.
pub fn hermes_command_lines(
    cell: &HermesCommandCell,
    width: u16,
    theme: &AutomationTheme,
) -> Vec<Line<'static>> {
    let typed = Span::styled(
        untrusted_line(&cell.typed),
        theme.dim.add_modifier(Modifier::BOLD),
    );
    let mut lines = vec![cut_spans(vec![typed], usize::from(width))];
    let options = MarkdownOptions {
        width: width.saturating_sub(2).max(1),
        ..MarkdownOptions::default()
    };
    let mut warned = !cell.warning;
    for body in render_markdown(&cell.body, &options) {
        let mut spans = vec![Span::raw("  ")];
        for span in body.spans {
            if warned {
                spans.push(span);
            } else {
                warned = warn_first_glyph(span, theme.warning, &mut spans);
            }
        }
        lines.push(Line::from(spans).style(body.style));
    }
    lines
}

/// Push `span`, drawing its first non-space character in `warning`. False when the span held
/// only spaces, so the glyph is still ahead.
fn warn_first_glyph(span: Span<'static>, warning: Style, out: &mut Vec<Span<'static>>) -> bool {
    let content = span.content.as_ref();
    let Some((index, glyph)) = content.char_indices().find(|(_, ch)| !ch.is_whitespace()) else {
        out.push(span);
        return false;
    };
    let after = index + glyph.len_utf8();
    if index > 0 {
        out.push(Span::styled(content[..index].to_owned(), span.style));
    }
    out.push(Span::styled(
        content[index..after].to_owned(),
        span.style.patch(warning),
    ));
    if after < content.len() {
        out.push(Span::styled(content[after..].to_owned(), span.style));
    }
    true
}
