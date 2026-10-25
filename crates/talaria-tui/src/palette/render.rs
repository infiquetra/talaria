//! Draw the slash palette. Issue #223.
//!
//! The layout is stateless: the window of rows is worked out from the rows and the selection
//! on every frame, and only the buffer area it is given is touched.

use std::ops::Range;

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use talaria_core::commands::{CommandSource, PaletteRow, PaletteState, RowKind};
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

use super::PaletteSection;
use crate::live::LiveSection;
use crate::markdown::sanitize_untrusted;

/// Selectable rows the window may hold.
const MAX_SELECTABLE: usize = 8;
/// Widest the name column grows, in cells.
const NAME_CAP: usize = 28;
/// Below this width the tag is dropped.
const TAG_MIN_WIDTH: u16 = 80;
/// Below this width the description is dropped as well.
const DESCRIPTION_MIN_WIDTH: u16 = 50;
/// At or above this width the footer shows the key hints.
const HINT_MIN_WIDTH: u16 = 60;
/// Tag of a row the connection refused.
const REFUSED_TAG: &str = "not available here";
/// Key hints at the left of the footer.
const KEY_HINT: &str =
    "\u{2191}\u{2193} choose \u{b7} Tab complete \u{b7} Enter run \u{b7} Esc close";

/// The draw used by [`LiveSection::render`]. Issue #223.
pub fn render_palette(section: &PaletteSection, area: Rect, buf: &mut Buffer) {
    let state = section.state();
    let height = palette_height(state, section.terminal_rows());
    let lines = height.min(area.height);
    if lines == 0 || area.width == 0 {
        return;
    }
    for y in area.y..area.y.saturating_add(lines) {
        for x in area.x..area.x.saturating_add(area.width) {
            buf[(x, y)].reset();
        }
    }

    let list_lines = usize::from(lines) - 1;
    let window = window(&state.rows, state.selected, list_lines);
    let name_column = name_column(&state.rows, area.width);
    for (offset, index) in window.take(list_lines).enumerate() {
        let y = area.y + u16::try_from(offset).unwrap_or(0);
        let row = &state.rows[index];
        draw_row(buf, area, y, row, name_column);
        if index == state.selected && row.kind == RowKind::Command {
            buf.set_style(
                Rect::new(area.x, y, area.width, 1),
                Style::new().add_modifier(Modifier::REVERSED),
            );
        }
    }
    draw_footer(buf, area, area.y + lines - 1, state);
}

impl LiveSection for PaletteSection {
    fn height(&self, _width: u16) -> u16 {
        palette_height(self.state(), self.terminal_rows())
    }

    fn render(&self, area: Rect, buf: &mut Buffer) {
        render_palette(self, area, buf);
    }
}

/// Lines the palette wants: the window plus the footer, within a third of the terminal.
fn palette_height(state: &PaletteState, terminal_rows: u16) -> u16 {
    if !state.open || state.rows.is_empty() {
        return 0;
    }
    let cap = (terminal_rows / 3).max(3);
    let list_lines = window(&state.rows, state.selected, usize::from(cap) - 1).len();
    u16::try_from(list_lines + 1).unwrap_or(cap).min(cap)
}

/// The run of rows to draw: at most [`MAX_SELECTABLE`] selectable rows and `max_lines` lines.
/// It starts at the first row, unless the selection lies beyond it; then it ends at the
/// selection and reaches back as far as the limits allow.
fn window(rows: &[PaletteRow], selected: usize, max_lines: usize) -> Range<usize> {
    let mut selectable = 0;
    let mut end = 0;
    for row in rows.iter().take(max_lines) {
        if row.kind == RowKind::Command {
            if selectable == MAX_SELECTABLE {
                break;
            }
            selectable += 1;
        }
        end += 1;
    }
    if selected < end || selected >= rows.len() {
        return 0..end;
    }

    let mut start = selected + 1;
    selectable = usize::from(rows[selected].kind == RowKind::Command);
    while start > 0 && selected + 1 - start < max_lines {
        let is_command = rows[start - 1].kind == RowKind::Command;
        if is_command && selectable == MAX_SELECTABLE {
            break;
        }
        selectable += usize::from(is_command);
        start -= 1;
    }
    start..selected + 1
}

/// Width of the name column: the longest command name in the whole list, capped.
fn name_column(rows: &[PaletteRow], width: u16) -> usize {
    let longest = rows
        .iter()
        .filter(|row| row.kind == RowKind::Command)
        .map(|row| cell_width(&clean(&row.name)))
        .max()
        .unwrap_or(0);
    longest
        .min(NAME_CAP)
        .min(usize::from(width).saturating_sub(2))
}

fn draw_row(buf: &mut Buffer, area: Rect, y: u16, row: &PaletteRow, name_column: usize) {
    let width = usize::from(area.width);
    let text_width = width.saturating_sub(2);
    let x = area.x.saturating_add(2);
    match row.kind {
        RowKind::Header => {
            let style = Style::new().add_modifier(Modifier::BOLD | Modifier::DIM);
            let text = fit(&clean(&row.name), text_width);
            buf.set_stringn(x, y, &text, text_width, style);
        }
        RowKind::Hint | RowKind::Status => {
            let style = Style::new().add_modifier(Modifier::DIM);
            let text = fit(&clean(&row.name), text_width);
            buf.set_stringn(x, y, &text, text_width, style);
        }
        RowKind::Command => draw_command(buf, area, y, row, name_column),
    }
}

fn draw_command(buf: &mut Buffer, area: Rect, y: u16, row: &PaletteRow, name_column: usize) {
    let width = usize::from(area.width);
    let name = fit(&clean(&row.name), name_column);
    buf.set_stringn(
        area.x + 2,
        y,
        &name,
        name_column,
        Style::new().add_modifier(Modifier::BOLD),
    );

    let tag = if area.width >= TAG_MIN_WIDTH {
        tag_for(row)
    } else {
        None
    };
    let tag_start = tag.map(|(text, _)| width.saturating_sub(1 + cell_width(text)));
    if area.width >= DESCRIPTION_MIN_WIDTH {
        let description_start = 2 + name_column + 2;
        let end = tag_start.map_or(width, |start| start.saturating_sub(2));
        let room = end.saturating_sub(description_start);
        if room > 0 {
            let text = fit(&clean(&row.description), room);
            let x = area.x + u16::try_from(description_start).unwrap_or(0);
            buf.set_stringn(x, y, &text, room, Style::new());
        }
    }
    if let (Some((text, style)), Some(start)) = (tag, tag_start) {
        let x = area.x + u16::try_from(start).unwrap_or(0);
        buf.set_stringn(x, y, text, width - start, style);
    }
    if row.refused {
        buf.set_style(
            Rect::new(area.x, y, area.width, 1),
            Style::new().add_modifier(Modifier::DIM),
        );
    }
}

/// The tag text and style of a command row. An argument item has no source and no tag.
fn tag_for(row: &PaletteRow) -> Option<(&str, Style)> {
    let dim = Style::new().add_modifier(Modifier::DIM);
    if row.refused {
        return Some((REFUSED_TAG, dim));
    }
    match row.source.as_ref()? {
        CommandSource::Talaria => Some(("talaria", Style::new().fg(Color::Cyan))),
        other => Some((other.tag(), dim)),
    }
}

fn draw_footer(buf: &mut Buffer, area: Rect, y: u16, state: &PaletteState) {
    let width = usize::from(area.width);
    let dim = Style::new().add_modifier(Modifier::DIM);
    if let Some(text) = &state.footer {
        buf.set_stringn(area.x, y, fit(&clean(text), width), width, dim);
        return;
    }
    let shown = state
        .rows
        .iter()
        .filter(|row| row.kind == RowKind::Command)
        .count();
    if area.width < HINT_MIN_WIDTH {
        buf.set_stringn(area.x, y, format!("{shown}/{}", state.total), width, dim);
        return;
    }
    let count = format!("{shown} of {}", state.total);
    let start = width.saturating_sub(cell_width(&count));
    buf.set_stringn(
        area.x,
        y,
        fit(KEY_HINT, start.saturating_sub(1)),
        width,
        dim,
    );
    let x = area.x + u16::try_from(start).unwrap_or(0);
    buf.set_stringn(x, y, &count, width - start, dim);
}

/// Hermes text made safe to lay out: control and bidirectional characters become visible
/// stand-ins, and the line breaks and tabs left over become spaces.
fn clean(text: &str) -> String {
    sanitize_untrusted(text)
        .chars()
        .map(|ch| if ch == '\n' || ch == '\t' { ' ' } else { ch })
        .collect()
}

fn cell_width(text: &str) -> usize {
    UnicodeWidthStr::width(text)
}

/// `text` cut with `…` so that it takes at most `max` cells.
fn fit(text: &str, max: usize) -> String {
    if cell_width(text) <= max {
        return text.to_string();
    }
    if max == 0 {
        return String::new();
    }
    let mut out = String::new();
    let mut used = 0;
    for ch in text.chars() {
        let step = UnicodeWidthChar::width(ch).unwrap_or(0);
        if used + step > max - 1 {
            break;
        }
        used += step;
        out.push(ch);
    }
    out.push('\u{2026}');
    out
}
