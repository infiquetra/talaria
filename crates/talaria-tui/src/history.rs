//! Pure history rendering: cells to lines, sanitizing, wrapping and the commit ledger.

use std::borrow::Cow;

use ratatui::buffer::CellWidth;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use talaria_core::session::{Cell, PromptCell, PromptState, Settlement, ToolCell, ToolStatus};
pub use talaria_core::text::sanitize;
use unicode_segmentation::UnicodeSegmentation;

/// Turns one cell into lines for the given width.
pub trait CellLines {
    fn lines(&self, cell: &Cell, width: u16) -> Vec<Line<'static>>;
}

/// One plain rendering per cell variant.
pub struct PlainCellLines;

impl CellLines for PlainCellLines {
    fn lines(&self, cell: &Cell, width: u16) -> Vec<Line<'static>> {
        let bold = Style::default().add_modifier(Modifier::BOLD);
        let dim = Style::default().add_modifier(Modifier::DIM);
        match cell {
            Cell::User(user) => prefixed("> ", &user.text, bold),
            Cell::Assistant(assistant) => prefixed("", &assistant.text, Style::default()),
            Cell::Reasoning(reasoning) => prefixed("", &reasoning.text, dim),
            Cell::Tool(tool) => vec![Line::from(tool_line(tool))],
            Cell::Notice(notice) => prefixed("\u{b7} ", &notice.text, dim),
            Cell::Prompt(prompt) => vec![Line::styled(prompt_line(prompt), dim)],
            Cell::Shell(shell) => {
                crate::side::shell_cell_lines(shell, width, &crate::side::SideTheme::default())
            }
            Cell::Side(side) => {
                crate::side::side_cell_lines(side, width, &crate::side::SideTheme::default())
            }
            Cell::HermesCommand(command) => {
                let typed = Style::default().add_modifier(Modifier::DIM | Modifier::BOLD);
                let mut lines = vec![Line::styled(command.typed.clone(), typed)];
                for piece in split_lines(&command.body) {
                    lines.push(Line::styled(format!("  {piece}"), Style::default()));
                }
                lines
            }
            Cell::Background(cell) => crate::background::background_cell_lines(
                cell,
                width,
                &crate::background::BackgroundTheme::default(),
            ),
        }
    }
}

/// The text split into one line per source line, the prefix on the first, all in one style.
fn prefixed(prefix: &str, text: &str, style: Style) -> Vec<Line<'static>> {
    split_lines(text)
        .enumerate()
        .map(|(index, piece)| {
            let text = if index == 0 {
                format!("{prefix}{piece}")
            } else {
                piece.to_string()
            };
            Line::styled(text, style)
        })
        .collect()
}

/// Split on `\n`, with `\r\n` counting as one break. A lone `\r` stays in the text.
pub(crate) fn split_lines(text: &str) -> impl Iterator<Item = &str> {
    text.split('\n')
        .map(|piece| piece.strip_suffix('\r').unwrap_or(piece))
}

fn tool_line(tool: &ToolCell) -> String {
    let mut line = format!("\u{23fa} {}", tool.name);
    if let Some(summary) = &tool.summary {
        line.push_str(": ");
        line.push_str(summary);
    }
    match tool.status {
        ToolStatus::Failed => line.push_str(" (failed)"),
        ToolStatus::Unfinished => line.push_str(" (unfinished)"),
        ToolStatus::Unknown => line.push_str(" (unknown)"),
        ToolStatus::Running | ToolStatus::Succeeded => {}
    }
    line
}

fn prompt_line(prompt: &PromptCell) -> String {
    let asked = prompt
        .command
        .as_deref()
        .or(prompt.question.as_deref())
        .or(prompt.description.as_deref())
        .unwrap_or("");
    let mut line = format!("? {asked}");
    match &prompt.state {
        PromptState::Settled(Settlement::Answered(answer)) => {
            line.push_str(" \u{2192} ");
            line.push_str(answer);
        }
        PromptState::Settled(Settlement::Elsewhere) => {
            line.push_str(" \u{2192} answered elsewhere");
        }
        PromptState::Open | PromptState::Answering(_) => {}
    }
    line
}

/// Remembers which cells were already handed out.
pub struct CommitLedger {
    next: usize,
}

impl Default for CommitLedger {
    fn default() -> Self {
        Self::new()
    }
}

impl CommitLedger {
    pub fn new() -> Self {
        Self { next: 0 }
    }

    /// The longest run of final cells after the last one handed out. The ledger moves past them
    /// before returning, so a write that fails never hands the same cells out twice.
    pub fn take_ready<'a>(&mut self, cells: &'a [Cell]) -> &'a [Cell] {
        let start = self.next.min(cells.len());
        let ready = cells[start..]
            .iter()
            .take_while(|cell| cell.is_final())
            .count();
        self.next = start + ready;
        &cells[start..self.next]
    }

    /// A new foreground session starts again from the first cell.
    pub fn reset(&mut self) {
        self.next = 0;
    }
}

/// The columns a grapheme takes, by the rule ratatui's buffer uses for its cells. A grapheme
/// holding a control character takes none, as the buffer drops it.
pub(crate) fn grapheme_width(grapheme: &str) -> u16 {
    if grapheme.contains(char::is_control) {
        0
    } else {
        grapheme.cell_width()
    }
}

/// Wrap by grapheme with the terminal's width rule. A grapheme that does not fit moves to the
/// next row whole, an empty line is one empty row, and a width of 0 counts as 1.
pub fn wrap_lines(lines: Vec<Line<'static>>, width: u16) -> Vec<Line<'static>> {
    let width = width.max(1);
    let mut rows = Vec::new();
    for line in lines {
        let style = line.style;
        let alignment = line.alignment;
        let row_of = |spans: Vec<Span<'static>>| {
            let mut row = Line::from(spans).style(style);
            row.alignment = alignment;
            row
        };
        let mut current: Vec<Span<'static>> = Vec::new();
        let mut used: u16 = 0;
        for span in line.spans {
            let mut piece = String::new();
            for grapheme in span.content.graphemes(true) {
                let columns = grapheme_width(grapheme);
                if used > 0 && used.saturating_add(columns) > width {
                    if !piece.is_empty() {
                        current.push(Span::styled(std::mem::take(&mut piece), span.style));
                    }
                    rows.push(row_of(std::mem::take(&mut current)));
                    used = 0;
                }
                piece.push_str(grapheme);
                used = used.saturating_add(columns);
            }
            if !piece.is_empty() {
                current.push(Span::styled(piece, span.style));
            }
        }
        rows.push(row_of(current));
    }
    rows
}

/// The line with every span sanitized.
pub(crate) fn sanitize_line(mut line: Line<'static>) -> Line<'static> {
    for span in &mut line.spans {
        if let Cow::Owned(clean) = sanitize(&span.content) {
            span.content = Cow::Owned(clean);
        }
    }
    line
}
