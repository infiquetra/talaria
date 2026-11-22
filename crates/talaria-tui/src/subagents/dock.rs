//! The live subagent dock and shared row styles.
use super::labels::goal_labels;
use crate::{checklist::cut_to_width, history::sanitize, live::LiveSection};
use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::Style,
    text::{Line, Span},
};
use talaria_core::subagents::{
    Activity, ActivityKind, RowState, Spawning, SubagentCounts, SubagentRow, SubagentTree,
};
use unicode_width::UnicodeWidthStr;

#[derive(Clone, Debug)]
pub struct SubagentStyle {
    pub header: Style,
    pub hint: Style,
    pub running: Style,
    pub queued: Style,
    pub completed: Style,
    pub failed: Style,
    pub interrupted: Style,
    pub gone: Style,
}
impl Default for SubagentStyle {
    fn default() -> Self {
        crate::theme::legacy::subagent_style()
    }
}
impl SubagentStyle {
    pub fn plain() -> Self {
        Self {
            running: Style::new(),
            completed: Style::new(),
            failed: Style::new(),
            interrupted: Style::new(),
            ..Self::default()
        }
    }
    pub(super) fn state(&self, state: RowState) -> Style {
        match state {
            RowState::Running => self.running,
            RowState::Queued => self.queued,
            RowState::Completed => self.completed,
            RowState::Failed(_) => self.failed,
            RowState::Interrupted => self.interrupted,
            RowState::Gone => self.gone,
        }
    }
}
pub(super) fn literal(text: &str) -> String {
    let mut output = String::new();
    let mut separator = false;
    for c in text.chars() {
        if matches!(c, '\r' | '\n' | '\t') {
            if !separator {
                output.push(' ');
            }
            separator = true;
        } else {
            output.push(c);
            separator = false;
        }
    }
    sanitize(&output).into_owned()
}
/// Hermes sends one progress line per five tool calls, worded `🔀 [set 1 · 3/10] terminal, read_file`.
/// It repeats tool events that already arrived, so it is not the line to show first.
fn is_batch_progress(entry: &Activity) -> bool {
    entry.kind == ActivityKind::Progress && entry.text.trim_start().starts_with('🔀')
}
/// An activity line without Hermes's batch marker: no leading symbol and no `[set 1 · 3/10]` tag.
pub(super) fn clean_activity(entry: &Activity) -> String {
    let text = literal(&entry.text);
    if entry.kind != ActivityKind::Progress {
        return text;
    }
    let mut rest = text.trim_start();
    if let Some(after) = rest.strip_prefix('🔀') {
        rest = after.trim_start();
    }
    if let Some(tag) = rest.strip_prefix('[')
        && let Some((_, after)) = tag.split_once(']')
    {
        rest = after.trim_start();
    }
    rest.to_owned()
}
/// What the child is doing now: the newest tool, thinking or summary line.
/// Batched progress counts only when nothing else has arrived, with its marker removed.
pub(super) fn activity(row: &SubagentRow) -> String {
    if row.stop_requested {
        return "stopping".into();
    }
    row.activity
        .iter()
        .rev()
        .find(|a| !is_batch_progress(a))
        .or_else(|| row.activity.back())
        .map(clean_activity)
        .filter(|text| !text.is_empty())
        .unwrap_or_else(|| {
            if row.state == RowState::Queued {
                "queued"
            } else {
                "starting"
            }
            .into()
        })
}
pub(super) fn styled_row(
    text: String,
    glyph: char,
    state: RowState,
    width: u16,
    style: &SubagentStyle,
    selected: bool,
) -> Line<'static> {
    let text = cut_to_width(&text, width);
    let base = if selected {
        Style::new().reversed()
    } else {
        Style::new()
    };
    if let Some(at) = text.find(glyph) {
        let end = at + glyph.len_utf8();
        let mut line = Line::from(vec![
            Span::styled(text[..at].to_owned(), base),
            Span::styled(text[at..end].to_owned(), base.patch(style.state(state))),
            Span::styled(
                text[end..].to_owned(),
                if selected { base.bold() } else { base },
            ),
        ]);
        if let Some(at) = text.rfind(" · ") {
            dim_suffix(&mut line, &text[at..], base.patch(style.hint));
        }
        line
    } else {
        Line::styled(text, base)
    }
}
pub(super) fn dim_suffix(line: &mut Line<'static>, suffix: &str, style: Style) {
    if suffix.is_empty() {
        return;
    }
    if let Some(last) = line.spans.last_mut()
        && let Some(prefix) = last.content.strip_suffix(suffix)
    {
        last.content = prefix.to_owned().into();
        line.spans.push(Span::styled(suffix.to_owned(), style));
    }
}
pub struct SubagentDock<'a> {
    tree: &'a SubagentTree,
    counts: SubagentCounts,
    spawning: Spawning,
    open_hint: Option<&'a str>,
    style: &'a SubagentStyle,
    max_rows: u16,
    terminal_rows: u16,
}
impl<'a> SubagentDock<'a> {
    /// Use resolved styles for the existing dock projection.
    pub fn render_with_style(&self, area: Rect, buf: &mut Buffer, style: &SubagentStyle) {
        SubagentDock { style, ..*self }.render(area, buf);
    }
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        tree: &'a SubagentTree,
        counts: SubagentCounts,
        _key_label: Option<&'a str>,
        spawning: Spawning,
        open_hint: Option<&'a str>,
        style: &'a SubagentStyle,
        max_rows: u16,
        terminal_rows: u16,
    ) -> Self {
        Self {
            tree,
            counts,
            spawning,
            open_hint,
            style,
            max_rows,
            terminal_rows,
        }
    }
    pub fn lines(&self, width: u16) -> Vec<Line<'static>> {
        if width == 0 || self.max_rows == 0 || self.counts.live == 0 {
            return vec![];
        }
        let mut base = format!(
            "⎇ Subagents · {} running",
            self.counts.live.saturating_sub(self.counts.queued)
        );
        if self.counts.queued > 0 {
            base.push_str(&format!(" · {} queued", self.counts.queued));
        }
        let paused = if matches!(self.spawning, Spawning::Paused(_)) {
            " · spawning paused"
        } else {
            ""
        };
        let hint = self
            .open_hint
            .map(|s| format!(" · {} to open", literal(s)))
            .unwrap_or_default();
        let full = format!("{base}{paused}{hint}");
        let header = if full.width() <= usize::from(width) {
            full
        } else if format!("{base}{paused}").width() <= usize::from(width) {
            format!("{base}{paused}")
        } else {
            base
        };
        let mut lines = vec![Line::styled(
            cut_to_width(&header, width),
            self.style.header,
        )];
        if self.terminal_rows < 16 {
            return lines;
        }
        let live: Vec<_> = self
            .tree
            .rows_in_order()
            .into_iter()
            .filter(|r| self.tree.row(r.index).state.is_live())
            .collect();
        let goals: Vec<String> = live
            .iter()
            .map(|entry| literal(&self.tree.row(entry.index).goal))
            .collect();
        let goals: Vec<&str> = goals.iter().map(String::as_str).collect();
        let found = goal_labels(&goals);
        if let Some(shared) = &found.shared {
            lines.push(Line::styled(
                cut_to_width(&format!("  Task: {shared}"), width),
                self.style.hint,
            ));
        }
        for (entry, label) in live
            .iter()
            .zip(&found.labels)
            .take(usize::from(self.max_rows))
        {
            let row = self.tree.row(entry.index);
            let prefix = format!(
                "{}{} {label} · ",
                " ".repeat(2 + 2 * usize::from(entry.depth.min(3))),
                row.state.glyph(),
            );
            let activity_width = usize::from(width)
                .saturating_sub(prefix.width())
                .max(10)
                .min(usize::from(u16::MAX)) as u16;
            let text = format!("{prefix}{}", cut_to_width(&activity(row), activity_width));
            lines.push(styled_row(
                text,
                row.state.glyph(),
                row.state,
                width,
                self.style,
                false,
            ));
        }
        if live.len() > usize::from(self.max_rows) {
            lines.push(Line::styled(
                cut_to_width(
                    &format!("  … +{} more", live.len() - usize::from(self.max_rows)),
                    width,
                ),
                self.style.hint,
            ));
        }
        lines
    }
}
impl LiveSection for SubagentDock<'_> {
    fn height(&self, width: u16) -> u16 {
        u16::try_from(self.lines(width).len()).unwrap_or(u16::MAX)
    }
    fn render(&self, area: Rect, buf: &mut Buffer) {
        for (y, line) in (0..area.height).zip(self.lines(area.width)) {
            buf.set_line(area.x, area.y + y, &line, area.width);
        }
    }
}
