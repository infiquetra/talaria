//! Pure history rendering: cells to lines, sanitizing, wrapping and the commit ledger.

use std::borrow::Cow;

use ratatui::buffer::CellWidth;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use talaria_core::session::{
    Cell, NoticeCell, PromptCell, PromptState, Settlement, ToolCell, ToolStatus,
};
pub use talaria_core::text::{is_neutralized, sanitize};
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
            Cell::User(user) => prefixed_wrapped("> ", &user.text, bold, width),
            Cell::Assistant(assistant) => prefixed("", &assistant.text, Style::default()),
            Cell::Reasoning(reasoning) => prefixed("", &reasoning.text, dim),
            Cell::Tool(tool) => vec![Line::from(tool_line(tool))],
            Cell::Notice(notice) => {
                prefixed_wrapped("\u{b7} ", &notice_text(notice, None), dim, width)
            }
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

/// What a notice shows on the main screen: its text and, when it carries what Hermes handed the
/// agent, how many lines of that the transcript view holds. `transcript_key` is the label of the
/// key that opens the transcript view, when one is bound; without it the hint names the view. The
/// body itself never prints here.
pub(crate) fn notice_text<'a>(
    notice: &'a NoticeCell,
    transcript_key: Option<&str>,
) -> Cow<'a, str> {
    match notice_body(notice) {
        Some(body) => {
            let count = split_lines(body).count();
            let noun = if count == 1 { "line" } else { "lines" };
            let place = transcript_key.map_or_else(
                || " in transcript view".to_owned(),
                |key| format!(", {key} to view"),
            );
            Cow::Owned(format!("{} (+{count} {noun}{place})", notice.text))
        }
        None => Cow::Borrowed(notice.text.as_str()),
    }
}

/// What Hermes handed the agent, as a notice shows it: nothing when it only repeats the text.
pub(crate) fn notice_body(notice: &NoticeCell) -> Option<&str> {
    notice
        .body
        .as_deref()
        .filter(|body| body.trim() != notice.text)
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

/// The text word-wrapped to `width`, the two-column prefix on the first row and a two-space
/// hanging indent on every other row, all in one style. Rows break after a space; a token longer
/// than a row, such as a path, still breaks mid-token. Spacing inside a row is kept as typed.
pub(crate) fn prefixed_wrapped(
    prefix: &str,
    text: &str,
    style: Style,
    width: u16,
) -> Vec<Line<'static>> {
    let inner = usize::from(width).saturating_sub(2).max(1);
    let mut lines = Vec::new();
    for piece in split_lines(text) {
        let piece = sanitize(piece);
        let rows = crate::composer::wrap::wrap(&piece, inner);
        let last = rows.len().saturating_sub(1);
        for (index, row) in rows.iter().enumerate() {
            let mut body = &piece[row.start..row.end];
            if index > 0 {
                body = body.trim_start_matches(' ');
            }
            if index < last {
                body = body.trim_end_matches(' ');
            }
            // Only an empty source line keeps an empty row; a row left empty by trimming,
            // such as trailing spaces pushed onto a row of their own, is dropped.
            if body.is_empty() && last > 0 {
                continue;
            }
            let row = match (lines.is_empty(), body.is_empty()) {
                (true, _) => format!("{prefix}{body}"),
                (false, true) => String::new(),
                (false, false) => format!("  {body}"),
            };
            lines.push(Line::styled(row, style));
        }
    }
    lines
}

/// The text word-wrapped to `width` with no prefix or indent, one entry per row. Rows break
/// after a space; a token longer than a row still breaks mid-token. An empty source line keeps
/// one empty row.
pub(crate) fn word_wrapped(text: &str, width: u16) -> Vec<String> {
    let width = usize::from(width).max(1);
    let mut lines = Vec::new();
    for piece in split_lines(text) {
        let piece = sanitize(piece);
        let rows = crate::composer::wrap::wrap(&piece, width);
        let last = rows.len().saturating_sub(1);
        let before = lines.len();
        for (index, row) in rows.iter().enumerate() {
            let mut body = &piece[row.start..row.end];
            if index > 0 {
                body = body.trim_start_matches(' ');
            }
            if index < last {
                body = body.trim_end_matches(' ');
            }
            if !body.is_empty() {
                lines.push(body.to_owned());
            }
        }
        if lines.len() == before {
            lines.push(String::new());
        }
    }
    lines
}

/// Plain text lines the app commits, such as connect, switch and branch notices, as rows for
/// `width`.
///
/// A line that fits is kept as it is. A longer line that starts with the notice marker `· ` is
/// word-wrapped with a two-space hanging indent, like a notice cell; any other longer line is
/// word-wrapped with no indent. A line whose shape carries meaning is left to the plain hard
/// wrap: one that starts with whitespace, one whose text under the marker starts with whitespace,
/// one that holds a box-drawing character (a table or a rule), and one that holds a control
/// character.
pub fn committed_text_rows(lines: Vec<String>, width: u16) -> Vec<Line<'static>> {
    const MARKER: &str = "\u{b7} ";
    let width = width.max(1);
    let mut rows = Vec::new();
    for line in lines {
        let columns: usize = line
            .graphemes(true)
            .map(|grapheme| usize::from(grapheme_width(grapheme)))
            .sum();
        let preformatted = line.starts_with(char::is_whitespace)
            || line
                .strip_prefix(MARKER)
                .is_some_and(|text| text.starts_with(char::is_whitespace))
            || line
                .chars()
                .any(|c| c.is_control() || ('\u{2500}'..='\u{257f}').contains(&c));
        if columns <= usize::from(width) || preformatted {
            rows.extend(wrap_lines(vec![Line::from(line)], width));
        } else if let Some(text) = line.strip_prefix(MARKER) {
            rows.extend(prefixed_wrapped(MARKER, text, Style::default(), width));
        } else {
            rows.extend(word_wrapped(&line, width).into_iter().map(Line::from));
        }
    }
    rows
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
        PromptState::Settled(Settlement::Sent) => line.push_str(" \u{2192} answer sent"),
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
            if span.content.is_empty() {
                continue;
            }
            let bytes = span.content.as_bytes();
            if bytes.iter().all(|&b| (0x20..=0x7e).contains(&b)) {
                if used >= width {
                    rows.push(row_of(std::mem::take(&mut current)));
                    used = 0;
                }
                if bytes.len() <= usize::from(width - used) {
                    used += bytes.len() as u16;
                    current.push(span);
                    continue;
                }
                let mut offset = 0;
                while offset < bytes.len() {
                    let take = (bytes.len() - offset).min(usize::from(width - used));
                    current.push(Span::styled(
                        span.content[offset..offset + take].to_owned(),
                        span.style,
                    ));
                    offset += take;
                    used += take as u16;
                    if offset < bytes.len() {
                        rows.push(row_of(std::mem::take(&mut current)));
                        used = 0;
                    }
                }
                continue;
            }
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
