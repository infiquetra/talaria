//! Model roles overlay.

mod page;
mod route_picker;

pub use page::draw_model_roles;
pub use route_picker::draw_route_picker;
pub use talaria_core::model_roles::{RolesKey, Section};

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use talaria_core::model_roles::{RolesView, RowView};
use unicode_segmentation::UnicodeSegmentation;

use crate::history::{grapheme_width, sanitize};
use crate::keys::{Key, KeyCode, KeyContext, KeyKind, KeyMap};

/// The marker that cuts a long value at the column edge.
const ELLIPSIS: &str = "...";

/// What one visible line of a scrolling list shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Slot {
    /// The row at this index.
    Row(usize),
    /// A line standing in for this many rows cut off above, itself included.
    Above(usize),
    /// A line standing in for this many rows cut off below, itself included.
    Below(usize),
}

/// The page's key for a terminal key. While the picker, a filter or an input is open, plain
/// characters and Backspace go to it as typed text; every other key goes through the bindings.
#[must_use]
pub fn route_model_roles(view: &RolesView, map: &KeyMap, key: &Key) -> Option<RolesKey> {
    if key.kind == KeyKind::Release {
        return None;
    }
    let typing = view.picker.is_some() || view.raw_keys;
    let plain = !(key.mods.ctrl || key.mods.alt || key.mods.super_key || key.mods.other);
    if typing && plain {
        match key.code {
            KeyCode::Char(c) => return Some(RolesKey::Char(c)),
            KeyCode::Backspace => return Some(RolesKey::Backspace),
            _ => {}
        }
    }
    let context = if view.picker.is_some() {
        KeyContext::RoutePicker
    } else {
        KeyContext::ModelRoles
    };
    map.resolve_model_roles(context, key)
}

fn columns(text: &str) -> u16 {
    text.graphemes(true).fold(0_u16, |width, piece| {
        width.saturating_add(grapheme_width(piece))
    })
}

/// Cut text to `width` columns, ending in `...` when something was dropped.
fn cut(text: &str, width: u16) -> String {
    if columns(text) <= width {
        return text.to_owned();
    }
    if usize::from(width) <= ELLIPSIS.len() {
        return ".".repeat(usize::from(width));
    }
    let keep = width - ELLIPSIS.len() as u16;
    let mut result = String::new();
    let mut used = 0_u16;
    for piece in text.graphemes(true) {
        let next = used.saturating_add(grapheme_width(piece));
        if next > keep {
            break;
        }
        result.push_str(piece);
        used = next;
    }
    result.push_str(ELLIPSIS);
    result
}

/// The last `width` columns of the text, for an input that is longer than its box.
fn tail(text: &str, width: u16) -> String {
    let mut pieces: Vec<&str> = Vec::new();
    let mut used = 0_u16;
    for piece in text.graphemes(true).rev() {
        let next = used.saturating_add(grapheme_width(piece));
        if next > width {
            break;
        }
        pieces.push(piece);
        used = next;
    }
    pieces.iter().rev().copied().collect()
}

/// Sanitise, cut and pad one line, then paint it with `style` across the whole width.
fn paint(text: &str, x: u16, y: u16, width: u16, style: Style, buf: &mut Buffer) {
    if width == 0 {
        return;
    }
    let mut line = cut(&sanitize(text), width);
    let used = columns(&line);
    line.push_str(&" ".repeat(usize::from(width.saturating_sub(used))));
    buf.set_stringn(x, y, line, usize::from(width), style);
}

/// Wrap text at spaces to `width` columns. A word wider than a line is cut.
fn wrap_words(text: &str, width: u16) -> Vec<String> {
    let width = width.max(1);
    let mut lines: Vec<String> = Vec::new();
    let mut current = String::new();
    for word in text.split(' ') {
        if current.is_empty() {
            current = word.to_owned();
        } else if columns(&current) + 1 + columns(word) > width {
            lines.push(std::mem::take(&mut current));
            current = word.to_owned();
        } else {
            current.push(' ');
            current.push_str(word);
        }
    }
    lines.push(current);
    lines
        .into_iter()
        .map(|line| cut(line.trim_end(), width))
        .collect()
}

/// Choose the lines of a list `height` lines tall that keep the selected row visible.
/// When rows are cut off, the first visible line says how many are above and the last how
/// many are below.
fn window(total: usize, selected: Option<usize>, height: usize) -> Vec<Slot> {
    if height == 0 || total == 0 {
        return Vec::new();
    }
    if total <= height {
        return (0..total).map(Slot::Row).collect();
    }
    let selected = selected.unwrap_or(0).min(total - 1);
    if height < 3 {
        let start = (selected + 1).saturating_sub(height);
        return (start..start + height).map(Slot::Row).collect();
    }
    let start = selected.saturating_sub(height / 2).min(total - height);
    let end = start + height;
    let mut slots: Vec<Slot> = (start..end).map(Slot::Row).collect();
    if start > 0 {
        slots[0] = Slot::Above(start + 1);
    }
    if end < total {
        slots[height - 1] = Slot::Below(total - end + 1);
    }
    slots
}

/// Paint a scrolling list of rows into `area`. The selected row is reversed in the pane that
/// has focus and bold in the other; a dimmed row is dimmed.
fn paint_rows(rows: &[RowView], focused: bool, area: Rect, buf: &mut Buffer) {
    let marked = rows.iter().any(|row| row.marker.is_some());
    let selected = rows.iter().position(|row| row.selected);
    let slots = window(rows.len(), selected, usize::from(area.height));
    for (offset, slot) in slots.into_iter().enumerate() {
        let y = area
            .y
            .saturating_add(u16::try_from(offset).unwrap_or(u16::MAX));
        match slot {
            Slot::Row(index) => {
                let row = &rows[index];
                let mut text = String::from(if row.selected { "> " } else { "  " });
                if marked {
                    text.push(row.marker.unwrap_or(' '));
                    text.push(' ');
                }
                text.push_str(&row.text);
                let mut style = Style::default();
                if row.dimmed {
                    style = style.add_modifier(Modifier::DIM);
                }
                if row.selected {
                    style = style.add_modifier(if focused {
                        Modifier::REVERSED
                    } else {
                        Modifier::BOLD
                    });
                }
                paint(&text, area.x, y, area.width, style, buf);
            }
            Slot::Above(count) => paint(
                &format!("\u{2191} {count} more"),
                area.x,
                y,
                area.width,
                Style::default().add_modifier(Modifier::DIM),
                buf,
            ),
            Slot::Below(count) => paint(
                &format!("\u{2193} {count} more"),
                area.x,
                y,
                area.width,
                Style::default().add_modifier(Modifier::DIM),
                buf,
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn window_keeps_the_selected_row_and_counts_the_cut_rows() {
        assert_eq!(
            window(3, Some(2), 5),
            vec![Slot::Row(0), Slot::Row(1), Slot::Row(2)]
        );
        let slots = window(20, Some(19), 6);
        assert_eq!(slots[0], Slot::Above(15));
        assert_eq!(slots[5], Slot::Row(19));
        let slots = window(20, Some(0), 6);
        assert_eq!(slots[0], Slot::Row(0));
        assert_eq!(slots[5], Slot::Below(15));
        let slots = window(20, Some(10), 6);
        assert!(slots.contains(&Slot::Row(10)));
        assert!(matches!(slots[0], Slot::Above(_)));
        assert!(matches!(slots[5], Slot::Below(_)));
        for height in 1..8 {
            for selected in 0..20 {
                assert!(
                    window(20, Some(selected), height).contains(&Slot::Row(selected)),
                    "height {height} selected {selected}"
                );
            }
        }
    }

    #[test]
    fn cut_ends_in_dots_and_wrap_keeps_words() {
        assert_eq!(cut("abcdefghij", 8), "abcde...");
        assert_eq!(cut("abc", 8), "abc");
        assert_eq!(
            wrap_words("one two  three four", 9),
            vec!["one two", "three", "four"]
        );
    }
}
