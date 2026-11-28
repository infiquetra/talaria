//! The live subagent dock and shared row styles.
use super::{
    AgentClock,
    labels::{GoalLabels, task_labels},
};
use crate::{checklist::cut_to_width, history::sanitize, live::LiveSection};
use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::Style,
    text::{Line, Span},
};
use talaria_core::subagents::{
    Activity, ActivityKind, RowState, Spawning, SubagentCounts, SubagentRow, SubagentTree, TreeRow,
};
use unicode_width::UnicodeWidthStr;

#[derive(Clone, Debug)]
pub struct SubagentStyle {
    pub header: Style,
    pub hint: Style,
    pub model: Style,
    pub task: Style,
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
/// Protect the effort label when a long model name has to be shortened.
pub(super) fn route_label(row: &SubagentRow, width: u16) -> String {
    let model = literal(row.model.as_deref().unwrap_or("model ?"));
    let effort = literal(row.effort.as_deref().unwrap_or("?"));
    let suffix = format!(" · effort {effort}");
    let model_cols = usize::from(width).saturating_sub(suffix.width());
    let model = cut_to_width(&model, model_cols as u16);
    cut_to_width(&format!("{model}{suffix}"), width)
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
    clock: AgentClock,
    rows: std::cell::OnceCell<(Vec<TreeRow>, GoalLabels)>,
}
impl<'a> SubagentDock<'a> {
    /// Use resolved styles for the existing dock projection.
    pub fn render_with_style(&self, area: Rect, buf: &mut Buffer, style: &SubagentStyle) {
        SubagentDock {
            style,
            rows: Default::default(),
            ..*self
        }
        .render(area, buf);
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
            clock: AgentClock::default(),
            rows: Default::default(),
        }
    }
    pub fn with_clock(mut self, clock: AgentClock) -> Self {
        self.clock = clock;
        self
    }
    fn rows(&self) -> &(Vec<TreeRow>, GoalLabels) {
        self.rows.get_or_init(|| {
            let visible: Vec<_> = self
                .tree
                .rows_in_order()
                .into_iter()
                .filter(|entry| self.tree.row(entry.index).state.is_live())
                .take(usize::from(self.max_rows))
                .collect();
            let goals: Vec<_> = visible
                .iter()
                .map(|entry| self.tree.row(entry.index).goal.as_str())
                .collect();
            let labels = task_labels(&goals);
            (visible, labels)
        })
    }
    /// Only rows which fit in the final viewport can request a frame.
    pub fn wake_after_ms(&self, width: u16, height: u16) -> Option<u64> {
        if self.terminal_rows < 16 || height < 2 || self.counts.live == 0 {
            return None;
        }
        let (rows, labels) = self.rows();
        let visible = height.saturating_sub(1 + u16::from(labels.shared.is_some()));
        let visible = if width < 60 {
            visible.div_ceil(2)
        } else {
            visible
        };
        rows.iter()
            .take(usize::from(visible))
            .filter_map(|r| self.clock.signal(self.tree.row(r.index)).wake_ms)
            .min()
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
        let (live, found) = self.rows();
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
            let signal = self.clock.signal(row);
            let prefix = format!(
                "{}{} {}  ",
                " ".repeat(2 + 2 * usize::from(entry.depth.min(3))),
                signal.glyph,
                signal.label
            );
            let room = width.saturating_sub(prefix.width() as u16);
            let mut spans = vec![Span::styled(
                cut_to_width(&prefix, width),
                self.style.state(row.state),
            )];
            let stacked = width < 60;
            let route = route_label(
                row,
                if stacked {
                    width.saturating_sub(4)
                } else {
                    room.saturating_sub(15)
                },
            );
            let job_room = if stacked {
                room
            } else {
                room.saturating_sub(3 + route.width() as u16)
            };
            let task = if label.is_empty() {
                "Untitled task"
            } else {
                label
            };
            let activity_room = if !stacked && !row.activity.is_empty() && job_room >= 34 {
                16
            } else {
                0
            };
            let task = cut_to_width(task, (job_room - activity_room).min(56));
            let mut used = prefix.width() + task.width();
            spans.push(Span::styled(task, self.style.task));
            if !stacked {
                used += 3 + route.width();
                spans.push(Span::styled(" · ", self.style.hint));
                spans.push(Span::styled(route.clone(), self.style.model));
            }
            let remaining = usize::from(width).saturating_sub(used + 3);
            if remaining >= 4 && !row.activity.is_empty() {
                spans.push(Span::styled(" · ", self.style.hint));
                spans.push(Span::styled(
                    cut_to_width(&activity(row), remaining as u16),
                    self.style.hint,
                ));
            }
            lines.push(Line::from(spans));
            if stacked {
                lines.push(Line::styled(
                    cut_to_width(&format!("    {route}"), width),
                    self.style.model,
                ));
            }
        }
        if self.counts.live > u32::from(self.max_rows) {
            lines.push(Line::styled(
                cut_to_width(
                    &format!("  … +{} more", self.counts.live - u32::from(self.max_rows)),
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
        if width == 0 || self.max_rows == 0 || self.counts.live == 0 {
            return 0;
        }
        if self.terminal_rows < 16 {
            return 1;
        }
        let (rows, labels) = self.rows();
        (rows.len() as u16)
            .saturating_mul(if width < 60 { 2 } else { 1 })
            .saturating_add(
                1 + u16::from(labels.shared.is_some())
                    + u16::from(self.counts.live > u32::from(self.max_rows)),
            )
    }
    fn render(&self, area: Rect, buf: &mut Buffer) {
        for (y, line) in (0..area.height).zip(self.lines(area.width)) {
            buf.set_line(area.x, area.y + y, &line, area.width);
        }
    }
}
