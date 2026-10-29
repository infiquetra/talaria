//! Assistant and reasoning cells drawn with one resolved theme.
//!
//! A cell renders with the theme it started under, so a reply that streams across a skin change
//! finishes and commits in its starting colours; the caller keeps that theme per cell id.

use ratatui::style::{Modifier, Style};
use ratatui::text::Line;
use talaria_core::session::{Cell, CellId};

use super::Theme;
use crate::automation::{AutomationTheme, hermes_command_lines_with_options};
use crate::history::{CellLines, PlainCellLines};
use crate::markdown::{MarkdownOptions, render_markdown};

/// Draws Markdown cells with one theme; every other cell keeps its owner's look. A presented
/// Hermes command's reply is Markdown too, drawn as the automation unit draws it.
pub struct ThemedCells<'a> {
    /// The theme the cells started under.
    pub theme: &'a Theme,
}

/// The base style of reasoning text: the skin's thinking colour, else dim as in phase 1.
pub fn reasoning_base(theme: &Theme) -> Style {
    if theme.reasoning.fg.is_some() {
        theme.reasoning
    } else {
        theme.reasoning.add_modifier(Modifier::DIM)
    }
}

impl CellLines for ThemedCells<'_> {
    fn lines(&self, cell: &Cell, width: u16) -> Vec<Line<'static>> {
        let (text, base_style) = match cell {
            Cell::Assistant(cell) => (&cell.text, Style::default()),
            Cell::Reasoning(cell) => (&cell.text, reasoning_base(self.theme)),
            Cell::HermesCommand(cell) => {
                return hermes_command_lines_with_options(
                    cell,
                    width,
                    &AutomationTheme::default(),
                    &MarkdownOptions {
                        width,
                        theme: self.theme.markdown.clone(),
                        color: self.theme.color,
                        base_style: Style::default(),
                    },
                );
            }
            _ => return PlainCellLines.lines(cell, width),
        };
        render_markdown(
            text,
            &MarkdownOptions {
                width,
                theme: self.theme.markdown.clone(),
                color: self.theme.color,
                base_style,
            },
        )
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
        let themed = ThemedCells { theme: &theme };
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
