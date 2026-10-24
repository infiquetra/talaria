use pulldown_cmark::Alignment;
use ratatui::style::Style;
use ratatui::text::{Line, Span};

use super::wrap::{self, Piece};

pub(super) type Cell = Vec<Piece>;

pub(super) fn render(
    headers: &[Cell],
    rows: &[Vec<Cell>],
    alignments: &[Alignment],
    width: usize,
    border: Style,
    base: Style,
) -> Vec<Line<'static>> {
    if headers.is_empty() {
        return Vec::new();
    }
    if rows.is_empty() {
        return headers
            .iter()
            .flat_map(|cell| wrap::wrap(cell, width))
            .collect();
    }
    let natural: Vec<_> = (0..headers.len())
        .map(|index| {
            std::iter::once(&headers[index])
                .chain(rows.iter().filter_map(|row| row.get(index)))
                .map(|cell| {
                    wrap::wrap(cell, usize::MAX)
                        .iter()
                        .map(|line| wrap::columns(&line.spans))
                        .max()
                        .unwrap_or(0)
                })
                .max()
                .unwrap_or(1)
                .max(1)
        })
        .collect();
    let available = width as i64 - (3 * headers.len() + 1) as i64;
    let minimum: Vec<_> = natural.iter().map(|&value| value.min(10)).collect();
    let minimum_sum: usize = minimum.iter().sum();
    if available <= 0 || available < minimum_sum as i64 {
        return fallback(headers, rows, width, base);
    }
    let available = available as usize;
    let widths = if natural.iter().sum::<usize>() <= available {
        natural.clone()
    } else {
        let mut widths = minimum;
        let remaining = available - minimum_sum;
        let demand: usize = natural
            .iter()
            .zip(&widths)
            .map(|(natural, minimum)| natural - minimum)
            .sum();
        for (index, value) in widths.iter_mut().enumerate() {
            *value += remaining * (natural[index] - *value) / demand;
        }
        let mut left = available - widths.iter().sum::<usize>();
        while left > 0 {
            for (index, value) in widths.iter_mut().enumerate() {
                if left > 0 && *value < natural[index] {
                    *value += 1;
                    left -= 1;
                }
            }
        }
        widths
    };
    let mut output = vec![border_line(&widths, '┌', '┬', '┐', border)];
    output.extend(row(headers, &widths, alignments, border, base));
    output.push(border_line(&widths, '├', '┼', '┤', border));
    for cells in rows {
        output.extend(row(cells, &widths, alignments, border, base));
    }
    output.push(border_line(&widths, '└', '┴', '┘', border));
    output
}

fn border_line(
    widths: &[usize],
    left: char,
    middle: char,
    right: char,
    style: Style,
) -> Line<'static> {
    let mut text = left.to_string();
    for (index, width) in widths.iter().enumerate() {
        if index > 0 {
            text.push(middle);
        }
        text.push_str(&"─".repeat(width + 2));
    }
    text.push(right);
    Line::from(Span::styled(text, style))
}

fn row(
    cells: &[Cell],
    widths: &[usize],
    alignments: &[Alignment],
    border: Style,
    base: Style,
) -> Vec<Line<'static>> {
    let wrapped: Vec<_> = widths
        .iter()
        .enumerate()
        .map(|(index, width)| {
            cells
                .get(index)
                .map(|cell| wrap::wrap(cell, *width))
                .unwrap_or_default()
        })
        .collect();
    let height = wrapped.iter().map(Vec::len).max().unwrap_or(1).max(1);
    (0..height)
        .map(|y| {
            let mut spans = vec![Span::styled("│ ", border)];
            for (index, lines) in wrapped.iter().enumerate() {
                if index > 0 {
                    wrap::append(&mut spans, " │ ", border);
                }
                let cell = lines.get(y);
                let padding =
                    widths[index].saturating_sub(cell.map_or(0, |line| wrap::columns(&line.spans)));
                let left = match alignments.get(index) {
                    Some(Alignment::Right) => padding,
                    Some(Alignment::Center) => padding / 2,
                    _ => 0,
                };
                let padding_style = cell
                    .and_then(|line| line.spans.first())
                    .map_or(base, |span| span.style);
                wrap::append(&mut spans, &" ".repeat(left), padding_style);
                if let Some(cell) = cell {
                    for span in &cell.spans {
                        wrap::append(&mut spans, &span.content, span.style);
                    }
                }
                wrap::append(&mut spans, &" ".repeat(padding - left), padding_style);
            }
            wrap::append(&mut spans, " │", border);
            Line::from(spans)
        })
        .collect()
}

fn fallback(headers: &[Cell], rows: &[Vec<Cell>], width: usize, base: Style) -> Vec<Line<'static>> {
    let mut output = Vec::new();
    for (row_index, cells) in rows.iter().enumerate() {
        if row_index > 0 {
            output.push(Line::default());
        }
        for (index, header) in headers.iter().enumerate() {
            let mut label: Vec<Span<'static>> = wrap::wrap(header, usize::MAX)
                .into_iter()
                .flat_map(|line| line.spans)
                .collect();
            wrap::append(&mut label, ": ", base);
            let prefix_width = wrap::columns(&label);
            let cell = cells.get(index).cloned().unwrap_or_default();
            if prefix_width >= width {
                let mut pieces = header.clone();
                pieces.push(Piece::new(": ", base, false));
                pieces.extend(cell);
                output.extend(wrap::wrap(&pieces, width));
            } else {
                let mut lines = wrap::wrap(&cell, width - prefix_width);
                if lines.is_empty() {
                    lines.push(Line::default());
                }
                for (line_index, line) in lines.into_iter().enumerate() {
                    let mut spans = if line_index == 0 {
                        label.clone()
                    } else {
                        vec![Span::styled(" ".repeat(prefix_width), base)]
                    };
                    spans.extend(line.spans);
                    // Empty values do not need the label's trailing separator space.
                    if line_index == 0
                        && spans
                            .last()
                            .is_some_and(|span| span.content.ends_with(": "))
                        && let Some(last) = spans.last_mut()
                    {
                        last.content.to_mut().pop();
                    }
                    output.push(Line::from(spans));
                }
            }
        }
    }
    output
}
