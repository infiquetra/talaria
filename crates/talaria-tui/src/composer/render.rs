//! Draw the composer into a live section. Styles stay on the default colours.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

use super::buffer::row_of;
use super::wrap::{Row, display_cluster};
use super::{Prepared, VimMode};
use crate::history::sanitize;
use crate::keys::Action;
use crate::live::LiveSection;

/// One drawn cell: its text and style.
type Cell = (String, Style);

/// A prepared view of the composer for one draw.
#[derive(Debug)]
pub struct ComposerSection<'a> {
    composer: &'a super::Composer,
}

impl<'a> ComposerSection<'a> {
    pub(crate) fn new(composer: &'a super::Composer) -> Self {
        Self { composer }
    }

    fn rows(&self) -> &[Row] {
        self.composer
            .cache
            .as_ref()
            .map_or(&[][..], |cache| cache.rows.as_slice())
    }

    /// The text cells of one shown row: clusters drawn as the terminal will show them, a
    /// tab as four spaces, the caret reversed.
    fn text_cells(&self, index: usize, cells: usize) -> Vec<Cell> {
        let composer = self.composer;
        let text = composer.draft.text();
        let dim = Style::default().add_modifier(Modifier::DIM);
        let reversed = Modifier::REVERSED;
        let mut out: Vec<Cell> = Vec::new();
        if text.is_empty() {
            if composer
                .vim_mode()
                .is_some_and(|mode| mode != VimMode::Insert)
            {
                out.push((" ".to_string(), Style::default().add_modifier(reversed)));
                return out;
            }
            let placeholder = composer.placeholder();
            for (position, cluster) in placeholder.graphemes(true).enumerate() {
                if out.len() + UnicodeWidthStr::width(cluster) > cells {
                    break;
                }
                let mut style = dim;
                if position == 0 {
                    style = style.add_modifier(reversed);
                }
                push_cluster(&mut out, cluster, style);
            }
            return out;
        }
        let rows = self.rows();
        let Some(row) = rows.get(index) else {
            return out;
        };
        let caret = if composer
            .vim_mode()
            .is_some_and(|mode| mode != VimMode::Insert)
        {
            super::vim::displayed_caret(text, composer.draft.caret())
        } else {
            composer.draft.caret()
        };
        let selection = composer
            .vim_state()
            .and_then(|state| state.selection(text, caret));
        let caret_here = row_of(rows, caret) == index;
        let mut caret_drawn = false;
        for (offset, cluster) in text[row.start..row.end].grapheme_indices(true) {
            let mut style = Style::default();
            if composer.highlight_revision == composer.draft.revision() {
                let start = row.start + offset;
                for (range, highlight) in &composer.highlights {
                    if range.start <= start && start + cluster.len() <= range.end {
                        style = style.patch(*highlight);
                    }
                }
            }
            if caret_here && row.start + offset == caret {
                style = style.add_modifier(reversed);
                caret_drawn = true;
            }
            let selected = selection.as_ref().is_some_and(|range| {
                range.start <= row.start + offset && row.start + offset < range.end
            });
            if selected {
                style = style.add_modifier(reversed);
            }
            let drawn = if cluster == "\t" {
                "    ".to_string()
            } else {
                display_cluster(cluster).into_owned()
            };
            if out.len() + UnicodeWidthStr::width(drawn.as_str()) > cells {
                break;
            }
            if cluster == "\t" {
                for space in 0..4 {
                    let cell_style = if space == 0 || selected {
                        style
                    } else {
                        style.remove_modifier(reversed)
                    };
                    out.push((" ".to_string(), cell_style));
                }
            } else {
                push_cluster(&mut out, &drawn, style);
            }
        }
        if caret_here && !caret_drawn && out.len() < cells {
            out.push((" ".to_string(), Style::default().add_modifier(reversed)));
        }
        out
    }

    /// The text of the hint at the bottom: the exit hint, else the hidden rows below.
    fn bottom_hint(&self, prepared: &Prepared) -> Option<String> {
        if self.composer.exit_armed.is_some() {
            let label = self
                .composer
                .keymap
                .label(Action::Cancel)
                .unwrap_or_else(|| "Ctrl+C".to_string());
            return Some(format!("Press {label} again to exit"));
        }
        (prepared.below > 0).then(|| format!("\u{2193} {} more", prepared.below))
    }

    fn top_hint(prepared: &Prepared) -> Option<String> {
        (prepared.above > 0).then(|| format!("\u{2191} {} more", prepared.above))
    }

    fn mode_segment(&self) -> Option<(&'static str, Option<String>)> {
        if self.composer.exit_armed.is_some() {
            return None;
        }
        let state = self.composer.vim_state()?;
        let label = match state.mode() {
            VimMode::Normal => "-- NORMAL --",
            VimMode::Insert => "-- INSERT --",
            VimMode::Visual => "-- VISUAL --",
            VimMode::VisualLine => "-- VISUAL LINE --",
        };
        Some((label, state.pending_label()))
    }

    fn mode_cells(&self, room: usize) -> Option<Vec<Cell>> {
        let (mode, pending) = self.mode_segment()?;
        let dim = Style::default().add_modifier(Modifier::DIM);
        let mut cells = Vec::new();
        for cluster in mode.graphemes(true) {
            push_cluster(&mut cells, cluster, dim);
        }
        if let Some(pending) = pending
            && cells.len() + 2 + UnicodeWidthStr::width(pending.as_str()) <= room
        {
            cells.push((" ".to_string(), dim));
            cells.push((" ".to_string(), dim));
            for cluster in pending.graphemes(true) {
                push_cluster(&mut cells, cluster, Style::default());
            }
        }
        Some(cells)
    }

    fn render_boxed(&self, prepared: &Prepared, area: Rect, buf: &mut Buffer) {
        let width = usize::from(prepared.width.min(area.width));
        let interior = width.saturating_sub(2);
        let dim = Style::default().add_modifier(Modifier::DIM);
        let border = self.composer.style.border;
        let mut marker = self.composer.style.marker;
        if self.composer.draft.text().is_empty()
            && self
                .composer
                .vim_mode()
                .is_some_and(|mode| mode != VimMode::Insert)
        {
            marker = marker.add_modifier(Modifier::DIM);
        }

        let mut top = border_cells(interior, Self::top_hint(prepared).as_deref(), dim, border);
        if let Some(title) = &self.composer.title {
            let hint_cells = Self::top_hint(prepared).map_or(0, |hint| hint.chars().count() + 1);
            let room = interior.saturating_sub(1 + hint_cells);
            let mut placed: Vec<Cell> = vec![(" ".to_string(), Style::default())];
            placed.extend(fit_title(title, room));
            for (slot, cell) in top.iter_mut().zip(placed) {
                *slot = cell;
            }
        }
        let mut lines: Vec<Vec<Cell>> = Vec::new();
        lines.push(framed('\u{256D}', top, '\u{256E}', border));

        for offset in 0..prepared.shown {
            let mut line: Vec<Cell> = vec![
                ("\u{2502}".to_string(), border),
                (" ".to_string(), Style::default()),
            ];
            if offset == 0 {
                line.push((">".to_string(), marker));
                line.push((" ".to_string(), marker));
            } else {
                line.push((" ".to_string(), Style::default()));
                line.push((" ".to_string(), Style::default()));
            }
            let mut text = self.text_cells(prepared.first + offset, prepared.text_cells);
            pad(&mut text, prepared.text_cells);
            line.extend(text);
            line.push((" ".to_string(), Style::default()));
            line.push((" ".to_string(), Style::default()));
            line.push(("\u{2502}".to_string(), border));
            lines.push(line);
        }

        let mut bottom = border_cells(interior, self.bottom_hint(prepared).as_deref(), dim, border);
        if let Some(mode) = self.mode_cells(interior.saturating_sub(1)) {
            bottom = vec![("\u{2500}".to_string(), border); interior];
            if !bottom.is_empty() {
                bottom[0] = (" ".to_string(), Style::default());
            }
            for (slot, cell) in bottom.iter_mut().skip(1).zip(mode) {
                *slot = cell;
            }
        }
        lines.push(framed('\u{2570}', bottom, '\u{256F}', border));
        draw_lines(&lines, area, buf);
    }

    fn render_borderless(&self, prepared: &Prepared, area: Rect, buf: &mut Buffer) {
        let marker = self.composer.style.marker;
        let dim = Style::default().add_modifier(Modifier::DIM);
        let mut lines: Vec<Vec<Cell>> = Vec::new();
        for offset in 0..prepared.shown {
            let mut line: Vec<Cell> = if offset == 0 {
                vec![(">".to_string(), marker), (" ".to_string(), marker)]
            } else {
                vec![(" ".to_string(), Style::default()); 2]
            };
            let mut text = self.text_cells(prepared.first + offset, prepared.text_cells);
            pad(&mut text, prepared.text_cells);
            line.extend(text);
            line.push((" ".to_string(), Style::default()));
            lines.push(line);
        }
        if prepared.hint_row {
            if let Some(mode) = self.mode_cells(usize::from(prepared.width)) {
                lines.push(mode);
                draw_lines(&lines, area, buf);
                return;
            }
            let hint = match (Self::top_hint(prepared), self.bottom_hint(prepared)) {
                (Some(up), Some(down)) if self.composer.exit_armed.is_none() => {
                    format!("{up} \u{b7} {down}")
                }
                (_, Some(down)) => down,
                (Some(up), None) => up,
                (None, None) => String::new(),
            };
            let mut line = Vec::new();
            for cluster in hint.graphemes(true) {
                push_cluster(&mut line, cluster, dim);
            }
            lines.push(line);
        }
        draw_lines(&lines, area, buf);
    }
}

impl LiveSection for ComposerSection<'_> {
    fn height(&self, _width: u16) -> u16 {
        self.composer.prepared.map_or(1, |prepared| prepared.height)
    }

    fn render(&self, area: Rect, buf: &mut Buffer) {
        let Some(prepared) = self.composer.prepared else {
            return;
        };
        if prepared.boxed {
            self.render_boxed(&prepared, area, buf);
        } else {
            self.render_borderless(&prepared, area, buf);
        }
    }
}

/// Add one cluster; a wide cluster takes its cell and leaves the next one blank.
fn push_cluster(out: &mut Vec<Cell>, cluster: &str, style: Style) {
    let width = UnicodeWidthStr::width(cluster);
    if width == 0 {
        return;
    }
    out.push((cluster.to_string(), style));
    for _ in 1..width {
        out.push((String::new(), style));
    }
}

fn pad(cells: &mut Vec<Cell>, width: usize) {
    while cells.len() < width {
        cells.push((" ".to_string(), Style::default()));
    }
}

/// A border line of `interior` cells with a hint right-aligned against the corner.
fn border_cells(
    interior: usize,
    hint: Option<&str>,
    hint_style: Style,
    border: Style,
) -> Vec<Cell> {
    let mut cells: Vec<Cell> = vec![("\u{2500}".to_string(), border); interior];
    if let Some(hint) = hint {
        let hint: Vec<char> = hint.chars().collect();
        let start = interior.saturating_sub(hint.len());
        for (slot, c) in cells[start..].iter_mut().zip(hint) {
            *slot = (c.to_string(), hint_style);
        }
    }
    cells
}

/// The title cut to `room` cells, with an ellipsis when it does not fit.
fn fit_title(title: &str, room: usize) -> Vec<Cell> {
    let mut cells: Vec<Cell> = Vec::new();
    let clean = sanitize(title);
    let clusters: Vec<&str> = clean.graphemes(true).collect();
    let total: usize = clusters.iter().map(|c| UnicodeWidthStr::width(*c)).sum();
    let limit = if total > room {
        room.saturating_sub(1)
    } else {
        room
    };
    for cluster in clusters {
        if cells.len() + UnicodeWidthStr::width(cluster) > limit {
            break;
        }
        push_cluster(&mut cells, cluster, Style::default());
    }
    if total > room && room > 0 {
        cells.push(("\u{2026}".to_string(), Style::default()));
    }
    cells
}

fn framed(left: char, middle: Vec<Cell>, right: char, border: Style) -> Vec<Cell> {
    let mut line = vec![(left.to_string(), border)];
    line.extend(middle);
    line.push((right.to_string(), border));
    line
}

/// Write the lines into the area, one row each, clipped to the area.
fn draw_lines(lines: &[Vec<Cell>], area: Rect, buf: &mut Buffer) {
    for (row, line) in (0..area.height).zip(lines) {
        let y = area.y + row;
        for (column, (symbol, style)) in (0..area.width).zip(line) {
            let cell = &mut buf[(area.x + column, y)];
            if symbol.is_empty() {
                cell.reset();
            } else {
                cell.set_symbol(symbol);
            }
            cell.set_style(*style);
        }
    }
}
