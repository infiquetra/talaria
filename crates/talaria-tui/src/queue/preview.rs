//! Indicator, preview, and the composer title for a queue.
//!
//! Issue 232. Queue and steer.

use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use talaria_core::queue::{QueueView, QueuedRow, RowStatus};

use crate::history::sanitize;
use crate::tool_cell::{cut_to, spaces, text_width};

/// The preview shows at most this many lines.
const PREVIEW_LINES: usize = 5;
/// The selection window shows at most this many rows.
const WINDOW_ROWS: usize = 8;
/// From this width the label spells `queued`; narrower it is `q(n)`.
const WIDE: u16 = 60;
/// From this width the hint line is written out in full.
const FULL_HINT: u16 = 100;
/// The label always sits at least this far from the text it follows.
const GAP: usize = 2;

const SELECT_HINT: &str = " · ↑ select";
const STEERING_PREFIX: &str = "↳ steering into the running turn: ";
const LOST_ROW: &str = "⚠ lost in a gateway restart · d to discard";
const HINT_FULL: &str =
    "enter edit · d remove · s steer · n send now (stops the turn) · K/J move · esc back";
const HINT_MID: &str = "enter edit · d remove · s steer · n now · K/J move · esc";
const HINT_NARROW: &str = "⏎ e · d · s · n · K/J · esc";

/// The right-aligned count. No label when `count` is 0.
///
/// Issue 232. Queue and steer.
#[must_use]
pub fn render_indicator(count: usize, paused: bool, width: u16) -> Span<'static> {
    Span::styled(indicator_text(count, paused, width), dim())
}

/// At most five dimmed preview lines, or at most ten selection lines.
///
/// Issue 232. Queue and steer. `show_select_hint` adds ` · ↑ select` at 60 columns or more.
#[must_use]
pub fn render_preview(view: &QueueView, width: u16, show_select_hint: bool) -> Vec<Line<'static>> {
    let columns = usize::from(width);
    let mut label = indicator_text(view.rows.len(), view.paused, width);
    let selecting = view.selected.is_some() && view.editing.is_none();
    if show_select_hint && !selecting && width >= WIDE && !label.is_empty() {
        label.push_str(SELECT_HINT);
    }

    // The label takes the end of the first line, so that line's text gives way for it.
    let reserve = if label.is_empty() {
        0
    } else {
        text_width(&label) + GAP
    };
    let mut lines: Vec<Line<'static>> = Vec::new();
    if let Some(newest) = view.steering.last() {
        let more = view.steering.len() - 1;
        let suffix = if more > 0 {
            format!(" (+{more} more)")
        } else {
            String::new()
        };
        lines.push(body_line(
            STEERING_PREFIX,
            &first_line(newest),
            &suffix,
            dim(),
            columns.saturating_sub(reserve),
        ));
    }

    if selecting {
        push_selection(&mut lines, view, columns, reserve);
    } else {
        push_preview(&mut lines, view, columns, reserve);
    }

    if let Some(first) = lines.first_mut() {
        add_label(first, &label, columns);
    }
    lines
}

/// Composer title while an edit is open, or `Sending…` while a submit is unconfirmed.
///
/// Issue 232. Queue and steer.
#[must_use]
pub fn composer_title(view: &QueueView) -> Option<String> {
    if let Some((index, total)) = view.editing {
        return Some(format!(
            "editing queued prompt {index} of {total} · enter save · esc cancel"
        ));
    }
    (view.sending > 0).then(|| "Sending…".to_string())
}

fn dim() -> Style {
    Style::default().add_modifier(Modifier::DIM)
}

fn indicator_text(count: usize, paused: bool, width: u16) -> String {
    if count == 0 {
        return String::new();
    }
    match (width >= WIDE, paused) {
        (true, true) => format!("queued ({count}) · paused"),
        (true, false) => format!("queued ({count})"),
        (false, true) => format!("q({count})!"),
        (false, false) => format!("q({count})"),
    }
}

/// Rows from the head, then `+m more`, in at most five lines with the steering line.
fn push_preview(lines: &mut Vec<Line<'static>>, view: &QueueView, columns: usize, reserve: usize) {
    let room = PREVIEW_LINES.saturating_sub(lines.len());
    let rows = &view.rows;
    let shown = if rows.len() <= room {
        rows.len()
    } else {
        room.saturating_sub(1)
    };
    for row in &rows[..shown] {
        let room = room_for(lines, columns, reserve);
        lines.push(row_line("", row, dim(), room));
    }
    if shown < rows.len() {
        let rest = rows.len() - shown;
        let room = room_for(lines, columns, reserve);
        lines.push(body_line("", &format!("+{rest} more"), "", dim(), room));
    }
}

/// A window of up to eight rows around the selection, then the hint line.
fn push_selection(
    lines: &mut Vec<Line<'static>>,
    view: &QueueView,
    columns: usize,
    reserve: usize,
) {
    let total = view.rows.len();
    let selected = view.selected.unwrap_or(0).min(total.saturating_sub(1));
    // The window only moves when the selection would leave it.
    let start = selected.saturating_sub(WINDOW_ROWS - 1);
    let end = (start + WINDOW_ROWS).min(total);
    for (index, row) in view.rows.iter().enumerate().take(end).skip(start) {
        if index == selected {
            let style = Style::default().add_modifier(Modifier::BOLD);
            let room = room_for(lines, columns, reserve);
            lines.push(row_line("› ", row, style, room));
        } else {
            let room = room_for(lines, columns, reserve);
            lines.push(row_line("  ", row, dim(), room));
        }
    }
    let above = start;
    let below = total - end;
    let suffix = if above > 0 || below > 0 {
        format!(" (+{above} above, +{below} below)")
    } else {
        String::new()
    };
    let hint = if columns >= usize::from(FULL_HINT) {
        HINT_FULL
    } else if columns >= usize::from(WIDE) {
        HINT_MID
    } else {
        HINT_NARROW
    };
    let room = room_for(lines, columns, reserve);
    lines.push(body_line("", hint, &suffix, dim(), room));
}

/// Columns for the next line: the first line gives up the label's columns.
fn room_for(lines: &[Line<'static>], columns: usize, reserve: usize) -> usize {
    if lines.is_empty() {
        columns.saturating_sub(reserve)
    } else {
        columns
    }
}

fn row_line(prefix: &str, row: &QueuedRow, style: Style, columns: usize) -> Line<'static> {
    if row.status == RowStatus::Unknown {
        return body_line(prefix, LOST_ROW, "", style, columns);
    }
    let extra = row.text.lines().count().saturating_sub(1);
    let suffix = if extra > 0 {
        format!(" (+{extra} lines)")
    } else {
        String::new()
    };
    body_line(prefix, &first_line(row), &suffix, style, columns)
}

/// The first line of a row's text with controls and direction marks made visible.
fn first_line(row: &QueuedRow) -> String {
    let first = row.text.lines().next().unwrap_or("");
    sanitize(first).into_owned()
}

/// `prefix`, `text` and `suffix` in one line. The text gives way with `…` when they do not
/// fit, and the suffix stays whole. The line leaves room for a label at the end of the line.
fn body_line(
    prefix: &str,
    text: &str,
    suffix: &str,
    style: Style,
    columns: usize,
) -> Line<'static> {
    let fixed = text_width(prefix) + text_width(suffix);
    let body = if fixed + text_width(text) <= columns {
        format!("{prefix}{text}{suffix}")
    } else if columns > fixed + 1 {
        format!("{prefix}{}{suffix}", cut_to(text, columns - fixed))
    } else {
        cut_to(&format!("{prefix}{text}{suffix}"), columns)
    };
    Line::from(Span::styled(body, style))
}

/// Right-align `label` on `line`, which was already cut to leave room for it.
fn add_label(line: &mut Line<'static>, label: &str, columns: usize) {
    if label.is_empty() {
        return;
    }
    let used: usize = line
        .spans
        .iter()
        .map(|span| text_width(&span.content))
        .sum();
    let pad = columns.saturating_sub(used + text_width(label));
    line.spans.push(Span::styled(spaces(pad), dim()));
    line.spans.push(Span::styled(label.to_string(), dim()));
}
