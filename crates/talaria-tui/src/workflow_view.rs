//! Windowed alternate-screen progress presentation. Untrusted content stays literal.
mod map;
mod overview;

use crate::{activity::FRAME_MS, checklist::cut_to_width, history::sanitize, theme::Motion};
use ratatui::{
    Frame,
    buffer::Buffer,
    layout::{Position, Rect},
    style::Style,
    text::{Line, Span},
    widgets::{Block, Paragraph, Widget},
};
use talaria_core::background::format_elapsed;
use talaria_core::subagents::RowState;
pub use talaria_workflow::progress::live::ProgressKey;
use talaria_workflow::progress::{
    AgentNode, AgentState, RunState, format_money, format_tokens,
    live::{NodeKey, ProgressState, VisibleNode, WaitView},
};
use talaria_workflow::tokens::tokens_text;
/// The failed agents a finished run lists above its rows before the rest are counted.
const FAILED_LISTED: usize = 5;
use unicode_width::UnicodeWidthStr;

#[derive(Clone, Debug)]
pub struct WorkflowTheme {
    pub normal: Style,
    pub selected: Style,
    pub muted: Style,
    pub warning: Style,
}
impl Default for WorkflowTheme {
    fn default() -> Self {
        crate::theme::legacy::workflow_theme()
    }
}
/// One line above the composer while a workflow run of the foreground session is active.
pub struct WorkflowDock<'a> {
    name: &'a str,
    status: &'a str,
    finished: u32,
    total: u32,
    open_hint: Option<&'a str>,
    header: Style,
    hint: Style,
    clock: Option<RunClock>,
}

/// What shows that a running workflow is alive: a spinner frame, the time since the run
/// started, or both. A run that is not running has none, and its line holds still.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RunClock {
    /// The spinner frame, under full motion. Under reduced motion the glyph stays still.
    pub frame: Option<usize>,
    /// Whole milliseconds since the run started, when its start time is known.
    pub elapsed_ms: Option<u64>,
}

impl RunClock {
    /// The clock of a run at `wall_now_ms` (Unix milliseconds) whose records say it started at
    /// `started_at_ms`. `tick_ms` is any millisecond count that rises with time and picks the
    /// spinner frame. A run that is not running has no clock. A start time that is missing (not
    /// above zero) or in the future gives no elapsed time rather than a wrong one.
    #[must_use]
    pub fn of_run(
        running: bool,
        started_at_ms: i64,
        wall_now_ms: u64,
        tick_ms: u64,
        motion: Motion,
    ) -> Option<Self> {
        if !running {
            return None;
        }
        let elapsed_ms = u64::try_from(started_at_ms)
            .ok()
            .filter(|started| *started > 0)
            .and_then(|started| wall_now_ms.checked_sub(started));
        let frame = (motion == Motion::Full).then(|| {
            usize::try_from(tick_ms / FRAME_MS).unwrap_or(0) % crate::launch_dialog::SPINNER.len()
        });
        Some(Self { frame, elapsed_ms })
    }

    /// How long until the line next changes: a spinner frame under full motion, the next whole
    /// second of the run under reduced motion, and never when nothing on the line moves.
    #[must_use]
    pub fn wake_after_ms(&self) -> Option<u64> {
        if self.frame.is_some() {
            return Some(FRAME_MS);
        }
        self.elapsed_ms.map(|elapsed| 1_000 - elapsed % 1_000)
    }
}

impl<'a> WorkflowDock<'a> {
    pub fn new(
        name: &'a str,
        status: &'a str,
        finished: u32,
        total: u32,
        open_hint: Option<&'a str>,
        header: Style,
        hint: Style,
    ) -> Self {
        Self {
            name,
            status,
            finished,
            total,
            open_hint,
            header,
            hint,
            clock: None,
        }
    }
    /// Show the spinner and the elapsed time of a running run.
    #[must_use]
    pub fn with_clock(mut self, clock: Option<RunClock>) -> Self {
        self.clock = clock;
        self
    }
    pub fn lines(&self, width: u16) -> Vec<Line<'static>> {
        if width == 0 {
            return vec![];
        }
        let clock = self.clock.unwrap_or_default();
        let glyph = clock.frame.map_or("⚙", |frame| {
            crate::launch_dialog::SPINNER[frame % crate::launch_dialog::SPINNER.len()]
        });
        let line = |with_time: bool| {
            let time = clock
                .elapsed_ms
                .filter(|_| with_time)
                .map(|ms| format!(" {}", format_elapsed(ms / 1_000)))
                .unwrap_or_default();
            sanitize(&format!(
                "{glyph} Workflow {} · {}{time} · {}/{} agents",
                self.name.replace(['\r', '\n', '\t'], " "),
                self.status.replace('_', " "),
                self.finished,
                self.total
            ))
            .into_owned()
        };
        let hint = format!(
            " · {} to open",
            self.open_hint
                .map_or_else(|| "/workflows".to_owned(), |key| sanitize(key).into_owned())
        );
        let room = usize::from(width);
        let with_time = line(true);
        if with_time.width() + hint.width() <= room {
            return vec![Line::from(vec![
                Span::styled(with_time, self.header),
                Span::styled(hint, self.hint),
            ])];
        }
        // Short of room, the hint goes first, then the time, and the agent count stays.
        if with_time.width() <= room {
            return vec![Line::styled(with_time, self.header)];
        }
        vec![Line::styled(cut_to_width(&line(false), width), self.header)]
    }
}
impl crate::live::LiveSection for WorkflowDock<'_> {
    fn height(&self, width: u16) -> u16 {
        u16::try_from(self.lines(width).len()).unwrap_or(u16::MAX)
    }
    fn render(&self, area: Rect, buf: &mut ratatui::buffer::Buffer) {
        for (y, line) in (0..area.height).zip(self.lines(area.width)) {
            buf.set_line(area.x, area.y + y, &line, area.width);
        }
    }
}

/// One line above the composer while the open session is a workflow agent's: which agent, how
/// the run's journal records it, and the key that opens the run.
pub struct WorkflowAgentDock<'a> {
    text: &'a str,
    open_hint: Option<&'a str>,
    back_key: Option<&'a str>,
    header: Style,
    hint: Style,
}
impl<'a> WorkflowAgentDock<'a> {
    pub fn new(text: &'a str, open_hint: Option<&'a str>, header: Style, hint: Style) -> Self {
        Self {
            text,
            open_hint,
            back_key: None,
            header,
            hint,
        }
    }
    /// Offer the key that goes back to the session the person came from. `key` is the label
    /// from the key map; the Left arrow is drawn as an arrow.
    #[must_use]
    pub fn with_back_key(mut self, key: Option<&'a str>) -> Self {
        self.back_key = key;
        self
    }
    pub fn lines(&self, width: u16) -> Vec<Line<'static>> {
        if width == 0 {
            return vec![];
        }
        let base =
            sanitize(&format!("⚙ {}", self.text.replace(['\r', '\n', '\t'], " "))).into_owned();
        let open = format!(
            " · {} opens the run",
            self.open_hint
                .map_or_else(|| "/workflows".to_owned(), |key| sanitize(key).into_owned())
        );
        let back = self.back_key.map(|key| {
            let key = if key == "Left" {
                "←".to_owned()
            } else {
                sanitize(key).into_owned()
            };
            format!(" · {key} back to the run")
        });
        let room = usize::from(width);
        // The key that goes back is dropped last of the two hints: it is the one a person who
        // just pressed Enter on an agent needs.
        let hints = match &back {
            Some(back) => [format!("{open}{back}"), back.clone(), open],
            None => [open.clone(), open, String::new()],
        };
        for hint in hints {
            if !hint.is_empty() && base.width() + hint.width() <= room {
                return vec![Line::from(vec![
                    Span::styled(base, self.header),
                    Span::styled(hint, self.hint),
                ])];
            }
        }
        vec![Line::styled(cut_to_width(&base, width), self.header)]
    }
}
impl crate::live::LiveSection for WorkflowAgentDock<'_> {
    fn height(&self, width: u16) -> u16 {
        u16::try_from(self.lines(width).len()).unwrap_or(u16::MAX)
    }
    fn render(&self, area: Rect, buf: &mut ratatui::buffer::Buffer) {
        for (y, line) in (0..area.height).zip(self.lines(area.width)) {
            buf.set_line(area.x, area.y + y, &line, area.width);
        }
    }
}

/// The block in the workflow dock's place while a routed turn builds a workflow: what it is,
/// how long it has run, and the step it is on. It holds still: only the seconds change.
pub struct AuthoringDock<'a> {
    view: &'a talaria_core::hivecode::authoring::AuthoringView,
    max_rows: u16,
    header: Style,
    hint: Style,
}
impl<'a> AuthoringDock<'a> {
    /// `max_rows` is 1 on a short screen, which draws the block as one row.
    pub fn new(
        view: &'a talaria_core::hivecode::authoring::AuthoringView,
        max_rows: u16,
        header: Style,
        hint: Style,
    ) -> Self {
        Self {
            view,
            max_rows,
            header,
            hint,
        }
    }
    pub fn lines(&self, width: u16) -> Vec<Line<'static>> {
        use talaria_core::hivecode::{authoring::elapsed_text, notice};
        if width == 0 || self.max_rows == 0 {
            return vec![];
        }
        let progress = &self.view.progress;
        let row = |text: String| sanitize(&text).into_owned();
        if self.max_rows == 1 {
            let short = row(format!(
                "⚙ {}",
                notice::authoring_short(progress, self.view.elapsed_ms)
            ));
            return vec![Line::styled(cut_to_width(&short, width), self.header)];
        }
        let title = row(format!(
            "⚙ {} · {}",
            notice::AUTHORING_TITLE,
            elapsed_text(self.view.elapsed_ms)
        ));
        let step = row(format!("  {}", notice::authoring_step(progress)));
        vec![
            Line::styled(cut_to_width(&title, width), self.header),
            Line::styled(cut_to_width(&step, width), self.hint),
        ]
    }
}
impl crate::live::LiveSection for AuthoringDock<'_> {
    fn height(&self, width: u16) -> u16 {
        u16::try_from(self.lines(width).len()).unwrap_or(u16::MAX)
    }
    fn render(&self, area: Rect, buf: &mut ratatui::buffer::Buffer) {
        for (y, line) in (0..area.height).zip(self.lines(area.width)) {
            buf.set_line(area.x, area.y + y, &line, area.width);
        }
    }
}

pub struct WorkflowView;
impl WorkflowView {
    pub fn render_picker(
        area: Rect,
        buf: &mut ratatui::buffer::Buffer,
        picker: &talaria_workflow::runner::ProgressPicker,
        theme: &WorkflowTheme,
    ) {
        Self::render_picker_inner(area, buf, picker, theme, None);
    }
    pub fn render_picker_themed(
        area: Rect,
        buf: &mut ratatui::buffer::Buffer,
        picker: &talaria_workflow::runner::ProgressPicker,
        theme: &WorkflowTheme,
        styles: &crate::theme::surfaces::ViewStyles,
    ) {
        Self::render_picker_inner(area, buf, picker, theme, Some(styles));
    }
    fn render_picker_inner(
        area: Rect,
        buf: &mut ratatui::buffer::Buffer,
        picker: &talaria_workflow::runner::ProgressPicker,
        theme: &WorkflowTheme,
        styles: Option<&crate::theme::surfaces::ViewStyles>,
    ) {
        let block = workflow_block("Workflows", theme, styles);
        let inner = block.inner(area);
        block.render(area, buf);
        if let Some(message) = picker.empty_message() {
            let lines = [
                message,
                "Turn Hivecode on with /hivecode on.",
                "With Hivecode on, a plain prompt goes to the talaria-workflows skill,",
                "and the agent proposes a workflow and asks before starting it.",
            ];
            for (y, text) in (inner.y..inner.bottom()).zip(lines) {
                Paragraph::new(text)
                    .style(theme.muted)
                    .render(Rect::new(inner.x, y, inner.width, 1), buf);
            }
            if inner.height > 0 {
                Paragraph::new("esc close")
                    .style(theme.muted)
                    .render(Rect::new(inner.x, inner.bottom() - 1, inner.width, 1), buf);
            }
            return;
        }
        let room = inner.height.saturating_sub(1) as usize;
        let start = picker
            .selected_index()
            .saturating_sub(room.saturating_sub(1));
        for (i, row) in picker.rows().iter().enumerate().skip(start).take(room) {
            Paragraph::new(sanitize(&picker_row(row)).into_owned())
                .style(if i == picker.selected_index() {
                    theme.selected
                } else {
                    theme.normal
                })
                .render(
                    Rect::new(inner.x, inner.y + (i - start) as u16, inner.width, 1),
                    buf,
                );
        }
        if inner.height > 0 {
            Paragraph::new("↑↓ select · enter open · esc close")
                .style(theme.muted)
                .render(Rect::new(inner.x, inner.bottom() - 1, inner.width, 1), buf);
        }
    }
    pub fn draw(frame: &mut Frame, area: Rect, state: &ProgressState, theme: &WorkflowTheme) {
        Self::render(area, frame.buffer_mut(), state, theme);
    }
    pub fn render(
        area: Rect,
        buf: &mut ratatui::buffer::Buffer,
        state: &ProgressState,
        theme: &WorkflowTheme,
    ) {
        Self::render_inner(area, buf, state, theme, None);
    }
    pub fn render_themed(
        area: Rect,
        buf: &mut ratatui::buffer::Buffer,
        state: &ProgressState,
        theme: &WorkflowTheme,
        styles: &crate::theme::surfaces::ViewStyles,
    ) {
        Self::render_inner(area, buf, state, theme, Some(styles));
    }
    fn render_inner(
        area: Rect,
        buf: &mut ratatui::buffer::Buffer,
        state: &ProgressState,
        theme: &WorkflowTheme,
        styles: Option<&crate::theme::surfaces::ViewStyles>,
    ) {
        let details = state.detail_offset.filter(|_| state.error.is_none());
        let overview = if details.is_none() && state.error.is_none() {
            overview::Overview::new(Block::bordered().inner(area), state)
        } else {
            None
        };
        let title = if state.error.is_some() {
            " Workflow unavailable "
        } else if state.evidence.is_some() {
            " Workflow evidence "
        } else if details.is_some() {
            "Workflow agent details"
        } else if overview.is_some() {
            " Workflow run "
        } else {
            "Workflow structure"
        };
        let block = workflow_block(title, theme, styles);
        let inner = block.inner(area);
        block.render(area, buf);
        if inner.height == 0 || inner.width == 0 {
            return;
        }
        let put = |y: u16, text: String, style: Style, buf: &mut ratatui::buffer::Buffer| {
            Paragraph::new(Line::from(cut_to_width(&sanitize(&text), inner.width)))
                .style(style)
                .render(Rect::new(inner.x, y, inner.width, 1), buf);
        };
        if let Some(error) = &state.error {
            let room = usize::from(inner.height.saturating_sub(1));
            let message = crate::history::word_wrapped(error, inner.width);
            for (y, line) in (inner.y..).zip(message.into_iter().take(room)) {
                put(y, line, theme.warning, buf);
            }
            put(inner.bottom() - 1, "Esc close".into(), theme.muted, buf);
            return;
        }
        if let Some(view) = &state.evidence {
            put(inner.y, view.title.clone(), theme.selected, buf);
            let room = usize::from(inner.height.saturating_sub(2));
            let lines = evidence_lines(view, inner.width);
            let start = view.offset.min(lines.len().saturating_sub(room));
            for (y, line) in (inner.y.saturating_add(1)..).zip(lines.iter().skip(start).take(room))
            {
                put(y, line.clone(), theme.normal, buf);
            }
            if inner.height > 1 {
                put(
                    inner.bottom() - 1,
                    "↑↓ scroll  PgUp/PgDn page  Home/End  Esc back".into(),
                    theme.muted,
                    buf,
                );
            }
            return;
        }
        if let Some(offset) = details {
            let lines = detail_lines(state, inner.width);
            let room = usize::from(inner.height.saturating_sub(1));
            let start = offset.min(lines.len().saturating_sub(room));
            for (y, line) in (inner.y..).zip(lines.into_iter().skip(start).take(room)) {
                put(y, line, theme.normal, buf);
            }
            put(
                inner.bottom() - 1,
                "↑↓ scroll  d / Esc back".into(),
                theme.muted,
                buf,
            );
            return;
        }
        if let Some(overview) = overview {
            let nodes = state.visible_nodes();
            overview.draw(state, theme, styles, buf);
            map::Map::new(overview.map, state, &nodes).draw(state, theme, styles, buf);
            overview::draw_inspector(overview.inspector, state, theme, styles, buf);
            overview::draw_footer(overview.footer, state, theme, buf);
            return;
        }
        let header = run_header(state, inner.width);
        let nodes = state.visible_nodes();
        let nodes = &nodes[1..]; // The run is drawn as the header.
        let layout = RunLayout::new(inner, state, nodes, header.len());
        for (y, (text, style)) in (inner.y..layout.tree.y).zip(header) {
            let style = match style {
                HeaderRole::Normal | HeaderRole::Evidence(_) => theme.normal,
                HeaderRole::Muted => theme.muted,
                HeaderRole::Warning => theme.warning,
            };
            put(y, text, style, buf);
        }
        if nodes.is_empty() && !state.progress.agents.is_empty() && layout.tree.height > 0 {
            // Name the real cause and the key that brings the rows back.
            let text = if state.collapsed.contains(&NodeKey::Run) {
                "everything is collapsed · → expands".to_owned()
            } else {
                format!(
                    "no agents match the {} filter · f changes it",
                    state.filter.as_str()
                )
            };
            put(layout.tree.y, text, theme.muted, buf);
        }
        draw_tree(state, theme, styles, nodes, &layout, buf);
        for (y, text) in (layout.tree.bottom()..layout.footer_y).zip(layout.preview) {
            put(y, text, theme.muted, buf);
        }
        for (y, text) in (layout.footer_y..inner.bottom()).zip(layout.footer) {
            let style = if state.steer_input.is_some() && y + 1 == inner.bottom() {
                theme.selected
            } else {
                theme.muted
            };
            put(y, text, style, buf);
        }
    }

    /// Keep vertical navigation on the current map level and bound detail scrolling.
    pub fn route_key(state: &ProgressState, area: Rect, key: ProgressKey) -> ProgressKey {
        if let Some(view) = &state.evidence {
            let inner = Block::bordered().inner(area);
            let room = usize::from(inner.height.saturating_sub(2)).max(1);
            let max = evidence_lines(view, inner.width).len().saturating_sub(room);
            let current = view.offset.min(max);
            return match key {
                ProgressKey::Up => ProgressKey::ScrollEvidence(current.saturating_sub(1)),
                ProgressKey::Down => {
                    ProgressKey::ScrollEvidence(current.saturating_add(1).min(max))
                }
                ProgressKey::EvidencePageUp => {
                    ProgressKey::ScrollEvidence(current.saturating_sub(room))
                }
                ProgressKey::EvidencePageDown => {
                    ProgressKey::ScrollEvidence(current.saturating_add(room).min(max))
                }
                ProgressKey::EvidenceHome => ProgressKey::ScrollEvidence(0),
                ProgressKey::EvidenceEnd => ProgressKey::ScrollEvidence(max),
                _ => key,
            };
        }
        if !matches!(key, ProgressKey::Up | ProgressKey::Down) {
            return key;
        }
        let inner = Block::bordered().inner(area);
        let Some(offset) = state.detail_offset else {
            if state.error.is_none()
                && state.confirmation_text().is_none()
                && state.steer_input.is_none()
                && let Some(overview) = overview::Overview::new(inner, state)
            {
                return map::Map::new(overview.map, state, &state.visible_nodes())
                    .vertical_key(&state.selection, key);
            }
            return key;
        };
        let lines = detail_lines(state, inner.width);
        let room = usize::from(inner.height.saturating_sub(1));
        let max = lines.len().saturating_sub(room);
        let offset = offset.min(max);
        ProgressKey::ScrollDetails(if key == ProgressKey::Up {
            offset.saturating_sub(1)
        } else {
            (offset + 1).min(max)
        })
    }

    /// Select the node under a terminal coordinate. Activation stays on Enter.
    pub fn click(state: &ProgressState, area: Rect, column: u16, row: u16) -> Option<ProgressKey> {
        if state.evidence.is_some() {
            let inner = Block::bordered().inner(area);
            return (inner.height > 1
                && row == inner.bottom() - 1
                && column >= inner.x
                && column < inner.right())
            .then_some(ProgressKey::CloseView);
        }
        if state.detail_offset.is_some()
            || state.error.is_some()
            || state.steer_input.is_some()
            || state.confirmation_text().is_some()
        {
            return None;
        }
        let inner = Block::bordered().inner(area);
        let nodes = state.visible_nodes();
        if let Some(overview) = overview::Overview::new(inner, state) {
            return overview.evidence_at(column, row).or_else(|| {
                map::Map::new(overview.map, state, &nodes).click(Position::new(column, row))
            });
        }
        let nodes = &nodes[1..];
        let header = run_header(state, inner.width);
        let layout = RunLayout::new(inner, state, nodes, header.len());
        if (inner.x..inner.right()).contains(&column)
            && (inner.y..layout.tree.y).contains(&row)
            && let Some((_, HeaderRole::Evidence(action))) = header.get(usize::from(row - inner.y))
        {
            return Some(action.clone());
        }
        if !(layout.tree.x..layout.tree.right()).contains(&column)
            || !(layout.tree.y..layout.tree.bottom()).contains(&row)
        {
            return None;
        }
        let node = nodes.get(layout.first + usize::from(row - layout.tree.y))?;
        Some(ProgressKey::SelectNode(node.key.clone()))
    }
}
pub use talaria_workflow::progress::text::run_line;

fn clean(text: &str, width: u16) -> String {
    cut_to_width(&sanitize(&text.replace(['\r', '\n', '\t'], " ")), width)
}

fn put(area: Rect, row: u16, text: &str, style: Style, buf: &mut Buffer) {
    if row < area.height && area.width > 0 {
        buf.set_stringn(
            area.x,
            area.y + row,
            clean(text, area.width),
            area.width as usize,
            style,
        );
    }
}

enum HeaderRole {
    Normal,
    Muted,
    Warning,
    Evidence(ProgressKey),
}

fn result_notices(state: &ProgressState, limit: usize) -> Vec<(String, HeaderRole)> {
    let mut notices = Vec::new();
    let run = &state.progress.run;
    if run.state == RunState::Running {
        return notices;
    }
    let verdict = run.script_status.as_deref().map_or_else(
        || "Workflow result".into(),
        |status| format!("Workflow verdict · {status}"),
    );
    notices.push((
        format!("{verdict} · v view"),
        HeaderRole::Evidence(ProgressKey::ViewResult),
    ));
    if run.state == RunState::CompletedWithFailures {
        let failures = state
            .progress
            .agents()
            .iter()
            .filter(|a| a.last_outcome.as_deref() == Some("failed") && !a.stopped);
        for agent in failures.clone().take(limit) {
            if agent.recovered_report.is_some() {
                notices.push((
                    format!("✗ {} · reply failed · saved report available", agent.label),
                    HeaderRole::Evidence(ProgressKey::ViewReport(agent.key.clone())),
                ));
            } else {
                notices.push((
                    format!(
                        "✗ {} · {}",
                        agent.label,
                        agent.failure.as_deref().unwrap_or("no reason given")
                    ),
                    HeaderRole::Warning,
                ));
            }
        }
        let remaining = failures.count().saturating_sub(limit);
        if remaining > 0 {
            notices.push((
                format!("… and {remaining} more failed agents"),
                HeaderRole::Warning,
            ));
        }
    }
    let mut warnings = state
        .progress
        .agents()
        .iter()
        .filter(|a| a.guardrail_halt.is_some());
    if let Some(agent) = warnings.next() {
        let more = warnings.count();
        let suffix = if more == 0 {
            String::new()
        } else {
            format!(" · {more} more")
        };
        notices.push((
            format!(
                "! {} · {}{suffix}",
                agent.label,
                warning_title(agent).unwrap_or_default()
            ),
            HeaderRole::Warning,
        ));
    }
    notices
}

fn warning_title(agent: &AgentNode) -> Option<&str> {
    agent
        .guardrail_halt
        .as_deref()
        .map(|warning| warning.split_once(": ").map_or(warning, |(title, _)| title))
}

fn finished_counts(state: &ProgressState) -> String {
    let run = &state.progress.run;
    let c = &run.counts;
    let cut_short = state
        .progress
        .agents()
        .iter()
        .filter(|agent| agent.cut_short.is_some())
        .count();
    let mut buckets = vec![format!("{} succeeded", c.done.saturating_sub(cut_short))];
    for (count, label) in [
        (c.failed.saturating_sub(run.budget_stopped), "failed"),
        (cut_short, "cut short"),
        (run.budget_stopped, "stopped"),
        (c.skipped, "skipped"),
        (run.refused, "refused"),
    ] {
        if count > 0 {
            buckets.push(format!("{count} {label}"));
        }
    }
    buckets.join(" · ")
}

fn evidence_lines(
    view: &talaria_workflow::progress::live::EvidenceView,
    width: u16,
) -> std::sync::Arc<[String]> {
    view.rows(width, |lines| {
        lines
            .iter()
            .flat_map(|line| {
                let safe = sanitize(line);
                if safe.is_empty() {
                    vec![String::new()]
                } else {
                    crate::history::word_wrapped(&safe, width.max(1))
                }
            })
            .collect()
    })
}

fn run_header(state: &ProgressState, width: u16) -> Vec<(String, HeaderRole)> {
    use HeaderRole::{Muted, Normal, Warning};
    let full = run_line(state);
    let width = usize::from(width);
    let mut lines = if state.progress.run.state != RunState::Running {
        let run = &state.progress.run;
        let mut lines = vec![(format!("{} · {}", run.name, run.state.as_str()), Normal)];
        let counts = finished_counts(state);
        if let Some(elapsed) = overview::run_elapsed(state) {
            let duration = format!("Finished in {elapsed}");
            if duration.width() + counts.width() + 3 <= width {
                lines.push((format!("{duration} · {counts}"), Normal));
            } else {
                lines.extend([(duration, Normal), (counts, Normal)]);
            }
        } else {
            lines.push((counts, Normal));
        }
        if !run.tokens_unknown {
            lines.push((
                tokens_text(
                    run.tokens,
                    run.cache_read,
                    run.cache_unknown_turns,
                    format_tokens,
                ),
                Muted,
            ));
        }
        lines
    } else if sanitize(&full).width() <= width {
        vec![(full, Normal)]
    } else {
        let run = &state.progress.run;
        let counts = &run.counts;
        let mut summary = format!(
            "{} · {}/{} agents",
            run.state.as_str(),
            counts.done,
            counts.total
        );
        if counts.needs_you > 0 {
            summary.push_str(&attention_suffix(counts.needs_you));
        }
        let headline = format!("{} · {summary}", run.name);
        let mut lines = if sanitize(&headline).width() <= width {
            vec![(headline, Normal)]
        } else {
            vec![(run.name.clone(), Normal), (summary, Normal)]
        };
        if state.no_live_runner {
            lines.push(("Runner stopped · use workflow resume".into(), Warning));
        } else if let Some(status) = &state.run_suffix {
            lines.push((status.clone(), Warning));
        }
        if state.filter != talaria_workflow::progress::Filter::All {
            lines.push((format!("Filter: {}", state.filter.as_str()), Muted));
        }
        lines
    };
    if !state.connected {
        lines.push((
            "journal only — live state and controls unavailable".into(),
            Warning,
        ));
    }
    lines.extend(result_notices(
        state,
        if width < 60 { 1 } else { FAILED_LISTED },
    ));
    if state.progress.last_seq == 0 {
        lines.push(("run starting — nothing journaled yet".into(), Muted));
    }
    lines
}

fn selected_agent(state: &ProgressState) -> Option<&AgentNode> {
    state.progress.agent(state.selected_agent_key()?)
}

fn selected_subagent<'a>(
    state: &ProgressState,
    agent: &'a AgentNode,
) -> Option<&'a talaria_core::subagents::SubagentRow> {
    let NodeKey::Subagent { id, .. } = &state.selection else {
        return None;
    };
    agent.subagents.get(id)
}

fn agent_prompt<'a>(state: &'a ProgressState, agent: &'a AgentNode) -> &'a str {
    state
        .prompt_first_line
        .as_deref()
        .or_else(|| agent.prompt.as_deref().and_then(|s| s.lines().next()))
        .unwrap_or(if agent.prompt_ref.is_some() {
            "(unavailable — journal blob missing)"
        } else {
            "(unavailable)"
        })
}

fn agent_model(agent: &AgentNode) -> String {
    let model = agent.model.as_deref().unwrap_or("unknown");
    match agent.provider.as_deref() {
        Some(provider) => format!("Model {model} · {provider}"),
        None => format!("Model {model}"),
    }
}

fn agent_usage(agent: &AgentNode) -> String {
    if agent.tokens_unknown {
        "unknown tokens".into()
    } else {
        tokens_text(
            agent.tokens,
            agent.cache_read,
            agent.cache_unknown_turns,
            format_tokens,
        )
    }
}

fn agent_preview(state: &ProgressState, agent: &AgentNode) -> Vec<String> {
    if let Some(child) = selected_subagent(state, agent) {
        let mut lines = Vec::new();
        if let Some(activity) = child.activity.back() {
            lines.push(activity.text.clone());
        } else if let Some(tool) = &child.last_tool {
            lines.push(format!("Tool {tool}"));
        }
        let model = child.model.as_deref().unwrap_or("unknown");
        lines.push(format!("Subagent model {model}"));
        return lines;
    }
    let mut lines = Vec::new();
    if let Some(path) = &agent.recovered_report {
        lines.push(format!("Saved report · d open · {path}"));
    }
    if let Some(warning) = warning_title(agent) {
        lines.push(warning.to_owned());
    }
    if let Some(reason) = agent.attention.as_deref().or(agent.failure.as_deref()) {
        lines.push(reason.into());
    } else if let Some(wait) = state.waits.get(&agent.key) {
        lines.push(wait_text(wait).into());
    } else if let Some(tool) = agent
        .last_tool
        .as_deref()
        .filter(|_| !agent.state.terminal())
    {
        lines.push(format!("Tool {tool}"));
    }
    lines.extend([
        format!("Prompt {}", agent_prompt(state, agent)),
        agent_model(agent),
        format!("{} · {}", agent_usage(agent), format_money(agent.money)),
    ]);
    lines
}

fn detail_lines(state: &ProgressState, width: u16) -> Vec<String> {
    let Some(agent) = selected_agent(state) else {
        return Vec::new();
    };
    let mut lines = Vec::new();
    if let Some(warning) = &agent.guardrail_halt {
        lines.push(warning.clone());
    }
    if let Some(child) = selected_subagent(state, agent) {
        lines.push(format!("Subagent {}", cut_to_width(&child.goal, 1024)));
        lines.push(format!(
            "Model {}",
            child.model.as_deref().unwrap_or("unknown")
        ));
        lines.push(format!(
            "Subagent session {}",
            child.child_session_id.as_deref().unwrap_or("none")
        ));
        lines.extend(
            child
                .activity
                .iter()
                .rev()
                .take(4)
                .rev()
                .map(|activity| cut_to_width(&activity.text, 1024)),
        );
    }
    lines.extend([
        format!(
            "{} · {} · {:.1}s",
            cut_to_width(&agent.label, 1024),
            agent.state.as_str(),
            agent.duration_ms(state.now_ms) as f64 / 1000.0
        ),
        agent_model(agent),
        format!(
            "Displayed {} · {}",
            agent_usage(agent),
            format_money(agent.money)
        ),
        format!(
            "Settled {}",
            tokens_text(
                agent.settled.counted,
                agent.settled.cache_read,
                agent.cache_unknown_turns,
                format_tokens
            )
        ),
        format!(
            "Runtime session {}",
            agent.session_id.as_deref().unwrap_or("none")
        ),
        format!(
            "Stored session {}",
            agent.stored_session_id.as_deref().unwrap_or("none")
        ),
        format!("Attempt {} · turn {}", agent.attempt, agent.turn),
        format!(
            "Admission {} ({})",
            agent.admission_id.as_deref().unwrap_or("none"),
            agent.admission_status.as_deref().unwrap_or("none")
        ),
    ]);
    if let Some(note) = agent.progress_note() {
        lines.push(note);
    }
    if let Some(reason) = agent.attention.as_deref().or(agent.failure.as_deref()) {
        lines.push(cut_to_width(reason, 1024));
    }
    if let Some(wait) = state.waits.get(&agent.key) {
        lines.push(wait_text(wait).into());
    }
    if agent.state == AgentState::Ended {
        lines.push("Ended without a journal outcome".into());
    }
    if agent.overflow_restarts > 0 {
        lines.push(format!(
            "Restarted {} of {} after a context overflow",
            agent.overflow_restarts,
            talaria_workflow::journal::MAX_OVERFLOW_RESTARTS
        ));
    }
    if agent.restart_requested {
        lines.push("Restart requested · runs again on resume".into());
    } else if agent.stopped || agent.state == AgentState::Failed {
        lines.push("Restart (r) runs this agent again".into());
    }
    lines.push(format!(
        "Prompt {}",
        cut_to_width(agent_prompt(state, agent), 1024)
    ));
    if let Some(tool) = &agent.last_tool {
        lines.push(format!("Last tool {}", cut_to_width(tool, 1024)));
    }
    lines.extend(agent.tail.iter().cloned());
    lines
        .into_iter()
        .flat_map(|line| crate::history::word_wrapped(&line, width))
        .collect()
}

struct RunLayout {
    tree: Rect,
    first: usize,
    preview: Vec<String>,
    footer_y: u16,
    footer: Vec<String>,
}

impl RunLayout {
    fn new(inner: Rect, state: &ProgressState, nodes: &[VisibleNode], header_lines: usize) -> Self {
        let selection = nodes.iter().position(|node| node.key == state.selection);
        let footer = outline_footer(inner, state, selection, nodes.len());
        let footer_height = u16::try_from(footer.len()).unwrap_or_default();
        let body = inner.height - footer_height;
        let reserved_header = if nodes.is_empty() && state.progress.agents.is_empty() {
            body
        } else {
            body.saturating_sub(1).max(1).min(body)
        };
        let header = u16::try_from(header_lines)
            .unwrap_or(u16::MAX)
            .min(reserved_header);
        let available = body - header;
        let preview: Vec<_> = selected_agent(state)
            .into_iter()
            .flat_map(|agent| agent_preview(state, agent))
            .flat_map(|text| crate::history::word_wrapped(&cut_to_width(&text, 1024), inner.width))
            .take(usize::from((available / 2).min(7)))
            .collect();
        let room = usize::from(available).saturating_sub(preview.len());
        let first = selection
            .unwrap_or(0)
            .saturating_sub(room / 2)
            .min(nodes.len().saturating_sub(room));
        let height = u16::try_from(nodes.len().max(1).min(room)).unwrap_or(u16::MAX);
        Self {
            tree: Rect::new(inner.x, inner.y + header, inner.width, height),
            first,
            preview,
            footer_y: inner.bottom() - footer_height,
            footer,
        }
    }
}

fn outline_footer(
    area: Rect,
    state: &ProgressState,
    selection: Option<usize>,
    count: usize,
) -> Vec<String> {
    let status = state.confirmation_text().or(state.footer.as_deref());
    let limit = usize::from(area.height.saturating_sub(2));
    if limit == 0 {
        return Vec::new();
    }
    if let Some(input) = &state.steer_input {
        let entry = format!("steer> {input}");
        return if limit == 1 {
            vec![entry]
        } else {
            vec![status.unwrap_or("Enter send  Esc cancel").into(), entry]
        };
    }
    let navigation = match area.width {
        76.. => "↑↓ move · ← fold / parent · → expand / child · Enter transcript · Esc close",
        36.. => "↑↓ move  ← fold  → expand  Esc close",
        _ => "↑↓ move  Esc close",
    };
    let mut footer = vec![status.unwrap_or(navigation).to_owned()];
    if area.height < 9 {
        append_hints(
            &mut footer,
            &[
                "v evidence",
                "d info/report",
                "Enter open",
                "a attention",
                "f filter",
            ],
            "  ",
            area.width,
        );
        footer.truncate(limit.min(2));
        return footer;
    }
    let (actions, separator): (&[&str], &str) = if area.width < 76 {
        (
            &[
                "v evidence",
                "d info/report",
                "Enter open",
                "a attention",
                "f filter",
            ],
            "  ",
        )
    } else {
        (
            &[
                "v evidence",
                "d details/report",
                "a next attention",
                "f filter",
            ],
            " · ",
        )
    };
    append_hints(&mut footer, actions, separator, area.width);
    append_hints(
        &mut footer,
        &[
            "s steer",
            "x stop",
            "r restart",
            "p pause",
            "Shift+x stop run",
        ],
        separator,
        area.width,
    );
    footer.truncate(limit);
    if let Some(index) = selection
        && let Some(last) = footer.last_mut()
    {
        let position = format!(" · {}/{}", index + 1, count);
        if last.width() + position.width() <= usize::from(area.width) {
            last.push_str(&position);
        }
    }
    footer
}

fn append_hints(rows: &mut Vec<String>, hints: &[&str], separator: &str, width: u16) {
    let mut line = String::new();
    for hint in hints {
        if !line.is_empty() && line.width() + separator.width() + hint.width() > usize::from(width)
        {
            rows.push(std::mem::take(&mut line));
        }
        if !line.is_empty() {
            line.push_str(separator);
        }
        line.push_str(hint);
    }
    if !line.is_empty() {
        rows.push(line);
    }
}

fn draw_tree(
    state: &ProgressState,
    theme: &WorkflowTheme,
    styles: Option<&crate::theme::surfaces::ViewStyles>,
    nodes: &[VisibleNode],
    layout: &RunLayout,
    buf: &mut ratatui::buffer::Buffer,
) {
    let mut next_branch = vec![false; nodes.len()];
    let mut upcoming = Vec::new();
    for (i, node) in nodes.iter().enumerate().rev() {
        while upcoming.last().is_some_and(|depth| *depth > node.depth) {
            upcoming.pop();
        }
        next_branch[i] = upcoming.last() == Some(&node.depth);
        if !next_branch[i] {
            upcoming.push(node.depth);
        }
    }

    let tree = layout.tree;
    let border = |symbol| styles.map_or(symbol, |styles| styles.border_symbol(symbol));
    let mut ancestor_branches = Vec::new();
    let width = tree.width;
    let max_indent = usize::from(width.saturating_sub(16) / 2);
    for (i, node) in nodes
        .iter()
        .enumerate()
        .take(layout.first + usize::from(tree.height))
    {
        let ancestor_count = usize::from(node.depth.saturating_sub(1));
        ancestor_branches.resize(ancestor_count, false);
        if i >= layout.first {
            let selected = node.key == state.selection;
            let mut prefix = String::from(if selected { "> " } else { "  " });
            if width >= 16 {
                for &continues in ancestor_branches
                    .iter()
                    .skip(ancestor_count.saturating_sub(max_indent))
                {
                    prefix.push_str(if continues { border("│") } else { " " });
                    prefix.push(' ');
                }
                prefix.push_str(border(if next_branch[i] { "├" } else { "└" }));
                prefix.push_str(border("─"));
                prefix.push(' ');
            }
            if state.collapsed.contains(&node.key) {
                prefix.push_str("▸ ");
            }
            let text = node_line(state, &node.key);
            let node_text = sanitize(&text);
            let row = tree.y + (i - layout.first) as u16;
            let agent = match &node.key {
                NodeKey::Agent(key) => state.progress.agent(key),
                _ => None,
            };
            let style = if selected {
                theme.selected
            } else if agent.is_some_and(ProgressState::agent_needs_attention) {
                theme.warning
            } else {
                theme.normal
            };
            let room = width.saturating_sub(prefix.width() as u16);
            let mut spans = vec![Span::raw(prefix), Span::raw(cut_to_width(&node_text, room))];
            if let Some(agent) = agent.filter(|agent| agent.state != AgentState::Queued) {
                let seconds = agent.duration_ms(state.now_ms) / 1_000;
                let duration = format_elapsed(u64::try_from(seconds).unwrap_or_default());
                let taken = node_text.width() + duration.width();
                if taken + 2 <= usize::from(room) {
                    spans.push(Span::raw(" ".repeat(usize::from(room) - taken)));
                    spans.push(Span::raw(duration));
                }
            }
            Paragraph::new(Line::from(spans))
                .style(style)
                .render(Rect::new(tree.x, row, width, 1), buf);
        }
        ancestor_branches.push(next_branch[i]);
    }
}

pub fn picker_key_input(map: &crate::keys::KeyMap, key: crate::keys::Key) -> Option<ProgressKey> {
    use crate::keys::{Action, KeyContext, KeyKind};
    if key.kind == KeyKind::Release {
        return None;
    }
    match map.resolve(&key, KeyContext::WorkflowProgress)? {
        Action::ProgressUp => Some(ProgressKey::Up),
        Action::ProgressDown => Some(ProgressKey::Down),
        Action::OpenAgent => Some(ProgressKey::OpenAgent),
        Action::CloseView => Some(ProgressKey::CloseView),
        _ => None,
    }
}
fn node_line(s: &ProgressState, node: &NodeKey) -> String {
    match node {
        NodeKey::Run => run_line(s),
        NodeKey::Child(path) => s
            .progress
            .children
            .iter()
            .find(|c| &c.workflow_path == path)
            .map(|c| {
                let attention = s
                    .progress
                    .agents()
                    .iter()
                    .filter(|a| {
                        a.workflow_path.starts_with(path) && ProgressState::agent_needs_attention(a)
                    })
                    .count();
                format!("{}{}", c.name, attention_suffix(attention))
            })
            .unwrap_or_default(),
        NodeKey::Phase { path, title } => s
            .progress
            .phases
            .iter()
            .find(|p| &p.workflow_path == path && &p.title == title)
            .map(|p| {
                format!(
                    "{} · agents {}/{}{}",
                    title,
                    p.counts.done,
                    p.counts.total,
                    attention_suffix(p.counts.needs_you + p.counts.failed + p.counts.ended)
                )
            })
            .unwrap_or_default(),
        NodeKey::Agent(key) => s
            .progress
            .agent(key)
            .map(|a| {
                let (glyph, status) = agent_status(a);
                let mut row = format!("{glyph} {} · {status}", a.label);
                if a.state == AgentState::Running
                    && let Some(note) = a.progress_note()
                {
                    row.push_str(&format!(" · {note}"));
                }
                row
            })
            .unwrap_or_default(),
        NodeKey::Subagent { key, id } => s
            .progress
            .agent(key)
            .and_then(|a| a.subagents.get(id))
            .map(|r| {
                let (glyph, status) = subagent_status(&r.state, s.subagent_stopping(key, id));
                format!("{glyph} {} · {status}", r.goal)
            })
            .unwrap_or_default(),
    }
}

fn agent_status(agent: &AgentNode) -> (char, &'static str) {
    if agent.recovered_report.is_some() {
        ('✗', "reply failed · d saved report")
    } else if agent.guardrail_halt.is_some() && agent.state == AgentState::Done {
        ('!', "done · review warning")
    } else if agent.cut_short.is_some() {
        ('✓', "cut short")
    } else if agent.stopped {
        ('—', "stopped")
    } else {
        (agent.state.glyph(), agent.state.as_str())
    }
}

fn subagent_status(state: &RowState, stopping: bool) -> (char, &'static str) {
    if stopping {
        return ('◌', "stopping");
    }
    let status = match state {
        RowState::Queued => "queued",
        RowState::Running => "running",
        RowState::Completed => "done",
        RowState::Failed(_) => "failed",
        RowState::Interrupted => "interrupted",
        RowState::Gone => "gone",
    };
    (state.glyph(), status)
}
fn attention_suffix(count: usize) -> String {
    match count {
        0 => String::new(),
        1 => " · 1 needs attention".into(),
        _ => format!(" · {count} need attention"),
    }
}
fn wait_text(w: &WaitView) -> &'static str {
    match w {
        WaitView::GlobalCap => "waiting for global capacity",
        WaitView::ProviderCap => "waiting for provider capacity",
        WaitView::ProviderLimited { .. } => "provider limited",
        WaitView::ModelLimited { .. } => "model limited",
        WaitView::ProviderPaused { .. } => "provider paused",
    }
}

/// The overlay consumes every key before the composer or foreground turn.
pub fn key_input(
    map: &crate::keys::KeyMap,
    key: crate::keys::Key,
    state: &ProgressState,
) -> Option<talaria_workflow::progress::live::ProgressKey> {
    use crate::keys::{Action, KeyCode, KeyContext, KeyKind};
    use talaria_workflow::progress::live::ProgressKey as K;
    if key.kind == KeyKind::Release {
        return None;
    }
    if state.evidence.is_some() {
        return match key.code {
            KeyCode::Up | KeyCode::Char('k') => Some(K::Up),
            KeyCode::Down | KeyCode::Char('j') => Some(K::Down),
            KeyCode::PageUp => Some(K::EvidencePageUp),
            KeyCode::PageDown => Some(K::EvidencePageDown),
            KeyCode::Home => Some(K::EvidenceHome),
            KeyCode::End => Some(K::EvidenceEnd),
            KeyCode::Esc | KeyCode::Char('q') => Some(K::CloseView),
            _ => None,
        };
    }
    if state.steer_input.is_some() {
        return match key.code {
            KeyCode::Char(c) if !key.mods.ctrl && !key.mods.alt && !key.mods.super_key => {
                Some(K::Char(c))
            }
            KeyCode::Backspace => Some(K::Backspace),
            KeyCode::Enter => Some(K::Enter),
            KeyCode::Esc => Some(K::CloseView),
            _ => None,
        };
    }
    Some(
        match map
            .resolve(&key, KeyContext::WorkflowProgress)
            .or_else(|| state.confirmation_text().map(|_| Action::CloseView))?
        {
            Action::ProgressUp => K::Up,
            Action::ProgressDown => K::Down,
            Action::ProgressLeft => K::Left,
            Action::ProgressRight => K::Right,
            Action::OpenAgent => K::OpenAgent,
            Action::Steer => K::Steer,
            Action::StopAgent => K::StopAgent,
            Action::RestartAgent => K::RestartAgent,
            Action::PauseRun => K::PauseRun,
            Action::StopRun => K::StopRun,
            Action::CycleFilter => K::CycleFilter,
            Action::ProgressNextAttention => K::NextAttention,
            Action::ProgressDetails => state
                .selected_agent_key()
                .and_then(|key| {
                    state
                        .progress
                        .agent(key)
                        .filter(|agent| agent.recovered_report.is_some())
                        .map(|_| K::ViewReport(key.into()))
                })
                .unwrap_or(K::Details),
            Action::ProgressViewResult => K::ViewResult,
            Action::CloseView => K::CloseView,
            Action::ProgressConfirm => K::Confirm,
            _ => return None,
        },
    )
}

/// One run in the run list, before it is cleaned for the screen.
pub fn picker_row(row: &talaria_workflow::runner::RunSummary) -> String {
    format!(
        "{}  {}  {}  agents {}/{}  parked {}  {}  money {}{}",
        row.run_id,
        row.name,
        row.status.label(),
        row.agents_finished,
        row.agents_total_known,
        row.parked,
        row.tokens_spent.map_or_else(
            || "tokens -".into(),
            |n| tokens_text(
                n,
                row.cache_read_tokens.unwrap_or_default(),
                row.cache_unknown_turns,
                format_tokens
            )
        ),
        row.money_spent_usd
            .map_or_else(|| "-".into(), |n| format!("{n:.4}")),
        row.warning
            .as_ref()
            .map_or_else(String::new, |w| format!("  {w}")),
    )
}

fn workflow_block<'a>(
    title: &'static str,
    theme: &WorkflowTheme,
    styles: Option<&'a crate::theme::surfaces::ViewStyles>,
) -> Block<'a> {
    let mut block = Block::bordered().title(title).style(theme.normal);
    if let Some(styles) = styles {
        let symbol = |s| styles.border_symbol(s);
        block = block.border_set(ratatui::symbols::border::Set {
            top_left: symbol("┌"),
            top_right: symbol("┐"),
            bottom_left: symbol("└"),
            bottom_right: symbol("┘"),
            vertical_left: symbol("│"),
            vertical_right: symbol("│"),
            horizontal_top: symbol("─"),
            horizontal_bottom: symbol("─"),
        });
    }
    block
}
