//! The agent's task checklist: the live panel and the committed block.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use talaria_core::checklist::{
    Checklist, ChecklistCommit, CommitKind, TodoItem, TodoStatus, TreeRow,
};
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

use crate::history::sanitize;
use crate::live::LiveSection;

/// Most rows the live panel ever shows.
pub const PANEL_MAX_ROWS: u16 = 8;

/// Below this many terminal rows the panel is always collapsed.
pub const SHORT_TERMINAL_ROWS: u16 = 16;

/// Most item rows one committed block shows.
pub const BLOCK_MAX_ITEMS: usize = 50;

/// The deepest level that still gets more indentation.
const MAX_DEPTH: u8 = 4;

/// Below this width the live panel is only its header.
const NARROW_WIDTH: u16 = 12;

/// Styles for the checklist panel and block.
#[derive(Debug, Clone)]
pub struct ChecklistStyle {
    pub header: Style,
    pub hint: Style,
    pub in_progress: Style,
    pub pending: Style,
    pub completed: Style,
    pub cancelled: Style,
    pub glyph_in_progress: Style,
    pub glyph_completed: Style,
}

impl ChecklistStyle {
    /// The full-colour styles.
    #[allow(clippy::should_implement_trait)]
    pub fn default() -> ChecklistStyle {
        ChecklistStyle {
            header: Style::default().add_modifier(Modifier::BOLD),
            hint: Style::default().add_modifier(Modifier::DIM),
            in_progress: Style::default().add_modifier(Modifier::BOLD),
            pending: Style::default(),
            completed: Style::default().add_modifier(Modifier::DIM),
            cancelled: Style::default().add_modifier(Modifier::DIM),
            glyph_in_progress: Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
            glyph_completed: Style::default()
                .fg(Color::Green)
                .add_modifier(Modifier::DIM),
        }
    }

    /// Bold and dim kept, every colour dropped.
    pub fn plain() -> ChecklistStyle {
        ChecklistStyle {
            header: Style::default().add_modifier(Modifier::BOLD),
            hint: Style::default().add_modifier(Modifier::DIM),
            in_progress: Style::default().add_modifier(Modifier::BOLD),
            pending: Style::default(),
            completed: Style::default().add_modifier(Modifier::DIM),
            cancelled: Style::default().add_modifier(Modifier::DIM),
            glyph_in_progress: Style::default().add_modifier(Modifier::BOLD),
            glyph_completed: Style::default().add_modifier(Modifier::DIM),
        }
    }
}

/// The live panel above the composer.
pub struct ChecklistSection<'a> {
    checklist: &'a Checklist,
    expanded: bool,
    max_rows: u16,
    hint: Option<String>,
    style: &'a ChecklistStyle,
}

impl<'a> ChecklistSection<'a> {
    /// Show this list in this form with this row budget.
    pub fn new(
        checklist: &'a Checklist,
        expanded: bool,
        max_rows: u16,
        hint: Option<String>,
        style: &'a ChecklistStyle,
    ) -> Self {
        Self {
            checklist,
            expanded,
            max_rows,
            hint,
            style,
        }
    }

    /// The panel rows at this width.
    pub fn lines(&self, width: u16) -> Vec<Line<'static>> {
        if width == 0 {
            return Vec::new();
        }
        if !self.expanded || self.max_rows <= 1 {
            return vec![self.collapsed_row(width)];
        }
        let header = self.header_row(width);
        if width < NARROW_WIDTH {
            return vec![header];
        }
        let tree = self.checklist.tree();
        let budget = usize::from(self.max_rows);
        let mut rows = vec![header];
        if tree.is_empty() {
            return rows;
        }
        if tree.len() < budget {
            for row in &tree {
                rows.push(self.item_row(*row, width));
            }
            return rows;
        }
        // The window shows the focus and what follows it; the rest is counted.
        let count = budget - 2;
        let start = self.window_start(count, tree.len());
        for row in tree.iter().skip(start).take(count) {
            rows.push(self.item_row(*row, width));
        }
        let hidden = tree.len() - count;
        rows.push(hint_row(
            &format!("  … +{hidden} more"),
            width,
            self.style.hint,
        ));
        rows
    }

    /// Where the item window starts, so the focus is in it.
    fn window_start(&self, count: usize, total: usize) -> usize {
        let start = match self.checklist.focus() {
            Some(focus) if count >= 2 => focus.saturating_sub(1),
            Some(focus) => focus,
            None => 0,
        };
        if start + count > total {
            total.saturating_sub(count)
        } else {
            start
        }
    }

    /// The expanded header: the counts, the cancelled ones and the key.
    fn header_row(&self, width: u16) -> Line<'static> {
        let counts = self.checklist.counts();
        let mut text = format!("▾ Tasks {}/{}", counts.completed, counts.total);
        if counts.cancelled > 0 {
            text.push_str(&format!(" · {} cancelled", counts.cancelled));
        }
        let hint = self
            .hint
            .as_ref()
            .map(|hint| format!(" · {hint} to collapse"));
        let with_hint = match &hint {
            Some(hint) => format!("{text}{hint}"),
            None => text.clone(),
        };
        if display_width(&with_hint) > usize::from(width) {
            // The key goes first; the counts stay.
            return Line::from(vec![Span::styled(
                cut_to_width(&text, width),
                self.style.header,
            )]);
        }
        let mut spans = vec![Span::styled(text, self.style.header)];
        if let Some(hint) = hint {
            spans.push(Span::styled(hint, self.style.hint));
        }
        Line::from(spans)
    }

    /// The one row that names the step in progress.
    fn collapsed_row(&self, width: u16) -> Line<'static> {
        let counts = self.checklist.counts();
        let base = format!("▸ Tasks {}/{}", counts.completed, counts.total);
        let cancelled = if counts.cancelled > 0 {
            Some(format!(" · {} cancelled", counts.cancelled))
        } else {
            None
        };
        let focus = self.focus_content();
        let hint = self
            .hint
            .as_ref()
            .map(|hint| format!(" · {hint} to expand"));

        let mut row = base.clone();
        if let Some(cancelled) = &cancelled {
            row.push_str(cancelled);
        }
        let with_focus = match &focus {
            Some((content, _)) => format!("{row} · {content}"),
            None => row.clone(),
        };
        let full = match &hint {
            Some(hint) => format!("{with_focus}{hint}"),
            None => with_focus.clone(),
        };
        if display_width(&full) <= usize::from(width) {
            return self.collapsed_line(&row, &focus, hint.as_ref());
        }
        // The key goes first.
        if display_width(&with_focus) <= usize::from(width) {
            return self.collapsed_line(&row, &focus, None);
        }
        // Then the focus content, cut to what is left of the row.
        let Some((content, style)) = &focus else {
            return Line::from(vec![Span::styled(
                cut_to_width(&with_focus, width),
                self.style.header,
            )]);
        };
        let keep = usize::from(width)
            .saturating_sub(display_width(&row) + 3)
            .max(10);
        let cut = cut_to_width(content, u16::try_from(keep).unwrap_or(u16::MAX));
        let shortened = format!("{row} · {cut}");
        if display_width(&shortened) <= usize::from(width) {
            return Line::from(vec![
                Span::styled(row, self.style.header),
                Span::styled(format!(" · {cut}"), *style),
            ]);
        }
        Line::from(vec![Span::styled(
            cut_to_width(&shortened, width),
            self.style.header,
        )])
    }

    /// The collapsed row from its parts.
    fn collapsed_line(
        &self,
        base: &str,
        focus: &Option<(String, Style)>,
        hint: Option<&String>,
    ) -> Line<'static> {
        let mut spans = vec![Span::styled(base.to_string(), self.style.header)];
        if let Some((content, style)) = focus {
            spans.push(Span::styled(format!(" · {content}"), *style));
        }
        if let Some(hint) = hint {
            spans.push(Span::styled(hint.clone(), self.style.hint));
        }
        Line::from(spans)
    }

    /// The content of the step in progress, made literal.
    fn focus_content(&self) -> Option<(String, Style)> {
        let position = self.checklist.focus()?;
        let tree = self.checklist.tree();
        let row = tree.get(position)?;
        let item = self.checklist.items.get(row.index)?;
        Some((literal(&item.content), self.item_style(item.status)))
    }

    /// One item row: its indentation, its glyph and its content.
    fn item_row(&self, row: TreeRow, width: u16) -> Line<'static> {
        let Some(item) = self.checklist.items.get(row.index) else {
            return Line::default();
        };
        item_line(item, row.depth, width, self.style)
    }

    /// The style of one item's text.
    fn item_style(&self, status: TodoStatus) -> Style {
        match status {
            TodoStatus::Pending => self.style.pending,
            TodoStatus::InProgress => self.style.in_progress,
            TodoStatus::Completed => self.style.completed,
            TodoStatus::Cancelled => self.style.cancelled,
        }
    }
}

impl LiveSection for ChecklistSection<'_> {
    fn height(&self, width: u16) -> u16 {
        u16::try_from(self.lines(width).len()).unwrap_or(u16::MAX)
    }

    fn render(&self, area: Rect, buf: &mut Buffer) {
        let rows = self.lines(area.width);
        for (offset, row) in (0..area.height).zip(rows.iter()) {
            buf.set_line(area.x, area.y + offset, row, area.width);
        }
    }
}

/// The scrollback block for one commit.
pub fn committed_block_lines(
    commit: &ChecklistCommit,
    width: u16,
    style: &ChecklistStyle,
) -> Vec<Line<'static>> {
    if width == 0 {
        return Vec::new();
    }
    let checklist = &commit.checklist;
    let counts = checklist.counts();
    let active = counts.pending + counts.in_progress;
    let mut header = format!("Tasks {}/{}", counts.completed, counts.total);
    if active > 0 {
        header.push_str(&format!(" · {active} not finished"));
    } else {
        header.push_str(" · done");
    }
    if counts.cancelled > 0 {
        header.push_str(&format!(", {} cancelled", counts.cancelled));
    }
    if commit.kind == CommitKind::Resumed {
        header.push_str(" (from before this resume)");
    }
    let mut rows = vec![Line::from(vec![Span::styled(
        cut_to_width(&header, width),
        style.header,
    )])];
    let tree = checklist.tree();
    for row in tree.iter().take(BLOCK_MAX_ITEMS) {
        let Some(item) = checklist.items.get(row.index) else {
            continue;
        };
        rows.push(item_line(item, row.depth, width, style));
    }
    if tree.len() > BLOCK_MAX_ITEMS {
        let hidden = tree.len() - BLOCK_MAX_ITEMS;
        rows.push(hint_row(&format!("  … +{hidden} more"), width, style.hint));
    }
    if checklist.skipped > 0 {
        let text = if checklist.skipped == 1 {
            "  (1 item could not be shown)".to_string()
        } else {
            format!("  ({} items could not be shown)", checklist.skipped)
        };
        rows.push(hint_row(&text, width, style.hint));
    }
    rows
}

/// One item row: the indentation, the glyph and the content, cut to the width.
fn item_line(item: &TodoItem, depth: u8, width: u16, style: &ChecklistStyle) -> Line<'static> {
    let text_style = match item.status {
        TodoStatus::Pending => style.pending,
        TodoStatus::InProgress => style.in_progress,
        TodoStatus::Completed => style.completed,
        TodoStatus::Cancelled => style.cancelled,
    };
    let glyph_style = match item.status {
        TodoStatus::InProgress => style.glyph_in_progress,
        TodoStatus::Completed => style.glyph_completed,
        _ => style.pending,
    };
    let indent = " ".repeat(indent_width(depth));
    let mark = glyph(item.status);
    let cut = cut_to_width(&format!("{indent}{mark} {}", literal(&item.content)), width);
    // The glyph keeps its own colour while the cut still shows it.
    if cut.starts_with(&indent) && cut[indent.len()..].starts_with(mark) {
        let head = indent.len() + mark.len();
        return Line::from(vec![
            Span::styled(indent, text_style),
            Span::styled(mark, glyph_style),
            Span::styled(cut[head..].to_string(), text_style),
        ]);
    }
    Line::from(vec![Span::styled(cut, text_style)])
}

/// Cut text to this display width on grapheme boundaries, ending `…`.
pub fn cut_to_width(text: &str, width: u16) -> String {
    if width == 0 {
        return String::new();
    }
    let limit = usize::from(width);
    if display_width(text) <= limit {
        return text.to_string();
    }
    let mut kept = String::new();
    let mut used = 0usize;
    for grapheme in text.graphemes(true) {
        let columns = display_width(grapheme);
        if used + columns > limit - 1 {
            break;
        }
        kept.push_str(grapheme);
        used += columns;
    }
    kept.push('…');
    kept
}

/// The columns a text takes.
fn display_width(text: &str) -> usize {
    UnicodeWidthStr::width(text)
}

/// The columns one item's indentation takes.
fn indent_width(depth: u8) -> usize {
    2 + 2 * usize::from(depth.min(MAX_DEPTH))
}

/// The glyph of one state.
fn glyph(status: TodoStatus) -> &'static str {
    match status {
        TodoStatus::Completed => "[x]",
        TodoStatus::InProgress => "[>]",
        TodoStatus::Pending => "[ ]",
        TodoStatus::Cancelled => "[-]",
    }
}

/// The content of one item, made literal: every run of line breaks is one space, and
/// the remaining control characters become visible stand-ins.
fn literal(content: &str) -> String {
    let mut plain = String::with_capacity(content.len());
    let mut in_break = false;
    for character in content.chars() {
        if matches!(character, '\r' | '\n' | '\t') {
            if !in_break {
                plain.push(' ');
                in_break = true;
            }
        } else {
            in_break = false;
            plain.push(character);
        }
    }
    sanitize(&plain).into_owned()
}

/// One dim row, cut to the width.
fn hint_row(text: &str, width: u16, style: Style) -> Line<'static> {
    Line::from(vec![Span::styled(cut_to_width(text, width), style)])
}
