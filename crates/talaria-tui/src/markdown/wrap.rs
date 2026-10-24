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
