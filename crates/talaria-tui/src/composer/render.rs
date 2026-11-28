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
use crate::theme::BorderPreset;
use crate::theme::surfaces::{ViewStyles, tokens as t};

/// One drawn cell: its text and style.
type Cell = (String, Style);

/// A prepared view of the composer for one draw.
#[derive(Debug)]
pub struct ComposerSection<'a> {
    composer: &'a super::Composer,
    border_preset: BorderPreset,
    conversation: Option<&'a str>,
    focused: bool,
}

impl<'a> ComposerSection<'a> {
    pub(crate) fn new(composer: &'a super::Composer) -> Self {
        Self {
            composer,
            border_preset: BorderPreset::Rounded,
            conversation: None,
            focused: false,
        }
    }

    /// Select the frame glyph preset for this draw.
    pub fn with_border_preset(mut self, preset: BorderPreset) -> Self {
        self.border_preset = preset;
        self
    }

    /// Draw a conversation draft with its destination and effective shortcuts.
    pub fn in_conversation(mut self, title: &'a str, focused: bool) -> Self {
        self.conversation = Some(title);
        self.focused = focused;
        self
    }

    /// Draw with the selected frame preset while retaining the prepared draft.
    pub fn render_with_theme(&self, area: Rect, buf: &mut Buffer, views: &ViewStyles) {
        let section = Self {
            composer: self.composer,
            conversation: self.conversation,
            focused: self.focused,
            border_preset: if views.active() {
                views.border_preset()
            } else {
                self.border_preset
            },
        };
        if let (Some(title), Some(prepared)) = (section.conversation, self.composer.prepared)
            && prepared.boxed
        {
            section.render_conversation(title, &prepared, area, buf, views);
        } else {
            section.render(area, buf);
        }
        views.finish(
            area,
            buf,
            crate::theme::surfaces::tokens::COMPOSER_TEXT,
            false,
        );
    }

    fn border_glyphs(&self) -> [char; 6] {
        match self.border_preset {
            BorderPreset::Rounded => ['╭', '╮', '╰', '╯', '─', '│'],
            BorderPreset::Single => ['┌', '┐', '└', '┘', '─', '│'],
            BorderPreset::Double => ['╔', '╗', '╚', '╝', '═', '║'],
            BorderPreset::Ascii => ['+', '+', '+', '+', '-', '|'],
            BorderPreset::None => [' '; 6],
        }
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
        let show_caret = self.conversation.is_none() || self.focused;
        let mut out: Vec<Cell> = Vec::new();
        if text.is_empty() {
            if composer
                .vim_mode()
                .is_some_and(|mode| mode != VimMode::Insert)
            {
                if show_caret {
                    out.push((" ".to_string(), Style::default().add_modifier(reversed)));
                }
                return out;
            }
            let placeholder = if self.conversation.is_some() {
                "Type a message".to_owned()
            } else {
                composer.placeholder()
            };
            for (position, cluster) in placeholder.graphemes(true).enumerate() {
                if out.len() + UnicodeWidthStr::width(cluster) > cells {
                    break;
                }
                let mut style = dim;
                if position == 0 && show_caret {
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
        let caret_here = row_of(rows, caret) == index && show_caret;
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
        let [
            top_left,
            top_right,
            bottom_left,
            bottom_right,
            horizontal,
            vertical,
        ] = self.border_glyphs();
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

        let mut top = border_cells(
            interior,
            Self::top_hint(prepared).as_deref(),
            dim,
            border,
            horizontal,
        );
        if let Some(title) = self.composer.title.as_deref() {
            let hint_cells = Self::top_hint(prepared).map_or(0, |hint| hint.chars().count() + 1);
            let room = interior.saturating_sub(1 + hint_cells);
            let mut placed: Vec<Cell> = vec![(" ".to_string(), Style::default())];
            placed.extend(fit_title(title, room));
            for (slot, cell) in top.iter_mut().zip(placed) {
                *slot = cell;
            }
        }
        let mut lines: Vec<Vec<Cell>> = Vec::new();
        lines.push(framed(top_left, top, top_right, border));

        for offset in 0..prepared.shown {
            let mut line: Vec<Cell> = vec![
                (vertical.to_string(), border),
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
            line.push((vertical.to_string(), border));
            lines.push(line);
        }

        let hint = self.bottom_hint(prepared);
        let mut bottom = border_cells(interior, hint.as_deref(), dim, border, horizontal);
        if let Some(mode) = self.mode_cells(interior.saturating_sub(1)) {
            bottom = vec![(horizontal.to_string(), border); interior];
            if !bottom.is_empty() {
                bottom[0] = (" ".to_string(), Style::default());
            }
            for (slot, cell) in bottom.iter_mut().skip(1).zip(mode) {
                *slot = cell;
            }
        }
        lines.push(framed(bottom_left, bottom, bottom_right, border));
        draw_lines(&lines, area, buf);
    }

    fn render_conversation(
        &self,
        conversation: &str,
        prepared: &Prepared,
        area: Rect,
        buf: &mut Buffer,
        views: &ViewStyles,
    ) {
        let [
            top_left,
            top_right,
            bottom_left,
            bottom_right,
            horizontal,
            vertical,
        ] = self.border_glyphs();
        let width = usize::from(prepared.width.min(area.width));
        let interior = width.saturating_sub(2);
        let dim = Style::default().add_modifier(Modifier::DIM);
        let border = if self.border_preset == BorderPreset::None {
            Style::default()
        } else if self.focused {
            views.inline(t::COMPOSER_BORDER_FOCUSED, self.composer.style.border)
        } else {
            views.inline(t::COMPOSER_BORDER, self.composer.style.border)
        };
        let marker_style = if self.focused {
            views.inline(t::COMPOSER_MARKER, self.composer.style.marker)
        } else {
            views.inline(t::COMPOSER_BORDER, self.composer.style.border)
        };
        let label = views.inline(t::COMPOSER_TEXT, Style::default()).bold();
        let muted = views.inline(t::COMPOSER_PLACEHOLDER, dim);
        let room = interior.saturating_sub(2);
        let above = Self::top_hint(prepared)
            .filter(|hint| hint.width() + 6 <= room)
            .map(|hint| format!(" {hint} "));
        let title_room = room.saturating_sub(above.as_deref().map_or(0, UnicodeWidthStr::width));
        let title = self.composer.title.as_deref().unwrap_or(conversation);
        let mut top = border_cells(interior, above.as_deref(), muted, border, horizontal);
        let mut title_cells = fit_title(title, title_room);
        if !title_cells.is_empty() {
            title_cells.insert(0, (" ".to_owned(), Style::default()));
            title_cells.push((" ".to_owned(), Style::default()));
            for (slot, (text, _)) in top.iter_mut().zip(title_cells) {
                *slot = (text, label);
            }
        }

        let mut lines = vec![framed(top_left, top, top_right, border)];
        let text_width = prepared.text_cells.min(interior.saturating_sub(5));
        for offset in 0..prepared.shown {
            let mut line = vec![(" ".to_owned(), Style::default())];
            let marker = match (offset, self.border_preset) {
                (0, BorderPreset::Ascii) => "> ",
                (0, _) => "› ",
                _ => "  ",
            };
            extend_cells(&mut line, marker, marker_style);
            let mut text = self.text_cells(prepared.first + offset, text_width);
            if self.composer.draft.text().is_empty() {
                for (_, style) in &mut text {
                    *style = muted.patch(*style);
                }
            }
            line.extend(text);
            pad(&mut line, interior);
            lines.push(framed(vertical, line, vertical, border));
        }
        let mut hint_cells = Vec::new();
        if let Some(hint) = self.bottom_hint(prepared) {
            extend_cells(&mut hint_cells, &hint, muted);
        } else if let Some(mode) = self.mode_cells(room) {
            hint_cells = mode;
        } else if self.focused {
            self.shortcut_cells(&mut hint_cells, room, label, muted);
        }
        let mut bottom = border_cells(interior, None, muted, border, horizontal);
        if !hint_cells.is_empty() {
            hint_cells.insert(0, (" ".to_owned(), Style::default()));
            hint_cells.truncate(room + 1);
            hint_cells.push((" ".to_owned(), Style::default()));
            for (slot, cell) in bottom.iter_mut().zip(hint_cells) {
                *slot = cell;
            }
        }
        lines.push(framed(bottom_left, bottom, bottom_right, border));
        draw_lines(&lines, area, buf);
    }

    fn shortcut_cells(&self, cells: &mut Vec<Cell>, room: usize, key_style: Style, label: Style) {
        let keys = &self.composer.keymap;
        let newline = keys
            .chords(Action::Newline)
            .iter()
            .find(|key| self.composer.keyboard_enhancement || !key.needs_enhancement())
            .map(|key| key.label());
        let start = cells.len();
        for (key, action) in [
            (keys.label(Action::Submit), "send"),
            (newline, "newline"),
            (Some("/".to_owned()), "commands"),
            (keys.label(Action::TranscriptOpen), "transcript"),
        ] {
            let Some(key) = key else { continue };
            let separator = if cells.len() == start { "" } else { "   " };
            if cells.len() - start + separator.len() + key.width() + 1 + action.len() <= room {
                extend_cells(cells, separator, label);
                extend_cells(cells, &key, key_style);
                extend_cells(cells, " ", label);
                extend_cells(cells, action, label);
            }
        }
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
            if let Some(title) = self.conversation {
                self.render_conversation(title, &prepared, area, buf, &ViewStyles::default());
            } else {
                self.render_boxed(&prepared, area, buf);
            }
        } else {
            self.render_borderless(&prepared, area, buf);
        }
    }
}

fn extend_cells(out: &mut Vec<Cell>, text: &str, style: Style) {
    for cluster in text.graphemes(true) {
        push_cluster(out, cluster, style);
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
    horizontal: char,
) -> Vec<Cell> {
    let mut cells: Vec<Cell> = vec![(horizontal.to_string(), border); interior];
    if let Some(hint) = hint {
        let start = interior.saturating_sub(hint.width());
        let mut placed = Vec::new();
        for cluster in hint.graphemes(true) {
            if placed.len() + cluster.width() > interior {
                break;
            }
            push_cluster(&mut placed, cluster, hint_style);
        }
        for (slot, cell) in cells[start..].iter_mut().zip(placed) {
            *slot = cell;
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
