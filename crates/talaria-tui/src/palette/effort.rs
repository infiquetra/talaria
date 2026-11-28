//! The inline reasoning effort picker. A marked stop and a bold level name carry the
//! selection even in monochrome. All changes remain staged until Enter.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Widget};
use talaria_core::hivecode::notice::NAME as HIVECODE;
use talaria_core::models::{Effort, EffortScale};

use super::render::clean;
use crate::checklist::cut_to_width;
use crate::history::word_wrapped;
use crate::keys::{KeyChord, KeyMap, ModelAction};
use crate::live::LiveSection;
use crate::theme::surfaces::{ViewStyles, tokens as t};

const INDENT: &str = "  ";
const GROUP_GAP: &str = "    ";

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

/// Presentation data for one frame. Reading the picker never changes the session.
pub struct EffortScaleSection {
    model: Option<String>,
    profile: String,
    levels: Vec<Effort>,
    selected: Effort,
    current: Option<Effort>,
    note: Option<String>,
    hivecode: bool,
    hivecode_changed: bool,
    save: bool,
    adjust_key: Option<String>,
    save_key: Option<String>,
    terminal_rows: u16,
    views: ViewStyles,
}

impl EffortScaleSection {
    #[must_use]
    pub fn new(scale: &EffortScale, keys: ScaleKeyLabels, terminal_rows: u16) -> Self {
        let adjust_key = match (keys.lower, keys.higher) {
            (Some(lower), Some(higher)) => Some(format!("{lower}/{higher}")),
            (one, None) | (None, one) => one,
        };
        Self {
            model: scale.model().map(clean),
            profile: clean(scale.profile()),
            levels: scale.levels(),
            selected: scale.selected(),
            current: scale.current(),
            note: scale.note().map(|note| clean(&note)),
            hivecode: scale.hivecode(),
            hivecode_changed: scale.hivecode_changed(),
            save: scale.save_default(),
            adjust_key,
            save_key: keys.save,
            terminal_rows,
            views: ViewStyles::default(),
        }
    }

    #[must_use]
    pub fn with_theme(mut self, views: ViewStyles) -> Self {
        self.views = views;
        self
    }

    fn muted(&self) -> Style {
        self.views.inline(t::LIST_MUTED, Style::new().dim())
    }

    fn chosen(&self) -> Style {
        self.views.inline(t::EFFORT_SELECTED, Style::new()).bold()
    }

    fn key(&self, text: &str) -> Span<'static> {
        Span::styled(
            text.to_owned(),
            self.views.inline(t::DIALOG_KEY, Style::new()).bold(),
        )
    }

    fn heading(&self, width: u16) -> Line<'static> {
        let mut line = Line::from(vec![
            Span::styled(
                " Effort",
                self.views.inline(t::LIST_HEADER, Style::new()).bold(),
            ),
            Span::raw("   "),
            Span::styled(
                if self.current == Some(self.selected) {
                    "Current: "
                } else {
                    "Preview: "
                },
                self.muted(),
            ),
            Span::styled(self.selected.as_wire(), self.chosen()),
        ]);
        if let Some(current) = self.current.filter(|level| *level != self.selected) {
            line.push_span(Span::styled(
                format!("   Current: {}", current.as_wire()),
                self.muted(),
            ));
        }
        if let Some(model) = &self.model {
            let room = width.saturating_sub(line.width() as u16).saturating_sub(3);
            if room >= 4 {
                line.push_span(Span::styled(" · ", self.muted()));
                line.push_span(Span::styled(
                    cut_to_width(model, room),
                    self.views.inline(t::EFFORT_MODEL, self.muted()),
                ));
            }
        }
        line
    }

    fn compact_scale(&self) -> Line<'static> {
        let at = self
            .levels
            .iter()
            .position(|level| *level == self.selected)
            .unwrap_or(0);
        Line::from(vec![
            Span::styled("  ◀ ", self.muted()),
            Span::styled(self.selected.as_wire(), self.chosen().underlined()),
            Span::styled(
                format!(" ▶   {} of {}", at + 1, self.levels.len()),
                self.muted(),
            ),
        ])
    }

    /// Use the label centres as stop positions so a level and its marker stay aligned.
    fn scale_rows(&self, width: u16) -> Option<[Line<'static>; 3]> {
        let label_width: usize = self.levels.iter().map(|level| level.as_wire().len()).sum();
        let gap = (usize::from(width).saturating_sub(INDENT.len() + label_width)
            / self.levels.len().saturating_sub(1).max(1))
        .min(5);
        if gap < 2 {
            return None;
        }
        let filled = self.views.inline(t::EFFORT_TRACK, Style::new());
        let empty = self.muted();
        let mut label_start = INDENT.len();
        let mut rail_end = INDENT.len();
        let mut rail = Line::from(INDENT);
        let mut labels = Line::from(INDENT);
        for (index, level) in self.levels.iter().enumerate() {
            if index > 0 {
                labels.push_span(" ".repeat(gap));
                label_start += gap;
            }
            let label = level.as_wire();
            let centre = label_start + label.len() / 2;
            let distance = centre - rail_end;
            if index == 0 {
                rail.push_span(" ".repeat(distance));
            } else if *level <= self.selected {
                rail.push_span(Span::styled("━".repeat(distance), filled));
            } else {
                rail.push_span(Span::styled("─".repeat(distance), empty));
            }
            let (stop, style) = match level.cmp(&self.selected) {
                std::cmp::Ordering::Less => ("●", filled),
                std::cmp::Ordering::Equal => ("◆", self.chosen()),
                std::cmp::Ordering::Greater => ("○", empty),
            };
            rail.push_span(Span::styled(stop, style));
            rail_end = centre + 1;
            label_start += label.len();
            labels.push_span(if *level == self.selected {
                Span::styled(label, self.chosen().underlined())
            } else {
                Span::raw(label)
            });
        }
        let gap = labels
            .width()
            .saturating_sub(INDENT.len() + "Faster".len() + "More reasoning".len());
        let captions = Line::styled(
            format!("{INDENT}Faster{}More reasoning", " ".repeat(gap)),
            self.muted(),
        );
        Some([captions, rail, labels])
    }

    fn switch_value(&self, on: bool) -> Span<'static> {
        if on {
            Span::styled(
                "on",
                self.views.inline(t::LIST_SUCCESS, Style::new()).bold(),
            )
        } else {
            Span::styled("off", self.muted())
        }
    }

    fn switches(&self, width: u16, rows: usize) -> Vec<Line<'static>> {
        if rows == 0 {
            return Vec::new();
        }
        let mut hive = Line::from(vec![
            Span::raw(INDENT),
            self.key("Tab"),
            Span::raw(format!(" {HIVECODE} ")),
            self.switch_value(self.hivecode),
        ]);
        if self.hivecode_changed {
            hive.push_span(Span::styled(
                if self.hivecode {
                    " (was off)"
                } else {
                    " (was on)"
                },
                self.muted(),
            ));
        }
        let mut save = Line::default();
        if let Some(key) = &self.save_key {
            save.push_span(self.key(key));
            save.push_span(" ");
        }
        save.push_span(if width >= 40 {
            "Save for new sessions "
        } else if width >= 24 {
            "Save default "
        } else {
            "Save "
        });
        save.push_span(self.switch_value(self.save));
        let available = usize::from(width).saturating_sub(INDENT.len() + save.width());
        if available >= 6 {
            save.push_span(Span::styled(
                format!(" ({})", cut_to_width(&self.profile, (available - 3) as u16)),
                self.muted(),
            ));
        }
        if hive.width() + GROUP_GAP.len() + save.width() <= usize::from(width) {
            hive.push_span(GROUP_GAP);
            hive.spans.extend(save.spans);
            vec![hive]
        } else if rows == 1 {
            let compact = |with_keys: bool| {
                let mut line = Line::default();
                if with_keys {
                    line.push_span(self.key("Tab"));
                    line.push_span(" ");
                }
                line.push_span(format!("{HIVECODE} "));
                line.push_span(self.switch_value(self.hivecode));
                line.push_span("  ");
                if with_keys && let Some(key) = &self.save_key {
                    line.push_span(self.key(key));
                    line.push_span(" ");
                }
                line.push_span("Save ");
                line.push_span(self.switch_value(self.save));
                line
            };
            let keyed = compact(true);
            vec![if keyed.width() <= usize::from(width) {
                keyed
            } else {
                compact(false)
            }]
        } else {
            save.spans.insert(0, Span::raw(INDENT));
            vec![hive, save]
        }
    }

    fn footer(&self, width: u16) -> Line<'static> {
        let mut actions = Line::from(vec![
            self.key("Enter"),
            Span::styled(" apply   ", self.muted()),
            self.key("Esc"),
            Span::styled(" cancel", self.muted()),
        ]);
        let mut line = Line::from(INDENT);
        if line.width() + actions.width() > usize::from(width) {
            actions = Line::from(vec![self.key("Enter"), Span::raw("  "), self.key("Esc")]);
        }
        if let Some(key) = &self.adjust_key {
            let adjust = self.key(key);
            if line.width() + adjust.width() + " adjust   ".len() + actions.width()
                <= usize::from(width)
            {
                line.push_span(adjust);
                line.push_span(Span::styled(" adjust   ", self.muted()));
            } else if line.width() + adjust.width() + 2 + actions.width() <= usize::from(width) {
                line.push_span(adjust);
                line.push_span("  ");
            }
        }
        line.spans.extend(actions.spans);
        line
    }

    fn height_cap(&self) -> u16 {
        (self.terminal_rows / 3).max(3)
    }

    fn lines(&self, width: u16, cap: u16) -> Vec<Line<'static>> {
        let footer = self.footer(width);
        match cap {
            0 => return vec![],
            1 => return vec![self.compact_scale()],
            2 => return vec![self.compact_scale(), footer],
            _ => {}
        }
        // Reserve the heading, one scale row and the footer before spending rows on detail.
        let mut spare = usize::from(cap) - 3;
        let body = width.saturating_sub(INDENT.len() as u16).max(1);
        let mut notes = Vec::new();
        if let Some(note) = &self.note
            && spare > 0
        {
            let wrapped = word_wrapped(note, body);
            notes = if wrapped.len() <= spare {
                wrapped
            } else {
                vec![cut_to_width(note, body)]
            };
            spare -= notes.len();
        }
        let settings = self.switches(width, spare);
        spare -= settings.len();

        let scale = (spare > 0).then(|| self.scale_rows(width)).flatten();
        if scale.is_some() {
            spare -= 1;
        }
        let mut detail = word_wrapped(self.selected.describe(), body);
        if detail.len() > spare {
            detail.clear();
        }
        spare -= detail.len();
        let mut rows = vec![self.heading(width)];
        if let Some([captions, rail, labels]) = scale {
            if spare > 0 {
                rows.push(captions);
            }
            rows.extend([rail, labels]);
        } else {
            rows.push(self.compact_scale());
        }
        rows.extend(
            detail
                .into_iter()
                .map(|text| Line::from(format!("{INDENT}{text}"))),
        );
        rows.extend(notes.into_iter().map(|text| {
            Line::styled(
                format!("{INDENT}{text}"),
                self.views.inline(t::PICKER_DETAIL_WARNING, Style::new()),
            )
        }));
        rows.extend(settings);
        rows.push(footer);
        rows
    }
}

impl LiveSection for EffortScaleSection {
    fn height(&self, width: u16) -> u16 {
        self.lines(width, self.height_cap()).len() as u16
    }

    fn render(&self, area: Rect, buf: &mut Buffer) {
        let area = area.intersection(buf.area);
        for y in area.top()..area.bottom() {
            for x in area.left()..area.right() {
                buf[(x, y)].reset();
            }
        }
        let rows = self.lines(area.width, self.height_cap().min(area.height));
        Paragraph::new(rows).render(area, buf);
        self.views.finish(area, buf, t::LIST_ROW, false);
    }
}
