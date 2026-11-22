use ratatui::style::Style;
use ratatui::text::{Line, Span};
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

#[derive(Clone)]
pub(super) struct Piece {
    pub text: String,
    pub style: Style,
    pub literal: bool,
}

impl Piece {
    pub fn new(text: impl Into<String>, style: Style, literal: bool) -> Self {
        Self {
            text: text.into(),
            style,
            literal,
        }
    }
}

#[derive(Clone, Copy, PartialEq)]
enum Kind {
    Text,
    Space,
    Break,
}

struct Unit<'a> {
    text: &'a str,
    style: Style,
    width: usize,
    kind: Kind,
}

pub(super) fn append(spans: &mut Vec<Span<'static>>, text: &str, style: Style) {
    if text.is_empty() {
        return;
    }
    if let Some(last) = spans.last_mut()
        && last.style == style
    {
        last.content.to_mut().push_str(text);
    } else {
        spans.push(Span::styled(text.to_owned(), style));
    }
}

pub(super) fn columns(spans: &[Span<'_>]) -> usize {
    spans
        .iter()
        .map(|span| {
            span.content
                .graphemes(true)
                .map(UnicodeWidthStr::width)
                .sum::<usize>()
        })
        .sum()
}

pub(super) fn wrap(pieces: &[Piece], width: usize) -> Vec<Line<'static>> {
    let width = width.max(1);
    let mut units = Vec::new();
    for piece in pieces {
        for grapheme in piece.text.graphemes(true) {
            let kind = if grapheme == "\n" {
                Kind::Break
            } else if grapheme == " " && !piece.literal {
                Kind::Space
            } else {
                Kind::Text
            };
            if kind == Kind::Space
                && units
                    .last()
                    .is_some_and(|unit: &Unit<'_>| unit.kind == Kind::Space)
            {
                continue;
            }
            // A tab is four independent spaces, so narrow code can still be cut.
            if grapheme == "\t" {
                for _ in 0..4 {
                    units.push(Unit {
                        text: " ",
                        style: piece.style,
                        width: 1,
                        kind: Kind::Text,
                    });
                }
            } else {
                units.push(Unit {
                    text: grapheme,
                    style: piece.style,
                    width: UnicodeWidthStr::width(grapheme),
                    kind,
                });
            }
        }
    }
    let mut output = Vec::new();
    let mut spans = Vec::new();
    let mut used = 0;
    let mut pending_space = None;
    let mut index = 0;
    while index < units.len() {
        let unit = &units[index];
        if unit.kind == Kind::Break {
            output.push(Line::from(std::mem::take(&mut spans)));
            used = 0;
            pending_space = None;
            index += 1;
            continue;
        }
        if unit.kind == Kind::Space {
            if !spans.is_empty() {
                pending_space = Some(unit.style);
            }
            index += 1;
            continue;
        }
        let end = (index..units.len())
            .find(|&i| units[i].kind != Kind::Text)
            .unwrap_or(units.len());
        let word_width: usize = units[index..end].iter().map(|unit| unit.width).sum();
        let separator = usize::from(pending_space.is_some());
        if !spans.is_empty() && used + separator + word_width > width {
            output.push(Line::from(std::mem::take(&mut spans)));
            used = 0;
            pending_space = None;
        }
        if let Some(style) = pending_space.take() {
            append(&mut spans, " ", style);
            used += 1;
        }
        for unit in &units[index..end] {
            if !spans.is_empty() && used + unit.width > width {
                output.push(Line::from(std::mem::take(&mut spans)));
                used = 0;
            }
            append(&mut spans, unit.text, unit.style);
            used += unit.width;
        }
        index = end;
    }
    if !spans.is_empty() {
        output.push(Line::from(spans));
    }
    output
}

pub(super) fn hard(spans: Vec<Span<'static>>, width: usize) -> Vec<Line<'static>> {
    let pieces: Vec<_> = spans
        .into_iter()
        .map(|span| Piece::new(span.content.into_owned(), span.style, true))
        .collect();
    let mut result = wrap(&pieces, width);
    if result.is_empty() {
        result.push(Line::default());
    }
    result
}

/// What a continued code row starts with, so copied rows are recognisable as one line.
pub(super) const CONTINUATION: &str = "↪ ";

/// Wrap one line of a code block. A line wider than `width` breaks after a space when one
/// sits in the last third of the row, and otherwise at the width. Every continuation row
/// starts with the dim `marker` and then the line's own indent.
pub(super) fn code(spans: Vec<Span<'static>>, width: usize, marker: Style) -> Vec<Line<'static>> {
    if spans.iter().any(|span| span.content.contains('\n')) {
        return hard(spans, width);
    }
    let width = width.max(1);
    let mut units: Vec<(&str, Style, usize)> = Vec::new();
    for span in &spans {
        for grapheme in span.content.graphemes(true) {
            if grapheme == "\t" {
                units.extend([(" ", span.style, 1); 4]);
            } else {
                units.push((grapheme, span.style, UnicodeWidthStr::width(grapheme)));
            }
        }
    }
    let row = |units: &[(&str, Style, usize)]| {
        let mut spans = Vec::new();
        for (text, style, _) in units {
            append(&mut spans, text, *style);
        }
        Line::from(spans)
    };
    if units.iter().map(|unit| unit.2).sum::<usize>() <= width {
        return vec![row(&units)];
    }
    let marker_columns = UnicodeWidthStr::width(CONTINUATION);
    let indent = units.iter().take_while(|unit| unit.0 == " ").count();
    let hanging = indent.min(width.saturating_sub(marker_columns) / 2);
    let indent_style = units.first().map_or(Style::default(), |unit| unit.1);
    let mut output = Vec::new();
    let mut start = 0;
    while start < units.len() {
        let first = start == 0;
        let room = if first {
            width
        } else {
            width - marker_columns - hanging
        };
        let mut used = 0;
        let mut end = start;
        while end < units.len() && (end == start || used + units[end].2 <= room) {
            used += units[end].2;
            end += 1;
        }
        let mut next = end;
        if end < units.len() {
            let floor = room - room / 3;
            let mut before = 0;
            let mut found = None;
            for (offset, unit) in units[start..end].iter().enumerate() {
                before += unit.2;
                if unit.0 == " "
                    && before >= floor
                    && units[start + offset + 1..].iter().any(|u| u.0 != " ")
                {
                    found = Some(start + offset + 1);
                }
            }
            if let Some(found) = found {
                next = found;
            }
        }
        let mut line = Vec::new();
        if !first {
            append(&mut line, CONTINUATION, marker);
            append(&mut line, &" ".repeat(hanging), indent_style);
        }
        for (text, style, _) in &units[start..next] {
            append(&mut line, text, *style);
        }
        output.push(Line::from(line));
        start = next;
        // Spaces that trail the whole line show nothing; do not give them a row of their own.
        if units[start..].iter().all(|unit| unit.0 == " ") {
            break;
        }
    }
    output
}
