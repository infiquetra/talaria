//! Rewind overlay drawing and keys. Issue 233. Checkpoint rewind and diff.
//!
//! The draw takes the rewind view, which already carries the diff document. The diff body is
//! drawn by the diff view of issue 221 and never from a `rendered` field.

use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use talaria_core::diff::parse_diff;
use talaria_core::rewind::{
    DiffDoc, LineStyle, RewindKey, RewindScreen, RewindView, ViewLine, ViewRow,
};
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

use crate::diff_view::{DiffRenderOptions, diff_lines};
use crate::history::sanitize;
use crate::tool_cell::ToolTheme;

/// Width of the age column, the length of `just now`.
const AGE_COLUMNS: usize = 8;
/// Width of the clock column, `HH:MM`.
const CLOCK_COLUMNS: usize = 5;
/// Width of the tail column on the turn list.
const TAIL_COLUMNS: usize = 5;
/// Below this width the age and clock columns are dropped.
const NO_TIMES_BELOW: usize = 50;
/// Below this width only the label and the text remain.
const LABEL_ONLY_BELOW: usize = 30;
/// Most rows of prompt text the actions screen shows.
const PROMPT_ROWS: usize = 6;

/// Colours for the rewind overlay.
pub struct RewindTheme {
    /// The selected row.
    pub selected: Style,
    /// A dimmed row.
    pub dim: Style,
    /// A warning line.
    pub warning: Style,
    /// An accent line.
    pub accent: Style,
}

impl RewindTheme {
    fn line(&self, style: LineStyle) -> Style {
        match style {
            LineStyle::Normal => Style::default(),
            LineStyle::Dim => self.dim,
            LineStyle::Warning => self.warning,
            LineStyle::Accent => self.accent,
        }
    }
}

/// Draw the rewind overlay into `buf`.
pub fn draw_rewind(view: &RewindView<'_>, area: Rect, buf: &mut Buffer, theme: &RewindTheme) {
    if area.height < 3 || area.width == 0 || view.screen == RewindScreen::Closed {
        return;
    }
    let width = usize::from(area.width);
    let capacity = usize::from(visible_rows(area));

    let gutter = if view.screen == RewindScreen::Turns {
        " "
    } else {
        ""
    };
    let title = fit(&format!("{gutter}{}", sanitize(&view.title)), width);
    let bold = Style::default().add_modifier(Modifier::BOLD);
    buf.set_stringn(area.x, area.y, &title, width, bold);

    let (lines, skip) = match view.screen {
        RewindScreen::Turns | RewindScreen::Checkpoints if !view.rows.is_empty() => {
            let skip = view.first_visible.min(view.rows.len().saturating_sub(1));
            (row_lines(view, width, theme, skip, capacity), 0)
        }
        RewindScreen::Diff => match view.diff {
            Some(doc) => {
                let lines = diff_body(doc, area.width, theme);
                let skip = view.diff_scroll.min(lines.len().saturating_sub(capacity));
                (lines, skip)
            }
            None => (body_lines(&view.body, view.screen, width, theme), 0),
        },
        _ => (body_lines(&view.body, view.screen, width, theme), 0),
    };
    for (offset, line) in lines.iter().skip(skip).take(capacity).enumerate() {
        let y = area.y + 1 + u16::try_from(offset).unwrap_or(u16::MAX);
        buf.set_line(area.x, y, line, area.width);
    }

    let footer = fit(&sanitize(&view.footer), width);
    buf.set_stringn(area.x, area.y + area.height - 1, &footer, width, theme.dim);
}

/// How many list rows fit in `area`.
#[must_use]
pub fn visible_rows(area: Rect) -> u16 {
    area.height.saturating_sub(2)
}

/// Map one key on `screen` to a rewind input. `None` means the key is ignored.
#[must_use]
pub fn key_to_rewind(key: &KeyEvent, screen: RewindScreen) -> Option<RewindKey> {
    if screen == RewindScreen::Closed || key.kind == KeyEventKind::Release {
        return None;
    }
    if key.modifiers.contains(KeyModifiers::CONTROL) {
        let is_c = matches!(key.code, KeyCode::Char('c' | 'C'));
        return (is_c && !key.modifiers.contains(KeyModifiers::ALT)).then_some(RewindKey::Close);
    }
    if key.modifiers.contains(KeyModifiers::ALT) {
        return None;
    }
    let shifted = key.modifiers.contains(KeyModifiers::SHIFT);
    let code = match key.code {
        KeyCode::Char('g') if shifted => KeyCode::Char('G'),
        other => other,
    };
    let esc = matches!(code, KeyCode::Esc | KeyCode::Char('q'));
    let scrolling = |code: KeyCode| match code {
        KeyCode::Up | KeyCode::Char('k') => Some(RewindKey::Up),
        KeyCode::Down | KeyCode::Char('j') => Some(RewindKey::Down),
        KeyCode::PageUp => Some(RewindKey::PageUp),
        KeyCode::PageDown => Some(RewindKey::PageDown),
        KeyCode::Home | KeyCode::Char('g') => Some(RewindKey::Home),
        KeyCode::End | KeyCode::Char('G') => Some(RewindKey::End),
        _ => None,
    };
    match screen {
        RewindScreen::Turns => scrolling(code)
            .or((code == KeyCode::Enter).then_some(RewindKey::Enter))
            .or(esc.then_some(RewindKey::Esc)),
        RewindScreen::Checkpoints => scrolling(code)
            .or((code == KeyCode::Enter).then_some(RewindKey::Enter))
            .or((code == KeyCode::Char('d')).then_some(RewindKey::D))
            .or((code == KeyCode::Char('r')).then_some(RewindKey::R))
            .or(esc.then_some(RewindKey::Esc)),
        RewindScreen::Actions => match code {
            KeyCode::Char('c') => Some(RewindKey::C),
            KeyCode::Char('f') => Some(RewindKey::F),
            KeyCode::Char('d') => Some(RewindKey::D),
            _ => esc.then_some(RewindKey::Esc),
        },
        RewindScreen::Confirm | RewindScreen::CheckpointConfirm => match code {
            KeyCode::Enter | KeyCode::Char('y') => Some(RewindKey::Enter),
            KeyCode::Esc | KeyCode::Char('n') => Some(RewindKey::Esc),
            _ => None,
        },
        RewindScreen::Diff => match code {
            KeyCode::Char(' ') => Some(RewindKey::PageDown),
            _ => scrolling(code).or(esc.then_some(RewindKey::Esc)),
        },
        RewindScreen::Loading => esc.then_some(RewindKey::Esc),
        RewindScreen::Working | RewindScreen::Closed => None,
    }
}

/// Rows of the turn or checkpoint list, `take` of them from `skip`.
fn row_lines(
    view: &RewindView<'_>,
    width: usize,
    theme: &RewindTheme,
    skip: usize,
    take: usize,
) -> Vec<Line<'static>> {
    let turns = view.screen == RewindScreen::Turns;
    let show_times = width >= NO_TIMES_BELOW;
    let show_tail = turns && width >= LABEL_ONLY_BELOW;
    let show_clock = !turns && show_times;
    let label_columns = view
        .rows
        .iter()
        .map(|row| sanitize(&row.label).width())
        .max()
        .unwrap_or(0);
    let suffix_columns = if show_tail { 2 + TAIL_COLUMNS + 1 } else { 0 };
    let mut fixed = 3 + label_columns + 2;
    if show_times {
        fixed += 2 + AGE_COLUMNS;
    }
    if show_clock {
        fixed += 2 + CLOCK_COLUMNS;
    }
    let text_columns = width.saturating_sub(fixed + suffix_columns);
    view.rows
        .iter()
        .skip(skip)
        .take(take)
        .map(|row| {
            let text = row_text(
                row,
                label_columns,
                show_times,
                show_clock,
                text_columns,
                show_tail,
            );
            let style = if row.mark {
                theme.selected
            } else {
                Style::default()
            };
            Line::from(Span::styled(fit(&text, width), style))
        })
        .collect()
}

fn row_text(
    row: &ViewRow<'_>,
    label_columns: usize,
    show_age: bool,
    show_clock: bool,
    text_columns: usize,
    show_tail: bool,
) -> String {
    let mark = if row.mark { '›' } else { ' ' };
    let mut out = format!(" {mark} {}", pad(&sanitize(&row.label), label_columns));
    if show_age {
        out.push_str("  ");
        out.push_str(&pad(&fit(&sanitize(&row.age), AGE_COLUMNS), AGE_COLUMNS));
    }
    if show_clock {
        out.push_str("  ");
        out.push_str(&pad(
            &fit(&sanitize(&row.clock), CLOCK_COLUMNS),
            CLOCK_COLUMNS,
        ));
    }
    out.push_str("  ");
    out.push_str(&pad(&fit(&sanitize(row.text), text_columns), text_columns));
    if show_tail {
        let tail = fit(&sanitize(row.tail), TAIL_COLUMNS);
        out.push_str("  ");
        out.push_str(&" ".repeat(TAIL_COLUMNS.saturating_sub(tail.width())));
        out.push_str(&tail);
        out.push(' ');
    }
    out
}

/// Body lines of a panel, wrapped with a one column gutter.
fn body_lines(
    body: &[ViewLine],
    screen: RewindScreen,
    width: usize,
    theme: &RewindTheme,
) -> Vec<Line<'static>> {
    let columns = width.saturating_sub(1).max(1);
    let mut prompt_rows = (screen == RewindScreen::Actions).then_some(PROMPT_ROWS);
    let mut out = Vec::new();
    for line in body {
        if line.text.is_empty() || line.text.starts_with("c  ") {
            prompt_rows = None;
        }
        let mut rows = wrap(&line.text, columns);
        if let Some(left) = prompt_rows.as_mut() {
            if rows.len() > *left {
                rows.truncate(*left);
                if let Some(last) = rows.last_mut() {
                    *last = fit(&format!("{last}…"), columns);
                }
            }
            *left -= rows.len();
        }
        let style = theme.line(line.style);
        out.extend(
            rows.into_iter()
                .map(|row| Line::from(Span::styled(format!(" {row}"), style))),
        );
    }
    out
}

/// Everything a diff screen scrolls: subtitle, stat, the diff, notes and the cut note.
fn diff_body(doc: &DiffDoc, width: u16, theme: &RewindTheme) -> Vec<Line<'static>> {
    let columns = usize::from(width).saturating_sub(1).max(1);
    let prose = |text: &str, style: Style| -> Vec<Line<'static>> {
        wrap(text, columns)
            .into_iter()
            .map(|row| Line::from(Span::styled(format!(" {row}"), style)))
            .collect()
    };
    match doc {
        DiffDoc::Off => prose(
            talaria_core::rewind::text::CHECKPOINTS_OFF,
            Style::default(),
        ),
        DiffDoc::Empty(text) => prose(text, Style::default()),
        DiffDoc::Failed(text) => prose(text, theme.warning),
        DiffDoc::Text {
            subtitle,
            stat,
            diff,
            stat_only,
            cut_note,
            ..
        } => {
            let mut out = Vec::new();
            if let Some(subtitle) = subtitle {
                out.extend(prose(subtitle, Style::default()));
            }
            for row in stat.lines() {
                let row = fit(&sanitize(row), usize::from(width));
                out.push(Line::from(Span::styled(row, theme.dim)));
            }
            if !stat.is_empty() {
                out.push(Line::default());
            }
            if !stat_only {
                let model = parse_diff(&clean_diff(diff));
                let options = DiffRenderOptions {
                    width,
                    indent: 1,
                    max_rows: None,
                    theme: ToolTheme::default(),
                    color: true,
                };
                out.extend(diff_lines(&model, &options).0);
                for note in &model.notes {
                    let row = fit(&sanitize(note), usize::from(width));
                    out.push(Line::from(Span::styled(row, theme.dim)));
                }
            }
            if let Some(note) = cut_note {
                out.extend(prose(note, theme.dim));
            }
            out
        }
    }
}

/// Diff text with each line sanitized. Line ends stay, so the parser still sees lines.
fn clean_diff(diff: &str) -> String {
    diff.split('\n')
        .map(|row| sanitize(row.strip_suffix('\r').unwrap_or(row)).into_owned())
        .collect::<Vec<_>>()
        .join("\n")
}

/// Wrap prose at word breaks over sanitized text. A word wider than the row is broken.
fn wrap(text: &str, columns: usize) -> Vec<String> {
    let columns = columns.max(1);
    let mut rows = Vec::new();
    for paragraph in text.split('\n') {
        let clean = sanitize(paragraph);
        let mut current = String::new();
        let mut used = 0;
        for word in clean.split(' ').filter(|word| !word.is_empty()) {
            let word_columns = word.width();
            if used > 0 && used + 1 + word_columns <= columns {
                current.push(' ');
                current.push_str(word);
                used += 1 + word_columns;
                continue;
            }
            if used > 0 {
                rows.push(std::mem::take(&mut current));
                used = 0;
            }
            for grapheme in word.graphemes(true) {
                let grapheme_columns = grapheme.width();
                if used > 0 && used + grapheme_columns > columns {
                    rows.push(std::mem::take(&mut current));
                    used = 0;
                }
                current.push_str(grapheme);
                used += grapheme_columns;
            }
        }
        rows.push(current);
    }
    rows
}

/// `text` cut to `columns` display columns, ending in `…` when it was cut.
fn fit(text: &str, columns: usize) -> String {
    if text.width() <= columns {
        return text.to_string();
    }
    let mut out = String::new();
    let mut used = 0;
    for grapheme in text.graphemes(true) {
        let grapheme_columns = grapheme.width();
        if used + grapheme_columns + 1 > columns {
            break;
        }
        out.push_str(grapheme);
        used += grapheme_columns;
    }
    if columns > 0 {
        out.push('…');
    }
    out
}

/// `text` followed by spaces up to `columns` display columns.
fn pad(text: &str, columns: usize) -> String {
    let mut out = text.to_string();
    out.push_str(&" ".repeat(columns.saturating_sub(text.width())));
    out
}
