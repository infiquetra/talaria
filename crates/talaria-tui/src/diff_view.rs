//! Diff rows for a tool cell. Issue 221.

use ratatui::style::Style;
use ratatui::text::{Line, Span};
use talaria_core::diff::{DiffModel, DiffRow, RowKind};

use crate::history::sanitize;
use crate::tool_cell::{ToolTheme, WrapRows, cut_force, cut_to, plain_line, spaces};

/// How to draw one diff.
pub struct DiffRenderOptions {
    /// Columns available for the whole row, including the indent.
    pub width: u16,
    /// Spaces before the line number.
    pub indent: u16,
    /// Most diff rows to draw. Absent means every row.
    pub max_rows: Option<usize>,
    /// Colours and weight.
    pub theme: ToolTheme,
    /// When false, colour is removed and bold and dim stay.
    pub color: bool,
}

/// Draw the diff. The second value is how many diff rows were not drawn.
///
/// Model notes are not drawn here; the caller places them after any more-lines row.
#[must_use]
pub fn diff_lines(model: &DiffModel, options: &DiffRenderOptions) -> (Vec<Line<'static>>, usize) {
    let total: usize = model
        .files
        .iter()
        .flat_map(|file| file.hunks.iter())
        .map(|hunk| hunk.rows.len())
        .sum();
    let widest = digits(max_number(model));
    let mut laid = layout(model, options, widest);
    if laid.shown < total {
        let narrower = digits(laid.max_number);
        if narrower < widest {
            laid = layout(model, options, narrower);
        }
    }
    let mut lines = laid.lines;
    if !options.color {
        for line in &mut lines {
            plain_line(line);
        }
    }
    (lines, total - laid.shown)
}

/// Rows built for one pass, and what they showed.
struct Laid {
    lines: Vec<Line<'static>>,
    shown: usize,
    max_number: u32,
}

/// Layout constants shared by every row of one pass.
struct Frame<'a> {
    width: usize,
    indent: usize,
    number_width: usize,
    /// The prefix is `indent + number_width + 3` columns; narrow means no room is left for text.
    narrow: bool,
    theme: &'a ToolTheme,
}

impl Frame<'_> {
    fn prefix_columns(&self) -> usize {
        self.indent + self.number_width + 3
    }

    /// Columns for the text of a wrapped row.
    fn text_columns(&self) -> usize {
        if self.narrow {
            self.width.saturating_sub(1)
        } else {
            self.width - self.prefix_columns()
        }
    }
}

fn digits(number: u32) -> usize {
    number.max(1).to_string().len()
}

/// The numbers a row shows: new for added and context rows, old for removed rows.
fn number_of(row: &DiffRow) -> Option<u32> {
    match row.kind {
        RowKind::Added | RowKind::Context => row.new_no,
        RowKind::Removed => row.old_no,
        RowKind::Note => None,
    }
}

/// The largest line number over every row.
fn max_number(model: &DiffModel) -> u32 {
    model
        .files
        .iter()
        .flat_map(|file| file.hunks.iter())
        .flat_map(|hunk| hunk.rows.iter())
        .filter_map(number_of)
        .max()
        .unwrap_or(0)
}

fn layout(model: &DiffModel, options: &DiffRenderOptions, number_width: usize) -> Laid {
    let width = usize::from(options.width.max(1));
    let indent = usize::from(options.indent);
    let mut frame = Frame {
        width,
        indent,
        number_width,
        narrow: false,
        theme: &options.theme,
    };
    frame.narrow = width <= frame.prefix_columns();
    let many = model.files.len() > 1;
    let mut lines: Vec<Line<'static>> = Vec::new();
    let mut pending: Vec<Line<'static>> = Vec::new();
    let mut shown = 0usize;
    let mut max_shown = 0u32;
    let mut stopped = false;
    'files: for (file_index, file) in model.files.iter().enumerate() {
        if many {
            let path = sanitize(file.display_path().unwrap_or(""));
            pending.push(fixed_row(
                &format!("{}{path}", spaces(indent)),
                options.theme.diff_file,
                width,
            ));
        }
        for (hunk_index, hunk) in file.hunks.iter().enumerate() {
            if file_index > 0 || hunk_index > 0 {
                let header = sanitize(&hunk.header);
                let text = format!("{}{}   {header}", spaces(indent), spaces(number_width));
                pending.push(fixed_row(&text, options.theme.diff_hunk, width));
            }
            for row in &hunk.rows {
                let budget = options.max_rows.map(|max| max.saturating_sub(lines.len()));
                let room = budget.map(|budget| budget.saturating_sub(pending.len()));
                let (rows, complete) = row_lines(row, &frame, room.map(|room| room + 1));
                let fits = complete && room.is_none_or(|room| rows.len() <= room);
                if fits {
                    lines.append(&mut pending);
                    lines.extend(rows);
                    shown += 1;
                    if let Some(number) = number_of(row) {
                        max_shown = max_shown.max(number);
                    }
                    continue;
                }
                if shown == 0 {
                    show_first_row_cut(row, &frame, options.max_rows, &mut pending, &mut lines);
                    shown += usize::from(!lines.is_empty());
                    if let Some(number) = number_of(row) {
                        max_shown = max_shown.max(number);
                    }
                }
                stopped = true;
                break 'files;
            }
        }
    }
    if !stopped
        && options
            .max_rows
            .is_none_or(|max| lines.len() + pending.len() <= max)
    {
        lines.append(&mut pending);
    }
    Laid {
        lines,
        shown,
        max_number: max_shown,
    }
}

/// The very first diff row is longer than the whole allowance: show its first rows, the last
/// ending in `…`. It counts as shown. Headers before it stay only when they leave room.
fn show_first_row_cut(
    row: &DiffRow,
    frame: &Frame<'_>,
    max_rows: Option<usize>,
    pending: &mut Vec<Line<'static>>,
    lines: &mut Vec<Line<'static>>,
) {
    let Some(max) = max_rows else { return };
    if max <= pending.len() {
        return;
    }
    let room = max - pending.len();
    let (mut rows, complete) = row_lines(row, frame, Some(room));
    if !complete {
        cut_last_text(&mut rows, frame.text_columns());
    }
    lines.append(pending);
    lines.extend(rows);
}

/// End the last row's text with `…` within `columns`.
fn cut_last_text(rows: &mut [Line<'static>], columns: usize) {
    let Some(last) = rows.last_mut() else { return };
    if last.spans.len() < 2 {
        return;
    }
    if let Some(span) = last.spans.last_mut() {
        span.content = cut_force(&span.content, columns).into();
    }
}

/// A row that is cut to the width and never wrapped.
fn fixed_row(text: &str, style: Style, width: usize) -> Line<'static> {
    Line::from(Span::styled(cut_to(text, width), style))
}

/// The rows of one diff row, at most `limit` of them when a limit is given. The second value
/// is true when every row of the text was built.
fn row_lines(row: &DiffRow, frame: &Frame<'_>, limit: Option<usize>) -> (Vec<Line<'static>>, bool) {
    let theme = frame.theme;
    let (style, sign) = match row.kind {
        RowKind::Added => (theme.diff_added, '+'),
        RowKind::Removed => (theme.diff_removed, '-'),
        RowKind::Context => (theme.diff_context, ' '),
        RowKind::Note => (theme.diff_note, ' '),
    };
    let text = sanitize(&row.text);
    let mut out: Vec<Line<'static>> = Vec::new();
    if frame.narrow {
        if frame.width < 2 {
            out.push(Line::from(Span::styled(sign.to_string(), style)));
            return (out, true);
        }
        let mut wrap = WrapRows::new(&text, frame.width - 1);
        while limit.is_none_or(|limit| out.len() < limit) {
            let Some(piece) = wrap.next() else { break };
            let lead = if out.is_empty() { sign } else { ' ' };
            out.push(Line::from(vec![
                Span::styled(lead.to_string(), style),
                Span::styled(piece.into_owned(), style),
            ]));
        }
        return (out, !wrap.has_more());
    }
    let blank = spaces(frame.prefix_columns());
    let mut wrap = WrapRows::new(&text, frame.text_columns());
    while limit.is_none_or(|limit| out.len() < limit) {
        let Some(piece) = wrap.next() else { break };
        let mut spans = Vec::with_capacity(5);
        if out.is_empty() && row.kind != RowKind::Note {
            let number = number_of(row).map_or_else(
                || spaces(frame.number_width),
                |number| format!("{number:>width$}", width = frame.number_width),
            );
            spans.push(Span::raw(spaces(frame.indent)));
            spans.push(Span::styled(number, theme.line_number));
            spans.push(Span::styled(format!(" {sign} "), style));
        } else {
            spans.push(Span::raw(blank.clone()));
        }
        spans.push(Span::styled(piece.into_owned(), style));
        out.push(Line::from(spans));
    }
    (out, !wrap.has_more())
}
