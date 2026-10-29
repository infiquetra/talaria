//! Transcript cell lines. Issue 222.
//!
//! Every cell the screen knows is drawn here. A cell kind this screen does not know is drawn by
//! the renderer the application passes in, so a later kind never breaks the build.

use ratatui::style::{Color, Modifier, Style};
use ratatui::text::Line;
use talaria_core::session::{
    AssistantCell, Cell, NoticeCell, PromptCell, PromptState, ReasoningCell, Settlement, ToolCell,
    ToolStatus, UserCell,
};

use crate::history::{CellLines, wrap_lines};
use crate::markdown::{MarkdownOptions, render_markdown};

/// Expanded tool bodies, or the compact header.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Details {
    /// Headers and bodies.
    Expanded,
    /// Headers, and a line count where a body was hidden.
    Compact,
}

/// Lines for one transcript cell.
pub struct PagerCellLines<'a> {
    /// Expanded or compact.
    pub details: Details,
    /// Markdown options for assistant and reasoning text.
    pub markdown: MarkdownOptions,
    /// Colours and modifiers for the transcript.
    pub theme: PagerTheme,
    /// Lines for a cell kind this screen does not know.
    pub other: &'a dyn CellLines,
}

impl CellLines for PagerCellLines<'_> {
    fn lines(&self, cell: &Cell, width: u16) -> Vec<Line<'static>> {
        // The wildcard arm keeps a cell kind added later from breaking the build.
        #[allow(unreachable_patterns)]
        match cell {
            Cell::User(inner) => self.user_lines(inner, width),
            Cell::Assistant(inner) => self.assistant_lines(inner, width),
            Cell::Reasoning(inner) => self.reasoning_lines(inner, width),
            Cell::Tool(inner) => self.tool_lines(inner, width),
            Cell::Notice(inner) => self.notice_lines(inner, width),
            Cell::Prompt(inner) => self.prompt_lines(inner, width),
            _ => self.other.lines(cell, width),
        }
    }
}

impl PagerCellLines<'_> {
    /// The columns the text of a cell may use, leaving room for the indent.
    fn inner_width(&self, width: u16) -> u16 {
        width.saturating_sub(2).max(1)
    }

    /// A message the person sent.
    fn user_lines(&self, cell: &UserCell, width: u16) -> Vec<Line<'static>> {
        let raw: Vec<Line<'static>> = cell
            .text
            .split('\n')
            .map(|piece| Line::from(piece.strip_suffix('\r').unwrap_or(piece).to_string()))
            .collect();
        let wrapped = wrap_lines(
            raw.into_iter().map(crate::history::sanitize_line).collect(),
            self.inner_width(width),
        );
        let mut out = Vec::with_capacity(wrapped.len());
        for (index, line) in wrapped.into_iter().enumerate() {
            let text = line
                .spans
                .iter()
                .map(|span| span.content.as_ref())
                .collect::<String>();
            let text = if index == 0 {
                format!("\u{203a} {text}")
            } else {
                format!("  {text}")
            };
            out.push(Line::styled(text, self.theme.user));
        }
        out
    }

    /// One assistant segment.
    fn assistant_lines(&self, cell: &AssistantCell, width: u16) -> Vec<Line<'static>> {
        let mut options = self.markdown.clone();
        options.width = self.inner_width(width);
        if cell.interim {
            options.base_style = options.base_style.patch(self.theme.dim);
        }
        let rendered = render_markdown(&cell.text, &options);
        let mut out: Vec<Line<'static>> =
            rendered.into_iter().map(|line| indent(line, 2)).collect();
        if cell.interim {
            out.push(Line::styled("  (interim)".to_string(), self.theme.dim));
        }
        out
    }

    /// One run of reasoning.
    fn reasoning_lines(&self, cell: &ReasoningCell, width: u16) -> Vec<Line<'static>> {
        let mut options = self.markdown.clone();
        options.width = self.inner_width(width);
        options.base_style = options.base_style.patch(self.theme.dim);
        let rendered = render_markdown(&cell.text, &options);
        let body: Vec<Line<'static>> = rendered.into_iter().map(|line| indent(line, 2)).collect();
        let mut out = vec![Line::styled(
            "\u{2234} thinking".to_string(),
            self.theme.dim,
        )];
        if self.details == Details::Compact {
            out.push(Line::styled(
                format!("  ({} lines)", body.len()),
                self.theme.dim,
            ));
        } else {
            out.extend(body);
        }
        out
    }

    /// One tool call.
    fn tool_lines(&self, cell: &ToolCell, _width: u16) -> Vec<Line<'static>> {
        let header = self.tool_header(cell);
        let body = self.tool_body(cell);
        if self.details == Details::Compact {
            let mut out = vec![header];
            if !body.is_empty() {
                out.push(Line::styled(
                    format!(" (+{} lines)", body.len()),
                    self.theme.dim,
                ));
            }
            return out;
        }
        let mut out = vec![header];
        out.extend(body);
        out
    }

    /// The header of a tool cell: the name, the outcome, the summary and the duration.
    fn tool_header(&self, cell: &ToolCell) -> Line<'static> {
        let mut text = format!("\u{23fa} {}", cell.name);
        if let Some(context) = &cell.context
            && !context.is_empty()
        {
            text.push('(');
            text.push_str(context);
            text.push(')');
        }
        text.push_str("  ");
        let (marker, style) = match cell.status {
            ToolStatus::Running => ("\u{2026} running", Style::default()),
            ToolStatus::Succeeded => ("\u{2713}", self.theme.tool_ok),
            ToolStatus::Failed => ("\u{2717}", self.theme.tool_failed),
            ToolStatus::Unfinished => ("\u{2298} unfinished", Style::default()),
            ToolStatus::Unknown => ("? unknown", self.theme.dim),
        };
        text.push_str(marker);
        if let Some(summary) = &cell.summary {
            text.push(' ');
            text.push_str(summary);
        }
        if let Some(duration) = cell.duration_s
            && let Some(formatted) = format_duration_fallback(duration)
        {
            text.push_str(" \u{b7} ");
            text.push_str(&formatted);
        }
        Line::styled(text, style)
    }

    /// The body of a tool cell: the arguments, the output and the diff.
    fn tool_body(&self, cell: &ToolCell) -> Vec<Line<'static>> {
        let mut out = Vec::new();
        if let Some(args) = &cell.args
            && !args.as_object().is_some_and(serde_json::Map::is_empty)
        {
            out.push(Line::styled("    args:".to_string(), self.theme.dim));
            let pretty = serde_json::to_string_pretty(args).unwrap_or_else(|_| args.to_string());
            for piece in pretty.split('\n') {
                out.push(Line::styled(format!("      {piece}"), Style::default()));
            }
        }
        if let Some(result) = &cell.result {
            out.push(Line::styled("    output:".to_string(), self.theme.dim));
            for piece in result.split('\n') {
                out.push(Line::styled(format!("      {piece}"), Style::default()));
            }
        }
        if let Some(diff) = &cell.diff {
            out.push(Line::styled("    diff:".to_string(), self.theme.dim));
            for piece in diff.split('\n') {
                let style = if piece.starts_with('+') {
                    self.theme.diff_add
                } else if piece.starts_with('-') {
                    self.theme.diff_del
                } else {
                    Style::default()
                };
                out.push(Line::styled(format!("      {piece}"), style));
            }
        }
        if cell.result.is_none() && cell.diff.is_none() {
            let text = if cell.status == ToolStatus::Running {
                "(running\u{2026})".to_string()
            } else {
                "(Hermes sent no output for this call)".to_string()
            };
            out.push(Line::styled(format!("    {text}"), self.theme.dim));
        }
        out
    }

    /// A local notice.
    fn notice_lines(&self, cell: &NoticeCell, _width: u16) -> Vec<Line<'static>> {
        let style = if cell.kind == talaria_core::session::NoticeKind::Error {
            self.theme.tool_failed
        } else {
            self.theme.dim
        };
        let mut out = Vec::new();
        for (index, piece) in cell.text.split('\n').enumerate() {
            let text = if index == 0 {
                format!("! {piece}")
            } else {
                piece.to_string()
            };
            out.push(Line::styled(text, style));
        }
        out
    }

    /// An approval or a question.
    fn prompt_lines(&self, cell: &PromptCell, _width: u16) -> Vec<Line<'static>> {
        let mut out = Vec::new();
        let header = match cell.kind {
            talaria_core::session::PromptKind::Approval => format!(
                "\u{2691} approval: {}",
                cell.command
                    .as_deref()
                    .or(cell.description.as_deref())
                    .or(cell.question.as_deref())
                    .unwrap_or("")
            ),
            talaria_core::session::PromptKind::Clarify => format!(
                "\u{2691} question: {}",
                cell.question
                    .as_deref()
                    .or(cell.description.as_deref())
                    .or(cell.command.as_deref())
                    .unwrap_or("")
            ),
        };
        out.push(Line::styled(header, self.theme.banner));
        if self.details == Details::Expanded {
            if let Some(description) = &cell.description {
                out.push(Line::styled(description.clone(), Style::default()));
            }
            for choice in &cell.choices {
                out.push(Line::styled(choice.clone(), Style::default()));
            }
        }
        out.push(Line::styled(prompt_state(&cell.state), self.theme.dim));
        out
    }
}

/// The state line of a prompt.
fn prompt_state(state: &PromptState) -> String {
    match state {
        PromptState::Open => "waiting for you".to_string(),
        PromptState::Answering(answer) => format!("answering: {answer}"),
        PromptState::Settled(Settlement::Answered(answer)) => format!("answered: {answer}"),
        PromptState::Settled(Settlement::Elsewhere) => "answered elsewhere".to_string(),
    }
}

/// The line with every line pushed to the right.
fn indent(line: Line<'static>, columns: usize) -> Line<'static> {
    let mut spans = vec![ratatui::text::Span::raw(" ".repeat(columns))];
    spans.extend(line.spans);
    let mut out = Line::from(spans);
    out.style = line.style;
    out.alignment = line.alignment;
    out
}

/// Styles for the transcript. Colour is dropped later when the Markdown options disable it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PagerTheme {
    /// Header row.
    pub header: Style,
    /// Footer row.
    pub footer: Style,
    /// Banner row.
    pub banner: Style,
    /// A message the person sent.
    pub user: Style,
    /// Dimmed text.
    pub dim: Style,
    /// A tool that succeeded.
    pub tool_ok: Style,
    /// A tool that failed, and an error notice.
    pub tool_failed: Style,
    /// An added diff line.
    pub diff_add: Style,
    /// A removed diff line.
    pub diff_del: Style,
    /// A search match that is not the current one.
    pub search_match: Style,
    /// The current search match.
    pub search_current: Style,
    /// The gutter marker on a cell whose rendered text cannot host the current match.
    pub gutter_mark: Style,
}

impl Default for PagerTheme {
    fn default() -> Self {
        let reversed = Style::default().add_modifier(Modifier::REVERSED);
        let bold = Style::default().add_modifier(Modifier::BOLD);
        Self {
            header: reversed,
            footer: reversed,
            banner: bold,
            user: bold,
            dim: Style::default().add_modifier(Modifier::DIM),
            tool_ok: Style::default().fg(Color::Green),
            tool_failed: Style::default().fg(Color::Red),
            diff_add: Style::default().fg(Color::Green),
            diff_del: Style::default().fg(Color::Red),
            search_match: reversed,
            search_current: reversed.add_modifier(Modifier::BOLD | Modifier::UNDERLINED),
            gutter_mark: bold,
        }
    }
}

/// `0.4s` under a minute, `2m05s` at a minute and beyond.
///
/// Absent, negative and non-finite durations have no text.
#[must_use]
pub fn format_duration_fallback(seconds: f64) -> Option<String> {
    if !seconds.is_finite() || seconds < 0.0 {
        return None;
    }
    let tenths = (seconds * 10.0).round();
    if tenths < 600.0 {
        let whole = tenths as u64;
        Some(format!("{}.{}s", whole / 10, whole % 10))
    } else {
        let whole = seconds.round() as u64;
        Some(format!("{}m{:02}s", whole / 60, whole % 60))
    }
}
