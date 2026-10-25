//! Shell and side cells and the side-tasks section.
//!
//! Shell output is untrusted text and is written literally; a side answer is model output and is
//! rendered as Markdown. Both are cleaned by core's `clean_output` first, and every line here then
//! goes through the history sanitiser, so no escape byte from either reaches the terminal.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use unicode_segmentation::UnicodeSegmentation;

use talaria_core::side::{
    Origin, PendingKind, ShellCell, ShellOutcome, SideCell, SideOutcome, SideTasks, clean_output,
    format_duration, next_tick, shell_outcome_text, side_outcome_text,
};

use crate::history::{grapheme_width, sanitize, sanitize_line, wrap_lines};
use crate::live::LiveSection;
use crate::markdown::{MarkdownOptions, render_markdown};

/// The composer title while the draft starts with `!`.
pub const SHELL_MODE_TITLE: &str = "shell \u{b7} runs in the session folder on the gateway host \u{b7} the agent does not see the output";

/// Hermes keeps at most this many code points of standard output.
const STDOUT_KEPT: usize = 4_000;
/// Hermes keeps at most this many code points of standard error.
const STDERR_KEPT: usize = 2_000;
const STDOUT_TRIMMED: &str =
    "(Hermes keeps at most the last 4,000 characters of output; earlier output may be missing)";
const STDERR_TRIMMED: &str = "(Hermes keeps at most the last 2,000 characters of error output; earlier output may be missing)";
const NO_OUTPUT: &str = "(no output)";
const NO_ANSWER: &str = "(no answer)";
const SIDE_FOOTER: &str = "side answer \u{b7} not part of the conversation";
/// Body rows and header continuation rows start this far in.
const INDENT: &str = "  ";
/// At most this many pending rows; the rest are counted on one more row.
const MAX_TASK_ROWS: usize = 3;

/// Warning, error and dim styles for side content.
#[derive(Debug, Clone)]
pub struct SideTheme {
    pub warning: Style,
    pub error: Style,
    pub dim: Style,
}

impl Default for SideTheme {
    fn default() -> Self {
        Self {
            warning: Style::default().fg(Color::Yellow),
            error: Style::default().fg(Color::Red),
            dim: Style::default().add_modifier(Modifier::DIM),
        }
    }
}

/// The lines of one shell cell at this width.
pub fn shell_cell_lines(cell: &ShellCell, width: u16, theme: &SideTheme) -> Vec<Line<'static>> {
    let width = width.max(1);
    let mut lines = shell_header(cell, width, theme);
    let ShellOutcome::Ok {
        stdout,
        stderr,
        code,
    } = &cell.outcome
    else {
        let text = shell_outcome_text(&cell.outcome).unwrap_or_default();
        let style = match cell.outcome {
            ShellOutcome::Unknown | ShellOutcome::Unreadable => theme.warning,
            _ => theme.error,
        };
        lines.extend(body(&text, width, style));
        return lines;
    };
    let out = clean_output(stdout);
    let err = clean_output(stderr);
    if stdout.chars().count() >= STDOUT_KEPT {
        lines.extend(body(STDOUT_TRIMMED, width, theme.dim));
    }
    if !out.trim().is_empty() {
        lines.extend(body(&out, width, Style::default()));
    }
    if stderr.chars().count() >= STDERR_KEPT {
        lines.extend(body(STDERR_TRIMMED, width, theme.dim));
    }
    if !err.trim().is_empty() {
        lines.extend(body(&err, width, theme.warning));
    }
    if *code != 0 {
        let exit = if *code < 0 {
            format!("exit {code} (stopped by signal {})", code.unsigned_abs())
        } else {
            format!("exit {code}")
        };
        lines.extend(body(&exit, width, theme.error));
    } else if out.trim().is_empty() && err.trim().is_empty() {
        lines.extend(body(NO_OUTPUT, width, theme.dim));
    }
    lines
}

/// `! <first line>` and the extra-line count, wrapped beside the dim duration on row 1.
fn shell_header(cell: &ShellCell, width: u16, theme: &SideTheme) -> Vec<Line<'static>> {
    let mut command_lines = cell.command.lines();
    let first = command_lines.next().unwrap_or_default();
    let extra = command_lines.count();
    let mut header = format!("! {}", sanitize(first));
    match extra {
        0 => {}
        1 => header.push_str(" (+1 line)"),
        more => header.push_str(&format!(" (+{more} lines)")),
    }
    let duration = format_duration(cell.duration_ms);
    let reserved = u16::try_from(duration.len())
        .unwrap_or(u16::MAX)
        .saturating_add(1);
    let room = width.saturating_sub(reserved).max(1);
    let (head, rest) = split_at_width(&header, room);
    let used = text_width(&head);
    let gap = usize::from(width.saturating_sub(used).saturating_sub(reserved - 1));
    let mut lines = vec![Line::from(vec![
        Span::raw(head),
        Span::raw(" ".repeat(gap)),
        Span::styled(duration, theme.dim),
    ])];
    if !rest.is_empty() {
        lines.extend(indented(
            wrap_lines(vec![Line::from(rest)], width.saturating_sub(2)),
            Style::default(),
        ));
    }
    lines
}

/// The lines of one side cell at this width.
pub fn side_cell_lines(cell: &SideCell, width: u16, theme: &SideTheme) -> Vec<Line<'static>> {
    let width = width.max(1);
    let mut header = format!("btw \u{b7} {}", cell.question);
    if cell.origin == Origin::Elsewhere {
        header.push_str(" \u{b7} asked in another window");
    }
    if cell.late {
        header.push_str(" \u{b7} late answer");
    }
    let header = header.replace('\n', " ");
    let (head, rest) = split_at_width(&sanitize(&header), width);
    let mut lines = vec![Line::from(head)];
    if !rest.is_empty() {
        lines.extend(indented(
            wrap_lines(vec![Line::from(rest)], width.saturating_sub(2)),
            Style::default(),
        ));
    }
    let SideOutcome::Answered { text } = &cell.outcome else {
        let text = side_outcome_text(&cell.outcome).unwrap_or_default();
        lines.extend(body(&text, width, theme.error));
        return lines;
    };
    let answer = clean_output(text);
    if answer.trim().is_empty() {
        lines.extend(body(NO_ANSWER, width, theme.dim));
    } else {
        let options = MarkdownOptions {
            width: width.saturating_sub(2).max(1),
            ..MarkdownOptions::default()
        };
        lines.extend(render_markdown(&answer, &options).into_iter().map(|line| {
            let style = line.style;
            let mut spans = vec![Span::raw(INDENT)];
            spans.extend(line.spans);
            sanitize_line(Line::from(spans).style(style))
        }));
    }
    lines.extend(body(SIDE_FOOTER, width, theme.dim));
    lines
}

/// Text split on line breaks, sanitised, wrapped by character two columns in, all in one style.
fn body(text: &str, width: u16, style: Style) -> Vec<Line<'static>> {
    let rows = text
        .split('\n')
        .map(|line| Line::from(sanitize(line).into_owned()))
        .collect();
    indented(wrap_lines(rows, width.saturating_sub(2)), style)
}

fn indented(rows: Vec<Line<'static>>, style: Style) -> Vec<Line<'static>> {
    rows.into_iter()
        .map(|row| {
            let mut spans = vec![Span::raw(INDENT)];
            spans.extend(row.spans.into_iter().map(|span| span.patch_style(style)));
            Line::from(spans)
        })
        .collect()
}

fn text_width(text: &str) -> u16 {
    text.graphemes(true).fold(0u16, |used, grapheme| {
        used.saturating_add(grapheme_width(grapheme))
    })
}

/// The longest leading part of `text` that fits `width` columns, and the rest.
fn split_at_width(text: &str, width: u16) -> (String, String) {
    let mut used: u16 = 0;
    for (index, grapheme) in text.grapheme_indices(true) {
        let columns = grapheme_width(grapheme);
        if used > 0 && used.saturating_add(columns) > width {
            return (text[..index].to_string(), text[index..].to_string());
        }
        used = used.saturating_add(columns);
    }
    (text.to_string(), String::new())
}

/// `text` cut to `width` columns, ending in `…` when anything was cut.
fn ellipsize(text: &str, width: u16) -> String {
    if text_width(text) <= width {
        return text.to_string();
    }
    let (head, _) = split_at_width(text, width.saturating_sub(1));
    let head = if text_width(&head) >= width {
        String::new()
    } else {
        head
    };
    format!("{head}\u{2026}")
}

/// The pending shell and side rows above the composer.
pub struct SideTasksSection<'a> {
    pub tasks: &'a SideTasks,
    pub now_ms: u64,
    pub theme: &'a SideTheme,
}

impl SideTasksSection<'_> {
    /// When the next row changes, if any row is pending.
    pub fn next_tick(&self) -> Option<u64> {
        next_tick(self.tasks, self.now_ms)
    }

    fn rows(&self) -> Vec<String> {
        let pending = &self.tasks.pending;
        let mut rows: Vec<String> = pending
            .iter()
            .take(MAX_TASK_ROWS)
            .map(|row| {
                let seconds = self.now_ms.saturating_sub(row.started_ms) / 1_000;
                match &row.kind {
                    PendingKind::Shell { command } => format!(
                        " \u{25cc} ! {} \u{b7} running \u{b7} {seconds}s",
                        command.lines().next().unwrap_or_default()
                    ),
                    PendingKind::Side { question, .. } => format!(
                        " \u{25cc} btw \u{b7} {} \u{b7} answering \u{b7} {seconds}s",
                        question.lines().next().unwrap_or_default()
                    ),
                }
            })
            .collect();
        if pending.len() > MAX_TASK_ROWS {
            rows.push(format!(" \u{25cc} +{} more", pending.len() - MAX_TASK_ROWS));
        }
        rows
    }
}

impl LiveSection for SideTasksSection<'_> {
    fn height(&self, _width: u16) -> u16 {
        let shown = self.tasks.pending.len().min(MAX_TASK_ROWS)
            + usize::from(self.tasks.pending.len() > MAX_TASK_ROWS);
        u16::try_from(shown).unwrap_or(u16::MAX)
    }

    fn render(&self, area: Rect, buf: &mut Buffer) {
        for (offset, row) in (0..area.height).zip(self.rows()) {
            let line = Line::styled(ellipsize(&sanitize(&row), area.width), self.theme.dim);
            buf.set_line(area.x, area.y + offset, &line, area.width);
        }
    }
}

/// The composer title for a shell draft, if the draft opens one.
pub fn shell_mode_title(draft: &str) -> Option<&'static str> {
    draft.starts_with('!').then_some(SHELL_MODE_TITLE)
}
