//! Assistant and reasoning cells drawn with one resolved theme.
//!
//! A cell renders with the theme it started under, so a reply that streams across a skin change
//! finishes and commits in its starting colours; the caller keeps that theme per cell id.

use std::collections::HashMap;
use std::time::{Duration, Instant};

use ratatui::style::{Modifier, Style};
use ratatui::text::Line;
use talaria_core::layout::ReasoningDisplay;
use talaria_core::session::{Cell, CellId, NoticeKind, ReasoningCell};

use super::Theme;
use crate::automation::hermes_command_lines_with_options_and_theme;
use crate::history::{CellLines, PlainCellLines, Speaker, word_wrapped};
use crate::markdown::{MarkdownOptions, render_markdown_with_theme};

/// Draws Markdown cells with one theme; every other cell keeps its owner's look. A presented
/// Hermes command's reply is Markdown too, drawn as the automation unit draws it.
pub struct ThemedCells<'a> {
    /// The theme the cells started under.
    pub theme: &'a Theme,
    /// The label of the key that opens the transcript view, when one is bound. A notice that
    /// carries what Hermes handed the agent names it.
    pub transcript_key: Option<&'a str>,
}

/// Preserve resolved reasoning attributes; an untouched default keeps the legacy dim fallback.
pub fn reasoning_base(theme: &Theme) -> Style {
    if theme.reasoning == Style::default() {
        theme.reasoning.add_modifier(Modifier::DIM)
    } else {
        theme.reasoning
    }
}

impl CellLines for ThemedCells<'_> {
    fn lines(&self, cell: &Cell, width: u16) -> Vec<Line<'static>> {
        self.cell_lines(cell, width, false)
    }

    fn continuation_lines(&self, cell: &Cell, width: u16) -> Vec<Line<'static>> {
        self.cell_lines(cell, width, true)
    }
}

impl ThemedCells<'_> {
    fn cell_lines(&self, cell: &Cell, width: u16, continuation: bool) -> Vec<Line<'static>> {
        let (text, base_style) = match cell {
            Cell::Assistant(cell) => (&cell.text, Style::default()),
            Cell::Reasoning(cell) => (&cell.text, reasoning_base(self.theme)),
            Cell::User(cell) => {
                let body = self.theme.views.inline(
                    super::surfaces::tokens::TRANSCRIPT_USER_TEXT,
                    Style::default(),
                );
                let label = self.theme.views.inline(
                    super::surfaces::tokens::TRANSCRIPT_USER_LABEL,
                    self.theme.status.style(crate::status::Tone::Accent),
                );
                let lines = word_wrapped(&cell.text, Speaker::body_width(width))
                    .into_iter()
                    .map(|row| Line::styled(row, body))
                    .collect();
                return Speaker::User.lines(lines, width, label, continuation);
            }
            Cell::Notice(cell) => {
                let dim = Style::default().add_modifier(Modifier::DIM);
                let token = if cell.kind == NoticeKind::Error {
                    super::surfaces::tokens::ALERT_ERROR
                } else {
                    super::surfaces::tokens::TRANSCRIPT_NOTICE
                };
                let style = if self.theme.views.active() {
                    self.theme.views.inline(token, dim)
                } else {
                    dim
                };
                return crate::history::prefixed_wrapped(
                    "\u{b7} ",
                    &crate::history::notice_text(cell, self.transcript_key),
                    style,
                    width,
                );
            }
            Cell::HermesCommand(cell) => {
                return hermes_command_lines_with_options_and_theme(
                    cell,
                    width,
                    &self.theme.views.automation_theme(),
                    &MarkdownOptions {
                        width,
                        theme: self.theme.markdown.clone(),
                        color: self.theme.color,
                        base_style: Style::default(),
                    },
                    &self.theme.views,
                );
            }
            Cell::Shell(cell) if self.theme.views.active() => {
                return self.theme.views.inline_lines(
                    crate::side::shell_cell_lines(cell, width, &self.theme.views.side_theme()),
                    super::surfaces::tokens::SIDE_METADATA,
                );
            }
            Cell::Side(cell) if self.theme.views.active() => {
                return self.theme.views.inline_lines(
                    crate::side::side_cell_lines(cell, width, &self.theme.views.side_theme()),
                    super::surfaces::tokens::SIDE_METADATA,
                );
            }
            Cell::Background(cell) if self.theme.views.active() => {
                return self.theme.views.inline_lines(
                    crate::background::background_cell_lines(
                        cell,
                        width,
                        &self.theme.views.background_theme(),
                    ),
                    super::surfaces::tokens::BACKGROUND_TEXT,
                );
            }
            _ => return PlainCellLines.lines(cell, width),
        };
        let mut lines = render_markdown_with_theme(
            text,
            &MarkdownOptions {
                width: if matches!(cell, Cell::Assistant(_)) {
                    Speaker::body_width(width)
                } else {
                    width
                },
                theme: self.theme.markdown.clone(),
                color: self.theme.color,
                base_style,
            },
            &self.theme.views,
        );
        if let Cell::Reasoning(reasoning) = cell {
            lines.insert(
                0,
                Line::styled(reasoning_heading(reasoning), heading_style(self.theme)),
            );
        } else {
            let label = self.theme.views.inline(
                super::surfaces::tokens::TRANSCRIPT_ASSISTANT_LABEL,
                self.theme.status.style(crate::status::Tone::Muted),
            );
            lines = Speaker::Assistant.lines(lines, width, label, continuation);
        }
        lines
    }
}

/// The style of a reasoning block's heading: the reasoning colour, dim.
fn heading_style(theme: &Theme) -> Style {
    let heading = reasoning_base(theme).add_modifier(Modifier::DIM);
    if theme.views.active() {
        theme
            .views
            .inline(super::surfaces::tokens::TRANSCRIPT_REASONING_LABEL, heading)
    } else {
        heading
    }
}

/// Whether the main screen draws this block as one line under `display`. A block that is its
/// turn's only output is always drawn in full, so a turn never ends on a bare summary line.
#[must_use]
pub fn reasoning_collapses(cell: &ReasoningCell, display: ReasoningDisplay) -> bool {
    display == ReasoningDisplay::Collapsed && !cell.no_reply
}

/// The one line a collapsed reasoning block shows: its heading, how long it took when the
/// screen timed it, its line count and where to read it in full.
#[must_use]
pub fn reasoning_summary_text(
    cell: &ReasoningCell,
    took: Option<Duration>,
    transcript_key: Option<&str>,
) -> String {
    let label = if !cell.sealed {
        "\u{2234} thinking\u{2026}".to_owned()
    } else if cell.latest_step {
        "\u{2234} reasoning, last step".to_owned()
    } else {
        match took.filter(|took| took.as_secs() >= 1) {
            Some(took) => format!("\u{2234} thought for {}", took_label(took)),
            None => "\u{2234} reasoning".to_owned(),
        }
    };
    let count = crate::history::split_lines(cell.text.trim()).count();
    let noun = if count == 1 { "line" } else { "lines" };
    let place = transcript_key.map_or_else(
        || " in transcript view".to_owned(),
        |key| format!(", {key} to view"),
    );
    format!("{label} (+{count} {noun}{place})")
}

/// `12s` under a minute, `1m 05s` after, as the working row counts.
fn took_label(took: Duration) -> String {
    let seconds = took.as_secs();
    if seconds < 60 {
        format!("{seconds}s")
    } else {
        format!("{}m {:02}s", seconds / 60, seconds % 60)
    }
}

/// [`reasoning_summary_text`] in the heading's style, wrapped only when the width is too
/// narrow for one row.
#[must_use]
pub fn reasoning_summary_lines(
    cell: &ReasoningCell,
    theme: &Theme,
    took: Option<Duration>,
    transcript_key: Option<&str>,
    width: u16,
) -> Vec<Line<'static>> {
    crate::history::prefixed_wrapped(
        "",
        &reasoning_summary_text(cell, took, transcript_key),
        heading_style(theme),
        width,
    )
}

/// How long each reasoning block streamed, from the frame that first showed it open to the
/// frame that first showed it sealed. A block that arrived whole, as Hermes's end-of-turn
/// reasoning and a resumed session's do, has no duration.
///
/// Cell ids are positions within one session, so another session reuses them. The clock
/// belongs to one cell source and forgets everything when the source changes.
#[derive(Debug, Default)]
pub struct ReasoningClock {
    source: u64,
    started: HashMap<CellId, Instant>,
    took: HashMap<CellId, Duration>,
}

impl ReasoningClock {
    /// An empty clock.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Follow the cells of `source`. A different source than before (a switch, a resume, a
    /// reload) forgets every timing, so a block in the new source never takes the time of the
    /// block that had its id in the old one.
    pub fn follow_source(&mut self, source: u64) {
        if self.source != source {
            self.source = source;
            self.started.clear();
            self.took.clear();
        }
    }

    /// Start the open blocks not yet seen, stop the ones sealed since, and forget blocks that
    /// are no longer in `cells`.
    pub fn observe(&mut self, cells: &[Cell], now: Instant) {
        let mut present = Vec::new();
        for cell in cells {
            let Cell::Reasoning(reasoning) = cell else {
                continue;
            };
            present.push(reasoning.id);
            if !reasoning.sealed {
                self.started.entry(reasoning.id).or_insert(now);
            } else if let Some(started) = self.started.remove(&reasoning.id) {
                self.took
                    .insert(reasoning.id, now.saturating_duration_since(started));
            }
        }
        self.started.retain(|id, _| present.contains(id));
        self.took.retain(|id, _| present.contains(id));
    }

    /// How long a block streamed. One the clock saw open but not yet sealed counts until `now`,
    /// so a block committed in the frame that seals it still has its time.
    #[must_use]
    pub fn took(&self, id: CellId, now: Instant) -> Option<Duration> {
        self.took.get(&id).copied().or_else(|| {
            self.started
                .get(&id)
                .map(|started| now.saturating_duration_since(*started))
        })
    }
}

/// The heading line of a reasoning block. Hermes's end-of-turn reasoning in a turn that ran
/// tools is only the latest step's thinking, drawn after that step, so its heading says so.
pub fn reasoning_heading(cell: &ReasoningCell) -> &'static str {
    if cell.latest_step {
        "∴ reasoning (last step)"
    } else if cell.sealed {
        "∴ reasoning"
    } else {
        "∴ thinking"
    }
}

/// The id of a cell drawn with a theme (assistant and reasoning text), if it is one.
pub fn themed_cell_id(cell: &Cell) -> Option<CellId> {
    match cell {
        Cell::Assistant(cell) => Some(cell.id),
        Cell::Reasoning(cell) => Some(cell.id),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use ratatui::text::Line;
    use talaria_core::automation::HermesCommandCell;
    use talaria_core::session::Cell;

    use super::ThemedCells;
    use crate::automation::{AutomationTheme, hermes_command_lines};
    use crate::history::CellLines;
    use crate::theme::{
        ColorDepth, DepthSource, Motion, MotionSource, Polarity, PolaritySource, TerminalColors,
        ThemeInputs, resolve,
    };

    fn rows(lines: &[Line<'_>]) -> Vec<String> {
        lines.iter().map(ToString::to_string).collect()
    }

    #[test]
    fn hermes_command_replies_render_as_markdown() {
        let theme = resolve(&ThemeInputs {
            skin: None,
            skipped: &[],
            follow_hermes_skin: false,
            polarity: (Polarity::Dark, PolaritySource::Config),
            depth: (ColorDepth::TrueColor, DepthSource::Config),
            motion: (Motion::Full, MotionSource::Default),
            terminal: TerminalColors::default(),
        });
        let themed = ThemedCells {
            theme: &theme,
            transcript_key: None,
        };
        for (name, typed, body, shown) in [
            (
                "yolo",
                "/yolo",
                "⚡ YOLO mode **ON** for this session",
                "  ⚡ YOLO mode ON for this session",
            ),
            (
                "loop",
                "/loop 45m",
                "/loop: missing prompt (usage: /loop [interval] \\<prompt\\>)",
                "  /loop: missing prompt (usage: /loop [interval] <prompt>)",
            ),
        ] {
            let command = HermesCommandCell {
                name: name.to_owned(),
                typed: typed.to_owned(),
                body: body.to_owned(),
                warning: false,
            };
            let lines = themed.lines(&Cell::HermesCommand(command.clone()), 80);
            assert_eq!(rows(&lines), vec![typed.to_owned(), shown.to_owned()]);
            assert_eq!(
                lines,
                hermes_command_lines(&command, 80, &AutomationTheme::default())
            );
        }
    }
}
