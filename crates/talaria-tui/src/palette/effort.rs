//! The `/effort` selector, drawn in the slash palette's place under the composer: Hermes's
//! levels on one scale with a marker over the selected one, the selected level's description,
//! the staged Hivecode and save switches, and the key hints.
//!
//! Nothing here animates, so reduced motion draws the same frame. Without colour the marker
//! glyph and the bold label still show the selection.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Widget};
use talaria_core::hivecode::notice::NAME as HIVECODE;
use talaria_core::models::{Effort, EffortScale};

use crate::history::word_wrapped;
use crate::keys::{KeyChord, KeyMap, ModelAction};
use crate::live::LiveSection;
use crate::theme::surfaces::{Token, ViewStyles, tokens as t};

/// Columns before the scale.
const INDENT: usize = 2;
/// Columns between the scale and the Hivecode column.
const SIDE_GAP: usize = 4;
/// The widest and narrowest gaps between two labels.
const GAPS: std::ops::RangeInclusive<usize> = 2..=5;
const FASTER: &str = "Faster";
const SMARTER: &str = "Smarter";
const TOGGLE: &str = "Tab to toggle";
const MARKER: &str = "\u{25b2}";
const RULE: &str = "\u{2500}";
const SEPARATOR: &str = " \u{b7} ";

/// The labels of the keys the hints name, from the keys the user bound.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ScaleKeyLabels {
    pub lower: Option<String>,
    pub higher: Option<String>,
    pub save: Option<String>,
}

/// The selector moves with the `/model` picker's effort keys and saves with its scope key, so a
/// rebinding applies to both.
#[must_use]
pub fn scale_key_labels(map: &KeyMap) -> ScaleKeyLabels {
    let label = |action| {
        map.model_chords(action)
            .first()
            .map(KeyChord::label)
            .map(|label| match label.as_str() {
                "Left" => "\u{2190}".to_owned(),
                "Right" => "\u{2192}".to_owned(),
                _ => label,
            })
    };
    ScaleKeyLabels {
        lower: label(ModelAction::EffortDown),
        higher: label(ModelAction::EffortUp),
        save: label(ModelAction::ModelScope),
    }
}

/// One frame of the open selector.
pub struct EffortScaleSection {
    title: String,
    levels: Vec<Effort>,
    selected: Effort,
    detail: String,
    note: Option<String>,
    hivecode: String,
    save: String,
    hints: Vec<String>,
    /// The whole terminal's rows, for the height cap.
    terminal_rows: u16,
    views: Option<ViewStyles>,
}

/// How far the selector is cut to fit its cap, from nothing cut to the least it draws.
const CUTS: usize = 7;

impl EffortScaleSection {
    /// `terminal_rows` is the whole terminal, used only for the height cap: a third of it, as
    /// the slash palette and the `/theme` selector take, so the composer keeps its rows.
    #[must_use]
    pub fn new(scale: &EffortScale, keys: ScaleKeyLabels, terminal_rows: u16) -> Self {
        let adjust = match (keys.lower, keys.higher) {
            (Some(lower), Some(higher)) => Some(format!("{lower}/{higher} adjust")),
            (Some(one), None) | (None, Some(one)) => Some(format!("{one} adjust")),
            (None, None) => None,
        };
        let hints = [
            adjust,
            Some(format!("Tab {HIVECODE}")),
            keys.save.map(|save| format!("{save} new sessions")),
            Some("Enter apply".to_owned()),
            Some("Esc cancel".to_owned()),
        ]
        .into_iter()
        .flatten()
        .collect();
        Self {
            // The model, profile and wire level are names Hermes reported: every row that can
            // carry one is cleaned before it is laid out.
            title: super::render::clean(&scale.title()),
            levels: scale.levels(),
            selected: scale.selected(),
            detail: super::render::clean(&scale.detail()),
            note: scale.note().map(|note| super::render::clean(&note)),
            hivecode: super::render::clean(&scale.hivecode_line()),
            save: super::render::clean(&scale.save_line()),
            hints,
            terminal_rows,
            views: None,
        }
    }

    /// Draw with the resolved appearance, as the palette does.
    #[must_use]
    pub fn with_theme(self, views: ViewStyles) -> Self {
        Self {
            views: Some(views),
            ..self
        }
    }

    fn style(&self, token: Token, fallback: Style) -> Style {
        self.views
            .as_ref()
            .map_or(fallback, |views| views.inline(token, fallback))
    }

    fn muted(&self) -> Style {
        self.style(t::LIST_MUTED, Style::new().add_modifier(Modifier::DIM))
    }

    /// The selection is bold whatever the theme says, so it shows without colour.
    fn chosen(&self) -> Style {
        self.style(t::LIST_SELECTED, Style::new())
            .add_modifier(Modifier::BOLD)
    }

    /// The widest the Hivecode column gets, so pressing Tab never moves the scale.
    fn side_width() -> usize {
        let staged = format!("{HIVECODE} off (now on)");
        staged.chars().count().max(TOGGLE.len())
    }

    /// The gap between labels and whether the Hivecode column fits beside the scale; `None`
    /// when the scale itself does not fit.
    fn layout(&self, width: usize) -> Option<(usize, bool)> {
        let labels: usize = self.levels.iter().map(|level| level.as_wire().len()).sum();
        let span = |gap: usize| labels + gap * self.levels.len().saturating_sub(1);
        let fits = |gap: usize, side: bool| {
            INDENT
                + span(gap)
                + if side {
                    SIDE_GAP + Self::side_width()
                } else {
                    0
                }
                <= width
        };
        GAPS.rev()
            .find(|gap| fits(*gap, true))
            .map(|gap| (gap, true))
            .or_else(|| {
                GAPS.rev()
                    .find(|gap| fits(*gap, false))
                    .map(|gap| (gap, false))
            })
    }

    /// The rows drawn at this width within the height cap. A short terminal drops the save
    /// line, the note, the detail and the "Faster … Smarter" row in that order, then keeps one
    /// hint row, then draws the compact scale and drops the Hivecode row. The title, the scale
    /// and a hint row naming Enter and Esc always stay.
    fn lines(&self, width: u16) -> Vec<Line<'static>> {
        let cap = usize::from((self.terminal_rows / 3).max(3));
        (0..CUTS)
            .map(|cut| self.lines_cut(width, cut))
            .find(|lines| lines.len() <= cap)
            .unwrap_or_else(|| self.lines_cut(width, CUTS))
    }

    fn lines_cut(&self, width: u16, cut: usize) -> Vec<Line<'static>> {
        let columns = usize::from(width);
        let body = width
            .saturating_sub(u16::try_from(INDENT).unwrap_or(0))
            .max(1);
        let indent = " ".repeat(INDENT);
        let wrapped = |text: &str, style: Style| -> Vec<Line<'static>> {
            word_wrapped(text, body)
                .into_iter()
                .map(|row| Line::from(vec![Span::raw(indent.clone()), Span::styled(row, style)]))
                .collect()
        };
        let mut lines = vec![Line::styled(
            format!(" {}", self.title),
            self.style(t::LIST_HEADER, Style::new().add_modifier(Modifier::BOLD)),
        )];
        let switch = format!("{}{SEPARATOR}{TOGGLE}", self.hivecode);
        let switch_row = cut < CUTS;
        match self.layout(columns).filter(|_| cut < CUTS - 1) {
            Some((gap, side)) => {
                let [ends, rule, labels] = self.scale_rows(gap, side);
                if cut < 4 {
                    lines.push(ends);
                }
                lines.extend([rule, labels]);
                if !side && switch_row {
                    lines.extend(wrapped(&switch, Style::new()));
                }
            }
            None => {
                let at = self
                    .levels
                    .iter()
                    .position(|level| *level == self.selected)
                    .unwrap_or(0);
                lines.push(Line::from(vec![
                    Span::raw(format!("{indent}\u{25c0} ")),
                    Span::styled(self.selected.as_wire(), self.chosen()),
                    Span::raw(format!(" \u{25b6}   {} of {}", at + 1, self.levels.len())),
                ]));
                if switch_row {
                    lines.extend(wrapped(&switch, Style::new()));
                }
            }
        }
        if cut < 3 {
            lines.extend(wrapped(&self.detail, Style::new()));
        }
        if let Some(note) = self.note.as_ref().filter(|_| cut < 2) {
            lines.extend(wrapped(note, self.muted()));
        }
        if cut < 1 {
            lines.extend(wrapped(&self.save, Style::new()));
        }
        let hint_width = columns.saturating_sub(1).max(1);
        let hints = if cut < 5 {
            hint_rows(&self.hints, hint_width)
        } else {
            vec![essential_hint(hint_width)]
        };
        lines.extend(
            hints
                .into_iter()
                .map(|row| Line::styled(format!(" {row}"), self.muted())),
        );
        lines
    }

    /// The "Faster … Smarter" row, the rule with the marker, and the labels, with the Hivecode
    /// column beside the last two when `side`.
    fn scale_rows(&self, gap: usize, side: bool) -> [Line<'static>; 3] {
        let indent = " ".repeat(INDENT);
        let mut starts = Vec::with_capacity(self.levels.len());
        let mut at = 0;
        for level in &self.levels {
            starts.push(at);
            at += level.as_wire().len() + gap;
        }
        let span = at.saturating_sub(gap);
        let centre = self
            .levels
            .iter()
            .zip(&starts)
            .find(|(level, _)| **level == self.selected)
            .map_or(0, |(level, start)| start + level.as_wire().len() / 2);
        let ends = Line::styled(
            format!(
                "{indent}{FASTER}{}{SMARTER}",
                " ".repeat(span.saturating_sub(FASTER.len() + SMARTER.len()))
            ),
            self.muted(),
        );
        let mut rule = vec![
            Span::raw(indent.clone()),
            Span::styled(RULE.repeat(centre), self.muted()),
            Span::styled(MARKER, self.chosen()),
            Span::styled(RULE.repeat(span.saturating_sub(centre + 1)), self.muted()),
        ];
        let mut labels = vec![Span::raw(indent)];
        for (index, level) in self.levels.iter().enumerate() {
            if index > 0 {
                labels.push(Span::raw(" ".repeat(gap)));
            }
            let style = if *level == self.selected {
                self.chosen()
            } else {
                Style::new()
            };
            labels.push(Span::styled(level.as_wire(), style));
        }
        if side {
            let gap = " ".repeat(SIDE_GAP);
            rule.push(Span::raw(format!("{gap}{}", self.hivecode)));
            labels.push(Span::styled(format!("{gap}{TOGGLE}"), self.muted()));
        }
        [ends, Line::from(rule), Line::from(labels)]
    }
}

/// The one hint row a short terminal keeps: the keys that apply and close the selector.
fn essential_hint(width: usize) -> String {
    let full = format!("Enter apply{SEPARATOR}Esc cancel");
    if full.chars().count() <= width {
        full
    } else {
        format!("Enter{SEPARATOR}Esc")
    }
}

/// The hints joined by ` · `, breaking only between two hints.
fn hint_rows(hints: &[String], width: usize) -> Vec<String> {
    let mut rows: Vec<String> = Vec::new();
    for hint in hints {
        match rows.last_mut() {
            Some(row)
                if row.chars().count() + SEPARATOR.chars().count() + hint.chars().count()
                    <= width =>
            {
                row.push_str(SEPARATOR);
                row.push_str(hint);
            }
            _ => rows.push(hint.clone()),
        }
    }
    rows
}

impl LiveSection for EffortScaleSection {
    fn height(&self, width: u16) -> u16 {
        u16::try_from(self.lines(width).len()).unwrap_or(u16::MAX)
    }

    fn render(&self, area: Rect, buf: &mut Buffer) {
        let area = area.intersection(buf.area);
        for y in area.top()..area.bottom() {
            for x in area.left()..area.right() {
                buf[(x, y)].reset();
            }
        }
        // A cut section keeps its bottom rows, so the hint naming Enter and Esc stays.
        let lines = self.lines(area.width);
        let hidden =
            u16::try_from(lines.len().saturating_sub(usize::from(area.height))).unwrap_or(u16::MAX);
        Paragraph::new(lines).scroll((hidden, 0)).render(area, buf);
        if let Some(views) = &self.views {
            views.finish(area, buf, t::LIST_ROW, false);
        }
    }
}
