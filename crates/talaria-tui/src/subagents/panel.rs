//! Full-screen subagent views and finished batch rendering.
use super::{
    SubagentStyle,
    dock::{activity, dim_suffix, literal, styled_row},
};
use crate::{
    checklist::cut_to_width,
    history::sanitize,
    keys::{Action, KeyMap},
};
use ratatui::{
    style::Style,
    text::{Line, Span},
};
use std::cell::Cell;
use talaria_app::subagents::{ControlOutcome, SubagentAction, outcome_text};
use talaria_core::{
    directory::rows::RowKey,
    subagents::{Clock, FailKind, FinishedBatch, RowState, Spawning, SubagentRow, SubagentTree},
};
use unicode_width::UnicodeWidthStr;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PanelMode {
    List,
    Detail,
    Tail,
    Steer,
    ConfirmPause,
}
pub struct PanelHeader {
    pub title: String,
    pub spawning: Spawning,
    pub key_label: Option<String>,
}
pub struct TailView {
    pub text: Option<String>,
    pub truncated: bool,
    pub ever_failed: bool,
}
pub struct AgentsPanel {
    key: RowKey,
    mode: PanelMode,
    previous: PanelMode,
    cursor: usize,
    shown_id: Option<String>,
    scroll: usize,
    follow: bool,
    draft: String,
    flash: Option<String>,
    pending: Option<SubagentAction>,
    close: bool,
    keys: KeyMap,
    style: SubagentStyle,
    spawning: Cell<Spawning>,
    running: Cell<Option<u32>>,
    viewport: Cell<usize>,
    rendered_scroll: Cell<usize>,
    now_ms: u64,
    armed_at: Option<u64>,
}
impl AgentsPanel {
    pub fn new(key: RowKey) -> Self {
        Self {
            key,
            mode: PanelMode::List,
            previous: PanelMode::List,
            cursor: 0,
            shown_id: None,
            scroll: 0,
            follow: true,
            draft: String::new(),
            flash: None,
            pending: None,
            close: false,
            keys: KeyMap::defaults(),
            style: SubagentStyle::default(),
            spawning: Cell::new(Spawning::Unknown),
            running: Cell::new(None),
            viewport: Cell::new(20),
            rendered_scroll: Cell::new(0),
            now_ms: 0,
            armed_at: None,
        }
    }
    pub fn key(&self) -> &RowKey {
        &self.key
    }
    pub fn set_key(&mut self, key: RowKey) {
        let keys = self.keys.clone();
        let style = self.style.clone();
        *self = Self::new(key);
        self.keys = keys;
        self.style = style;
    }
    pub fn mode(&self) -> PanelMode {
        self.mode
    }
    pub fn set_keymap(&mut self, keys: &KeyMap) {
        self.keys = keys.clone();
    }
    pub fn set_style(&mut self, style: SubagentStyle) {
        self.style = style;
    }
    pub fn set_spawning(&mut self, spawning: Spawning, running: Option<u32>) {
        self.spawning.set(spawning);
        self.running.set(running);
    }
    /// Show the gateway-wide pause question after a slash command.
    pub fn confirm_pause(&mut self) {
        if self.mode != PanelMode::ConfirmPause {
            self.previous = self.mode;
            self.mode = PanelMode::ConfirmPause;
        }
    }
    pub fn take_pending_action(&mut self) -> Option<SubagentAction> {
        self.pending.take()
    }
    pub fn take_close_request(&mut self) -> bool {
        std::mem::take(&mut self.close)
    }
    pub fn tick(&mut self, now_ms: u64) {
        self.now_ms = now_ms;
        if self
            .armed_at
            .is_some_and(|t| now_ms.saturating_sub(t) > 3000)
        {
            self.armed_at = None;
        }
    }
    pub fn selected_id(&self, tree: &SubagentTree) -> Option<String> {
        self.selected(tree).map(|r| r.id.clone())
    }
    fn selected<'a>(&self, tree: &'a SubagentTree) -> Option<&'a SubagentRow> {
        if self.mode != PanelMode::List {
            if let Some(id) = &self.shown_id {
                return tree.get(id);
            }
        }
        let rows = tree.rows_in_order();
        rows.get(self.cursor.min(rows.len().saturating_sub(1)))
            .map(|r| tree.row(r.index))
    }
    pub fn backspace(&mut self) {
        self.flash = None;
        if self.mode == PanelMode::Steer {
            self.draft.pop();
        }
    }
    pub fn type_char(&mut self, c: char) {
        self.flash = None;
        match self.mode {
            PanelMode::Steer if !c.is_control() && self.draft.chars().count() < 4000 => {
                self.draft.push(c)
            }
            PanelMode::ConfirmPause => match c.to_ascii_lowercase() {
                'y' => {
                    self.pending = Some(SubagentAction::ConfirmPause(true));
                    self.mode = self.previous;
                }
                'n' => self.mode = self.previous,
                _ => {}
            },
            _ => {}
        }
    }
    pub fn set_flash(&mut self, text: String) {
        self.flash = Some(literal(&text));
    }
    pub fn handle(&mut self, action: Action, tree: &SubagentTree) -> Option<SubagentAction> {
        self.flash = None;
        if self.mode == PanelMode::ConfirmPause {
            if action == Action::AgentsBack {
                self.mode = self.previous;
            }
            return None;
        }
        if self.mode == PanelMode::Steer {
            if action == Action::AgentsBack {
                self.mode = self.previous;
                self.draft.clear();
            } else if action == Action::AgentsTail {
                self.mode = self.previous;
                let text = self.draft.trim().to_owned();
                self.draft.clear();
                if !text.is_empty() {
                    return self
                        .shown_id
                        .clone()
                        .map(|id| SubagentAction::Steer { id, text });
                }
            }
            return None;
        }
        let total = tree.rows_in_order().len();
        let step = self.viewport.get().max(1);
        match action {
            Action::AgentsClose | Action::AgentsPanel => self.close = true,
            Action::AgentsBack => {
                if self.mode == PanelMode::List {
                    self.close = true;
                } else {
                    let tail = self.mode == PanelMode::Tail;
                    self.mode = PanelMode::List;
                    self.scroll = 0;
                    if tail {
                        self.pending = Some(SubagentAction::CloseTail);
                    }
                }
            }
            Action::AgentsTail | Action::AgentsDetail => {
                if let Some(id) = self.selected_id(tree) {
                    self.shown_id = Some(id);
                    self.mode = if action == Action::AgentsTail {
                        PanelMode::Tail
                    } else {
                        PanelMode::Detail
                    };
                    self.follow = true;
                    self.scroll = 0;
                }
            }
            Action::AgentsUp
            | Action::AgentsDown
            | Action::AgentsPageUp
            | Action::AgentsPageDown
            | Action::AgentsTop
            | Action::AgentsBottom => {
                if self.mode != PanelMode::List && self.follow {
                    self.scroll = self.rendered_scroll.get();
                }
                let delta = if matches!(action, Action::AgentsPageUp | Action::AgentsPageDown) {
                    step
                } else {
                    1
                };
                let position = if self.mode == PanelMode::List {
                    &mut self.cursor
                } else {
                    &mut self.scroll
                };
                match action {
                    Action::AgentsTop => *position = 0,
                    Action::AgentsBottom => {
                        *position = if self.mode == PanelMode::List {
                            total.saturating_sub(1)
                        } else {
                            usize::MAX
                        }
                    }
                    Action::AgentsUp | Action::AgentsPageUp => {
                        *position = position.saturating_sub(delta)
                    }
                    _ => *position = position.saturating_add(delta),
                }
                if self.mode == PanelMode::List {
                    self.cursor = self.cursor.min(total.saturating_sub(1));
                } else {
                    self.follow = action == Action::AgentsBottom;
                }
            }
            Action::AgentsStop => return self.selected_id(tree).map(SubagentAction::Stop),
            Action::AgentsStopAll => {
                let armed = self
                    .armed_at
                    .take()
                    .is_some_and(|t| self.now_ms.saturating_sub(t) <= 3000);
                if armed {
                    return Some(SubagentAction::ConfirmStopAll);
                }
                self.armed_at = Some(self.now_ms);
                return Some(SubagentAction::StopAll);
            }
            Action::AgentsPause => {
                if matches!(self.spawning.get(), Spawning::Paused(_)) {
                    return Some(SubagentAction::Resume);
                }
                self.previous = self.mode;
                self.mode = PanelMode::ConfirmPause;
                return Some(SubagentAction::RequestPause);
            }
            Action::AgentsSteer => {
                if let Some(row) = self.selected(tree) {
                    let refusal = if row.state == RowState::Queued {
                        Some(ControlOutcome::NotStarted {
                            goal: row.goal.clone(),
                        })
                    } else if row.state.is_terminal() {
                        Some(ControlOutcome::AlreadyFinished {
                            goal: row.goal.clone(),
                        })
                    } else if row.accepting_steer == Some(false) {
                        Some(ControlOutcome::SteerRejected {
                            goal: row.goal.clone(),
                        })
                    } else {
                        None
                    };
                    if let Some(outcome) = refusal {
                        self.flash = Some(outcome_text(&outcome));
                    } else {
                        self.shown_id = Some(row.id.clone());
                        self.previous = self.mode;
                        self.mode = PanelMode::Steer;
                        self.draft.clear();
                    }
                }
            }
            _ => {}
        }
        None
    }
    fn hint(&self, action: Action, description: &str) -> Option<String> {
        self.keys.label(action).map(|key| {
            format!(
                "{} {description}",
                if key == "Shift+X" {
                    "X".into()
                } else {
                    key.to_lowercase()
                }
            )
        })
    }
    fn footer(&self) -> String {
        let stop_all = if self.armed_at.is_some() {
            "again to stop all"
        } else {
            "stop all"
        };
        let pause = if matches!(self.spawning.get(), Spawning::Paused(_)) {
            "resume spawns"
        } else {
            "pause spawns"
        };
        [
            (Action::AgentsTail, "tail"),
            (Action::AgentsDetail, "detail"),
            (Action::AgentsSteer, "steer"),
            (Action::AgentsStop, "stop"),
            (Action::AgentsStopAll, stop_all),
            (Action::AgentsPause, pause),
            (Action::AgentsNextSession, "session"),
            (
                Action::AgentsBack,
                if self.mode == PanelMode::List {
                    "close"
                } else {
                    "back"
                },
            ),
        ]
        .into_iter()
        .filter_map(|(a, s)| self.hint(a, s))
        .collect::<Vec<_>>()
        .join(" · ")
    }
    #[allow(clippy::too_many_arguments)]
    pub fn render(
        &self,
        tree: &SubagentTree,
        header: &PanelHeader,
        tail: Option<&TailView>,
        width: u16,
        height: u16,
        now: Clock,
    ) -> Vec<Line<'static>> {
        if width == 0 || height == 0 {
            return vec![];
        }
        self.spawning.set(header.spawning);
        let counts = tree.counts();
        if !matches!(header.spawning, Spawning::Unknown) && self.running.get().is_none() {
            self.running
                .set(Some(counts.live.saturating_sub(counts.queued)));
        }
        let style = &self.style;
        let mut lines = vec![Line::styled(
            panel_header(tree, header, &self.key.session_id, width),
            style.header,
        )];
        let mut budget = usize::from(height.saturating_sub(3));
        self.viewport.set(budget);
        let mode = if matches!(self.mode, PanelMode::Steer | PanelMode::ConfirmPause) {
            self.previous
        } else {
            self.mode
        };
        let mut body = match mode {
            PanelMode::Detail => self
                .selected(tree)
                .map(|r| detail(r, width, now))
                .unwrap_or_default(),
            PanelMode::Tail => self
                .selected(tree)
                .map(|r| tail_lines(r, tail, width))
                .unwrap_or_default(),
            _ => {
                let rows = tree.rows_in_order();
                if rows.is_empty() {
                    vec![Line::from(
                        "No subagents in this session yet. They appear when the agent uses delegate_task.",
                    )]
                } else {
                    rows.iter()
                        .enumerate()
                        .map(|(i, e)| {
                            list_row(
                                tree.row(e.index),
                                e.depth,
                                i == self.cursor.min(rows.len() - 1),
                                width,
                                now,
                                style,
                            )
                        })
                        .collect()
                }
            }
        };
        if mode == PanelMode::Tail && !body.is_empty() && budget > 0 {
            lines.push(body.remove(0));
            budget -= 1;
            self.viewport.set(budget);
        }
        let start = if mode == PanelMode::List {
            self.cursor.saturating_sub(budget.saturating_sub(1))
        } else if self.follow && mode == PanelMode::Tail {
            body.len().saturating_sub(budget)
        } else {
            self.scroll.min(body.len().saturating_sub(budget))
        };
        self.rendered_scroll.set(start);
        lines.extend(body.into_iter().skip(start).take(budget));
        let flash = if self.mode == PanelMode::Steer {
            let goal = self
                .selected(tree)
                .map(|r| literal(&r.goal))
                .unwrap_or_default();
            let prefix = cut_to_width(
                &format!("Steer \"{goal}\": {}", literal(&self.draft)),
                width.saturating_sub(1),
            );
            Some(Line::from(vec![
                Span::raw(prefix),
                Span::styled(" ", Style::new().reversed()),
            ]))
        } else if self.mode == PanelMode::ConfirmPause {
            let count = self
                .running
                .get()
                .filter(|_| !matches!(header.spawning, Spawning::Unknown))
                .map(|n| format!(" {n} running now will continue."))
                .unwrap_or_default();
            Some(Line::from(format!(
                "Pause new subagents for every session on this Hermes gateway?{count} y/n"
            )))
        } else {
            self.flash.as_ref().map(|s| Line::from(s.clone()))
        };
        if let Some(flash) = flash {
            lines.push(flash);
        }
        lines.push(Line::styled(self.footer(), style.hint));
        lines
            .into_iter()
            .take(usize::from(height))
            .map(|line| {
                if line.width() <= usize::from(width) {
                    line
                } else {
                    let text: String = line.spans.iter().map(|s| s.content.as_ref()).collect();
                    Line::styled(cut_to_width(&text, width), line.style)
                }
            })
            .collect()
    }
}

fn panel_header(tree: &SubagentTree, header: &PanelHeader, fallback: &str, width: u16) -> String {
    let counts = tree.counts();
    let title = if header.title.trim().is_empty() {
        fallback
    } else {
        &header.title
    };
    let left = format!("Subagents · {}", literal(title));
    let (state, caps) = match header.spawning {
        Spawning::Unknown => ("spawning state unknown", None),
        Spawning::On(c) => ("spawning on", c),
        Spawning::Paused(c) => ("spawning paused for all sessions", c),
    };
    let base = format!(
        "{} running · {} finished · {state}",
        counts.live.saturating_sub(counts.queued),
        counts.terminal
    );
    let full = caps
        .map(|c| {
            format!(
                "{base} · depth ≤{} · ≤{} at once",
                c.max_spawn_depth, c.max_concurrent_children
            )
        })
        .unwrap_or_else(|| base.clone());
    let w = usize::from(width);
    let right = if left.width() + 2 + full.width() <= w {
        full
    } else {
        base
    };
    if right.width() + 2 >= w {
        return cut_to_width(&right, width);
    }
    let left = cut_to_width(&left, (w - right.width() - 2) as u16);
    format!(
        "{left}{}{right}",
        " ".repeat(w - left.width() - right.width())
    )
}
fn elapsed(row: &SubagentRow, now: Clock) -> String {
    if row.state.is_terminal() {
        row.duration_s.map(format_elapsed).unwrap_or_default()
    } else {
        format_elapsed(
            row.started_at
                .map(|t| now.wall_s - t)
                .unwrap_or_else(|| now.mono_ms.saturating_sub(row.first_seen_ms) as f64 / 1000.0),
        )
    }
}
fn pad(text: &str, width: u16) -> String {
    let text = cut_to_width(text, width);
    format!(
        "{text}{}",
        " ".repeat(usize::from(width).saturating_sub(text.width()))
    )
}
fn tools(row: &SubagentRow) -> String {
    row.tool_count
        .map(|n| format!("{n} {}", if n == 1 { "tool" } else { "tools" }))
        .unwrap_or_default()
}
fn list_row(
    row: &SubagentRow,
    depth: u8,
    selected: bool,
    width: u16,
    now: Clock,
    style: &SubagentStyle,
) -> Line<'static> {
    let prefix = format!(
        "{}{}{} ",
        if selected { "▸ " } else { "  " },
        " ".repeat(2 * usize::from(depth.min(8))),
        row.state.glyph()
    );
    let prefix_w = prefix.width() as u16;
    let goal_w = width.saturating_sub(prefix_w + 75);
    let goal = literal(&row.goal);
    let elapsed = elapsed(row, now);
    let text = if width >= 100 && goal_w >= 10 {
        let model = row
            .model
            .as_deref()
            .unwrap_or_default()
            .rsplit('/')
            .next()
            .unwrap_or_default();
        format!(
            "{prefix}{}  {}  {elapsed:>7}  {}  {}",
            pad(&goal, goal_w),
            pad(&literal(model), 24),
            pad(&tools(row), 8),
            cut_to_width(&activity(row), 28)
        )
    } else {
        format!(
            "{prefix}{}  {elapsed:>7}",
            cut_to_width(&goal, width.saturating_sub(prefix_w + 9))
        )
    };
    let mut line = styled_row(
        text.trim_end().to_owned(),
        row.state.glyph(),
        row.state,
        width,
        style,
        selected,
    );
    if width >= 100 && goal_w >= 10 {
        let suffix = cut_to_width(&activity(row), 28);
        let dim = if selected {
            style.hint.reversed()
        } else {
            style.hint
        };
        dim_suffix(&mut line, suffix.trim_end(), dim);
    }
    line
}
fn status(state: RowState) -> &'static str {
    match state {
        RowState::Queued => "queued",
        RowState::Running => "running",
        RowState::Completed => "completed",
        RowState::Failed(FailKind::Failed) => "failed",
        RowState::Failed(FailKind::Error) => "error",
        RowState::Failed(FailKind::Timeout) => "timed out",
        RowState::Interrupted => "stopped",
        RowState::Gone => "ended; result not seen",
    }
}
fn wrap(text: &str, width: u16) -> Vec<Line<'static>> {
    if width == 0 {
        return vec![];
    }
    let mut result = Vec::new();
    let mut line = String::new();
    for c in sanitize(text).chars() {
        if c == '\n' {
            result.push(Line::from(std::mem::take(&mut line)));
            continue;
        }
        if format!("{line}{c}").width() > usize::from(width) && !line.is_empty() {
            result.push(Line::from(std::mem::take(&mut line)));
        }
        line.push(c);
    }
    if !line.is_empty() {
        result.push(Line::from(line));
    }
    result
}
fn detail(row: &SubagentRow, width: u16, now: Clock) -> Vec<Line<'static>> {
    let mut fields = vec![
        format!("Goal: {}", literal(&row.goal)),
        format!(
            "Status: {}{}",
            status(row.state),
            if row.stop_requested {
                " (stopping)"
            } else {
                ""
            }
        ),
        format!("Id: {}", literal(&row.id)),
    ];
    if let Some(parent) = &row.parent_id {
        fields.push(format!("Parent: {}", literal(parent)));
    }
    if let Some(depth) = row.depth {
        fields.push(format!("Depth: {depth}"));
    }
    if let Some(model) = &row.model {
        fields.push(format!("Model: {}", literal(model)));
    }
    if !row.toolsets.is_empty() {
        fields.push(format!("Toolsets: {}", literal(&row.toolsets.join(", "))));
    }
    fields.push(format!("Started: {} ago", elapsed(row, now)));
    if let Some(duration) = row.duration_s {
        fields.push(format!("Duration: {}", format_elapsed(duration)));
    }
    if let Some(n) = row.tool_count {
        fields.push(format!(
            "Tools: {n}{}",
            row.last_tool
                .as_ref()
                .map(|t| format!(", last {}", literal(t)))
                .unwrap_or_default()
        ));
    }
    let tokens = [
        ("in", row.input_tokens),
        ("out", row.output_tokens),
        ("reasoning", row.reasoning_tokens),
    ]
    .into_iter()
    .filter_map(|(k, v)| v.map(|v| format!("{k} {v}")))
    .collect::<Vec<_>>();
    if !tokens.is_empty() {
        fields.push(format!("Tokens: {}", tokens.join(" · ")));
    }
    if let Some(n) = row.api_calls {
        fields.push(format!("API calls: {n}"));
    }
    for (label, files) in [
        ("Files read", &row.files_read),
        ("Files written", &row.files_written),
    ] {
        if !files.is_empty() {
            fields.push(format!(
                "{label}: {}",
                files
                    .iter()
                    .take(10)
                    .map(|f| literal(f))
                    .collect::<Vec<_>>()
                    .join(", ")
            ));
            if files.len() > 10 {
                fields.push(format!("  … +{} more", files.len() - 10));
            }
        }
    }
    if let Some(summary) = &row.summary {
        fields.push(format!("Summary: {}", literal(summary)));
    }
    if !row.activity.is_empty() {
        fields.push("Activity:".into());
        fields.extend(row.activity.iter().map(|a| literal(&a.text)));
    }
    fields.push(format!(
        "Steering: {}",
        match row.accepting_steer {
            Some(true) => "accepted",
            Some(false) => "closed",
            None => "unknown",
        }
    ));
    fields.into_iter().flat_map(|s| wrap(&s, width)).collect()
}
fn tail_lines(row: &SubagentRow, tail: Option<&TailView>, width: u16) -> Vec<Line<'static>> {
    let mut lines = vec![Line::from(format!(
        "Live transcript · \"{}\"{}",
        literal(&row.goal),
        if tail.is_some_and(|t| t.truncated) {
            " (latest 16 KB; older lines cut)"
        } else {
            ""
        }
    ))];
    if let Some(text) = tail.and_then(|t| t.text.as_ref()).filter(|t| !t.is_empty()) {
        for line in text.lines() {
            lines.extend(wrap(&line.replace('\t', "    "), width));
        }
    } else {
        lines.push(Line::from(if tail.is_some_and(|t| t.ever_failed) {
            "Live transcript unavailable."
        } else {
            "No live transcript yet."
        }));
    }
    lines
}
pub fn batch_block_lines(
    batch: &FinishedBatch,
    width: u16,
    style: &SubagentStyle,
) -> Vec<Line<'static>> {
    if width == 0 {
        return vec![];
    }
    let completed = batch
        .rows
        .iter()
        .filter(|(r, _)| r.state == RowState::Completed)
        .count();
    let failed = batch
        .rows
        .iter()
        .filter(|(r, _)| matches!(r.state, RowState::Failed(_) | RowState::Gone))
        .count();
    let stopped = batch
        .rows
        .iter()
        .filter(|(r, _)| r.state == RowState::Interrupted)
        .count();
    let mut header = format!("⎇ Subagents · {completed} finished");
    if failed > 0 {
        header.push_str(&format!(" · {failed} failed"));
    }
    if stopped > 0 {
        header.push_str(&format!(" · {stopped} stopped"));
    }
    if let Some(duration) = batch
        .rows
        .iter()
        .filter_map(|(r, _)| r.duration_s)
        .reduce(f64::max)
    {
        header.push_str(&format!(" · {}", format_elapsed(duration)));
    }
    let mut lines = vec![Line::styled(cut_to_width(&header, width), style.header)];
    for (row, depth) in batch.rows.iter().take(20) {
        let mut text = format!(
            "{}{} {}",
            " ".repeat(2 + 2 * usize::from(*depth)),
            row.state.glyph(),
            literal(&row.goal)
        );
        if let Some(d) = row.duration_s {
            text.push_str(&format!(" · {}", format_elapsed(d)));
        }
        if row.tool_count.is_some() {
            text.push_str(&format!(" · {}", tools(row)));
        }
        let summary = row
            .summary
            .as_deref()
            .unwrap_or_default()
            .lines()
            .next()
            .unwrap_or_default();
        let outcome = match row.state {
            RowState::Completed => literal(summary),
            RowState::Failed(_) => format!("{}: {}", status(row.state), literal(summary)),
            _ => status(row.state).into(),
        };
        if !outcome.is_empty() {
            text.push_str(&format!(" · {outcome}"));
        }
        lines.push(styled_row(
            text,
            row.state.glyph(),
            row.state,
            width,
            style,
            false,
        ));
    }
    if batch.rows.len() > 20 {
        lines.push(Line::styled(
            cut_to_width(&format!("  … +{} more", batch.rows.len() - 20), width),
            style.hint,
        ));
    }
    lines
}
pub fn format_elapsed(seconds: f64) -> String {
    let s = if seconds.is_finite() {
        seconds.max(0.0) as u64
    } else {
        0
    };
    if s < 60 {
        format!("{s}s")
    } else if s < 3600 {
        format!("{}m {:02}s", s / 60, s % 60)
    } else {
        format!("{}h {:02}m", s / 3600, (s % 3600) / 60)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    fn panel() -> AgentsPanel {
        AgentsPanel::new(RowKey {
            profile: "fixture".into(),
            session_id: "session".into(),
        })
    }

    #[test]
    fn pause_answers_are_explicit_and_pause_state_resumes_directly() {
        let tree = SubagentTree::new();
        let mut panel = panel();
        panel.handle(Action::AgentsPause, &tree);
        panel.type_char('z');
        assert!(panel.take_pending_action().is_none());
        assert_eq!(panel.mode(), PanelMode::ConfirmPause);
        panel.type_char('Y');
        assert_eq!(
            panel.take_pending_action(),
            Some(SubagentAction::ConfirmPause(true))
        );
        assert_eq!(panel.mode(), PanelMode::List);
        panel.set_spawning(Spawning::Paused(None), None);
        assert_eq!(
            panel.handle(Action::AgentsPause, &tree),
            Some(SubagentAction::Resume)
        );
    }

    #[test]
    fn stop_all_arm_expires_and_close_is_an_intent() {
        let tree = SubagentTree::new();
        let mut panel = panel();
        assert_eq!(
            panel.handle(Action::AgentsStopAll, &tree),
            Some(SubagentAction::StopAll)
        );
        panel.tick(3001);
        assert_eq!(
            panel.handle(Action::AgentsStopAll, &tree),
            Some(SubagentAction::StopAll)
        );
        assert_eq!(
            panel.handle(Action::AgentsStopAll, &tree),
            Some(SubagentAction::ConfirmStopAll)
        );
        panel.handle(Action::AgentsBack, &tree);
        assert!(panel.take_close_request());
        assert!(!panel.take_close_request());
    }

    #[test]
    fn footer_uses_rebound_keys_and_omits_unbound_actions() {
        let mut bindings = BTreeMap::new();
        bindings.insert("agents_stop".into(), vec!["ctrl+x".into()]);
        bindings.insert("agents_steer".into(), vec![]);
        let (map, notices) = KeyMap::from_entries(&bindings);
        assert!(notices.is_empty());
        let mut panel = panel();
        panel.set_keymap(&map);
        assert!(panel.footer().contains("ctrl+x stop"));
        assert!(!panel.footer().contains("steer"));
    }
}
