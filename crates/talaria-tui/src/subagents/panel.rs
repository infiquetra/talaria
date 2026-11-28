//! Full-screen subagent views and finished batch rendering.
use super::{
    AgentClock, SubagentStyle,
    dock::{activity, clean_activity, literal, route_label, styled_row},
    labels::task_labels,
};
use crate::{
    checklist::cut_to_width,
    history::{sanitize, word_wrapped},
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
    subagents::{
        Clock, FailKind, FinishedBatch, RowState, Spawning, SubagentRow, SubagentTree, TreeRow,
    },
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
    motion: crate::theme::Motion,
    next_frame_ms: Cell<Option<u64>>,
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
            motion: crate::theme::Motion::Reduced,
            next_frame_ms: Cell::new(None),
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
    pub fn set_motion(&mut self, motion: crate::theme::Motion) {
        self.motion = motion;
    }
    pub fn wake_after_ms(&self) -> Option<u64> {
        self.next_frame_ms.get()
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
    /// The child session selected in a viewing mode. A queued child may have no session yet.
    pub fn pane_target(&self, tree: &SubagentTree) -> Option<(RowKey, String)> {
        if !matches!(
            self.mode,
            PanelMode::List | PanelMode::Detail | PanelMode::Tail
        ) {
            return None;
        }
        let row = self.selected(tree)?;
        let session_id = row.child_session_id.as_ref().filter(|id| !id.is_empty())?;
        Some((
            RowKey {
                profile: self.key.profile.clone(),
                session_id: session_id.clone(),
            },
            row.goal.clone(),
        ))
    }
    fn selected<'a>(&self, tree: &'a SubagentTree) -> Option<&'a SubagentRow> {
        if self.mode != PanelMode::List
            && let Some(id) = &self.shown_id
        {
            return tree.get(id);
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
    pub fn clear_flash(&mut self) {
        self.flash = None;
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
                self.flash = Some("Type an instruction first.".to_owned());
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
                } else if key.len() == 1 {
                    key.to_lowercase()
                } else {
                    key
                }
            )
        })
    }
    /// Key hints in three groups: look at a child, control it, move around.
    fn footer_groups(&self) -> Vec<String> {
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
        let back = if self.mode == PanelMode::List {
            "close"
        } else {
            "back"
        };
        let groups: [&[(Action, &str)]; 3] = [
            &[
                (Action::AgentsTail, "tail"),
                (Action::AgentsDetail, "detail"),
                (Action::AgentsOpenInPane, "new pane"),
            ],
            &[
                (Action::AgentsSteer, "steer"),
                (Action::AgentsStop, "stop"),
                (Action::AgentsStopAll, stop_all),
                (Action::AgentsPause, pause),
            ],
            &[
                (Action::AgentsNextSession, "session"),
                (Action::AgentsBack, back),
            ],
        ];
        groups
            .into_iter()
            .map(|group| {
                group
                    .iter()
                    .filter_map(|(a, s)| self.hint(*a, s))
                    .collect::<Vec<_>>()
                    .join(" · ")
            })
            .filter(|group| !group.is_empty())
            .collect()
    }
    fn footer(&self) -> String {
        self.footer_groups().join("   ")
    }
    /// The groups run together with the same separator when the gaps do not fit.
    fn footer_for(&self, width: u16) -> String {
        let spaced = self.footer();
        if spaced.width() <= usize::from(width) {
            spaced
        } else {
            let hints = self.footer_groups().join(" · ");
            if self.keys.label(Action::AgentsBack).is_some() {
                crate::footer::with_recovery(&hints, width)
            } else {
                hints
            }
        }
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
        self.next_frame_ms.set(None);
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
        let mode = if matches!(self.mode, PanelMode::Steer | PanelMode::ConfirmPause) {
            self.previous
        } else {
            self.mode
        };
        let rows = tree.rows_in_order();
        let mut lines = vec![Line::styled(
            panel_header(tree, header, width),
            style.header,
        )];
        // A short terminal keeps its rows and gives up the extra header lines.
        let tall = height >= 12;
        if tall {
            lines.push(Line::styled(
                cut_to_width(
                    &status_line(header.spawning, shared_model(tree, &rows)),
                    width,
                ),
                style.hint,
            ));
        }
        let clock = AgentClock {
            now_ms: now.mono_ms,
            motion: self.motion,
        };
        let list = (mode == PanelMode::List && !rows.is_empty())
            .then(|| ListLayout::new(tree, &rows, width, now, clock));
        if let Some(list) = &list {
            if tall && let Some(shared) = &list.caption {
                lines.push(Line::styled(
                    cut_to_width(&format!("Task: {shared}"), width),
                    style.hint,
                ));
            }
            if height >= 8 {
                lines.push(Line::styled(list.column_header(), style.hint));
            }
        }
        let mut budget = usize::from(height).saturating_sub(lines.len() + 2);
        self.viewport.set(budget);
        let mut body = match mode {
            PanelMode::Detail => self
                .selected(tree)
                .map(|r| detail(r, &self.key.session_id, width, now))
                .unwrap_or_default(),
            PanelMode::Tail => self
                .selected(tree)
                .map(|r| tail_lines(r, tail, width))
                .unwrap_or_default(),
            _ => match &list {
                None => vec![Line::from(
                    "No subagents in this session yet. They appear when the agent uses delegate_task.",
                )],
                Some(list) => {
                    let current = self.cursor.min(rows.len() - 1);
                    let start = current.saturating_sub(budget.saturating_sub(1));
                    self.next_frame_ms.set(
                        rows.iter()
                            .skip(start)
                            .take(budget)
                            .filter_map(|entry| clock.signal(tree.row(entry.index)).wake_ms)
                            .min(),
                    );
                    rows.iter()
                        .enumerate()
                        .skip(start)
                        .take(budget)
                        .map(|(i, entry)| {
                            list.row(i, entry, tree.row(entry.index), i == current, width, style)
                        })
                        .collect()
                }
            },
        };
        if mode == PanelMode::Tail && !body.is_empty() && budget > 0 {
            lines.push(body.remove(0));
            budget -= 1;
            self.viewport.set(budget);
        }
        let start = if mode == PanelMode::List {
            0
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
        lines.truncate(usize::from(height.saturating_sub(1)));
        lines.push(Line::styled(self.footer_for(width), style.hint));
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

fn panel_header(tree: &SubagentTree, header: &PanelHeader, width: u16) -> String {
    let counts = tree.counts();
    let title = if header.title.trim().is_empty() {
        "Untitled session"
    } else {
        &header.title
    };
    let left = format!("Subagents · {}", literal(title));
    let mut right = format!("{} running", counts.live.saturating_sub(counts.queued));
    for (count, word) in [
        (counts.queued, "queued"),
        (counts.terminal, "finished"),
        (counts.failed, "failed"),
    ] {
        if count > 0 {
            right.push_str(&format!(" · {count} {word}"));
        }
    }
    let w = usize::from(width);
    if right.width() + 2 >= w {
        return cut_to_width(&right, width);
    }
    let left = cut_to_width(&left, (w - right.width() - 2) as u16);
    format!(
        "{left}{}{right}",
        " ".repeat(w - left.width() - right.width())
    )
}
/// The model every row uses, when there is one.
fn shared_model(tree: &SubagentTree, rows: &[TreeRow]) -> Option<String> {
    let first = model_name(tree.row(rows.first()?.index));
    rows.iter()
        .all(|entry| model_name(tree.row(entry.index)) == first)
        .then_some(first)
        .filter(|model| !model.is_empty())
}
fn model_name(row: &SubagentRow) -> String {
    route_label(row, u16::MAX)
}
/// The second header line: what spawning allows, plus the model when all rows use one.
fn status_line(spawning: Spawning, model: Option<String>) -> String {
    let (state, caps) = match spawning {
        Spawning::Unknown => ("Spawning state unknown", None),
        Spawning::On(c) => ("Spawning on", c),
        Spawning::Paused(c) => ("Spawning paused for all sessions", c),
    };
    let mut text = state.to_owned();
    if let Some(c) = caps {
        text.push_str(&format!(
            " · up to {} at once · max nesting {}",
            c.max_concurrent_children, c.max_spawn_depth
        ));
    }
    if let Some(model) = model {
        text.push_str(&format!(" · model {model}"));
    }
    text
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
/// Narrow terminals drop the model, tool count and activity columns.
const WIDE_LIST: u16 = 100;
/// Least columns the activity cell keeps when the task column grows.
const MIN_ACTIVITY: usize = 20;

fn right_align(text: &str, width: usize) -> String {
    format!("{}{text}", " ".repeat(width.saturating_sub(text.width())))
}

/// Column sizes and cell text for the list, worked out from the rows in view.
struct ListLayout {
    labels: Vec<String>,
    caption: Option<String>,
    /// Columns from after the marker to the end of the task label.
    task_w: usize,
    /// The model column, absent when every row uses one model or the terminal is narrow.
    models: Option<(Vec<String>, usize)>,
    times: Vec<String>,
    time_w: usize,
    clock: AgentClock,
    /// The tool count column, absent on a narrow terminal.
    tools: Option<(Vec<String>, usize)>,
}
impl ListLayout {
    fn new(
        tree: &SubagentTree,
        rows: &[TreeRow],
        width: u16,
        now: Clock,
        clock: AgentClock,
    ) -> Self {
        let width = usize::from(width);
        let wide = width >= usize::from(WIDE_LIST);
        let goals: Vec<_> = rows
            .iter()
            .map(|e| tree.row(e.index).goal.as_str())
            .collect();
        let found = task_labels(&goals);
        let times: Vec<String> = rows
            .iter()
            .map(|e| {
                let row = tree.row(e.index);
                let signal = clock.signal(row);
                if signal.glyph == '◌' {
                    signal.label.into_owned()
                } else {
                    format!("{} {}", signal.label, elapsed(row, now))
                        .trim_end()
                        .to_owned()
                }
            })
            .collect();
        let time_w = times
            .iter()
            .map(|t| t.width())
            .max()
            .unwrap_or(0)
            .max("Time".width());
        let names: Vec<String> = rows.iter().map(|e| model_name(tree.row(e.index))).collect();
        let models = (width >= 60 && names.iter().any(|name| name != &names[0])).then(|| {
            let w = names
                .iter()
                .map(|n| n.width())
                .max()
                .unwrap_or(0)
                .max("Model".width());
            let cols = w.min(width / 3);
            (
                rows.iter()
                    .map(|e| route_label(tree.row(e.index), cols as u16))
                    .collect(),
                cols,
            )
        });
        let tools = wide.then(|| {
            let counts: Vec<String> = rows
                .iter()
                .map(|e| {
                    tree.row(e.index)
                        .tool_count
                        .map(|n| n.to_string())
                        .unwrap_or_default()
                })
                .collect();
            let w = counts
                .iter()
                .map(|c| c.width())
                .max()
                .unwrap_or(0)
                .max("Tools".width());
            (counts, w)
        });
        let mut fixed = 2 + 2 + time_w;
        if let Some((_, w)) = &models {
            fixed += 2 + w;
        }
        if let Some((_, w)) = &tools {
            fixed += 2 + w;
        }
        if wide {
            fixed += 2 + MIN_ACTIVITY;
        }
        let room = width.saturating_sub(fixed);
        let limit = if wide {
            room.min((width * 2 / 5).max(14))
        } else {
            room
        };
        let natural = rows
            .iter()
            .zip(&found.labels)
            .map(|(e, label)| 2 * usize::from(e.depth.min(8)) + 2 + label.width())
            .max()
            .unwrap_or(0);
        Self {
            labels: found.labels,
            caption: found.shared,
            task_w: natural.min(limit.max(14)),
            models,
            times,
            time_w,
            clock,
            tools,
        }
    }
    /// Dim column titles lined up over the cells.
    fn column_header(&self) -> String {
        let mut text = format!("  {}", pad("  Task", self.task_w as u16));
        if let Some((_, w)) = &self.models {
            text.push_str(&format!("  {}", pad("Model", *w as u16)));
        }
        text.push_str(&format!("  {}", right_align("State", self.time_w)));
        if let Some((_, w)) = &self.tools {
            text.push_str(&format!("  {}", right_align("Tools", *w)));
            text.push_str("  Activity");
        }
        text.trim_end().to_owned()
    }
    fn row(
        &self,
        at: usize,
        entry: &TreeRow,
        row: &SubagentRow,
        selected: bool,
        width: u16,
        style: &SubagentStyle,
    ) -> Line<'static> {
        let indent = 2 * usize::from(entry.depth.min(8));
        let base = if selected {
            Style::new().reversed().bold()
        } else {
            Style::new()
        };
        let dim = if selected {
            style.hint.reversed()
        } else {
            style.hint
        };
        let label = pad(
            &self.labels[at],
            self.task_w
                .saturating_sub(indent + 2)
                .min(usize::from(u16::MAX)) as u16,
        );
        // The marker, indent and status glyph keep their own look; the bar starts after the glyph.
        let mut cells = vec![(label, style.task.patch(base))];
        if let Some((names, w)) = &self.models {
            cells.push((
                format!("  {}", pad(&names[at], *w as u16)),
                style.model.patch(base),
            ));
        }
        cells.push((
            format!("  {}", right_align(&self.times[at], self.time_w)),
            base,
        ));
        if let Some((counts, w)) = &self.tools {
            cells.push((format!("  {}", right_align(&counts[at], *w)), base));
        }
        let used: usize =
            2 + indent + 2 + cells.iter().map(|(text, _)| text.width()).sum::<usize>();
        let mut spans = vec![
            Span::raw(format!(
                "{}{}",
                if selected { "▸ " } else { "  " },
                " ".repeat(indent)
            )),
            Span::styled(
                self.clock.signal(row).glyph.to_string(),
                style.state(row.state),
            ),
            Span::styled(" ", base),
        ];
        if self.tools.is_some() {
            let room = usize::from(width).saturating_sub(used + 2);
            let text = cut_to_width(&activity(row), room.min(usize::from(u16::MAX)) as u16);
            cells.push((format!("  {text}"), dim));
        }
        spans.extend(
            cells
                .into_iter()
                .map(|(text, style)| Span::styled(text, style)),
        );
        if selected {
            let gap = usize::from(width).saturating_sub(spans.iter().map(Span::width).sum());
            if gap > 0 {
                spans.push(Span::styled(" ".repeat(gap), Style::new().reversed()));
            }
        } else {
            while let Some(last) = spans.last_mut() {
                let kept = last.content.trim_end().len();
                if kept == 0 {
                    spans.pop();
                } else {
                    last.content = last.content[..kept].to_owned().into();
                    break;
                }
            }
        }
        Line::from(spans)
    }
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
fn detail(row: &SubagentRow, session_id: &str, width: u16, now: Clock) -> Vec<Line<'static>> {
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
        format!("Session ID: {}", literal(session_id)),
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
    fields.push(format!(
        "Reasoning effort: {}",
        literal(row.effort.as_deref().unwrap_or("unknown"))
    ));
    if row.state == RowState::Running {
        fields.push(format!(
            "Last update: {} ago",
            format_elapsed(now.mono_ms.saturating_sub(row.last_event_ms) as f64 / 1000.0)
        ));
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
        fields.extend(
            row.activity
                .iter()
                .map(clean_activity)
                .filter(|a| !a.is_empty()),
        );
    }
    fields.push(format!(
        "Steering: {}",
        match row.accepting_steer {
            Some(true) => "accepted",
            Some(false) => "closed",
            None => "unknown",
        }
    ));
    fields
        .into_iter()
        .flat_map(|text| word_wrapped(&text, width).into_iter().map(Line::from))
        .collect()
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
    let finished = batch.rows.len();
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
    let mut header = format!("⎇ Subagents · {finished} finished");
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
    let shared_route = batch.rows.first().filter(|(first, _)| {
        batch
            .rows
            .iter()
            .all(|(r, _)| r.model == first.model && r.effort == first.effort)
    });
    let mut heading = Line::styled(cut_to_width(&header, width), style.header);
    if let Some((row, _)) = shared_route {
        let room = usize::from(width).saturating_sub(heading.width() + 3);
        if room >= 15 {
            heading.spans.push(Span::styled(
                format!(" · {}", route_label(row, room as u16)),
                style.model,
            ));
        }
    }
    let mut lines = vec![heading];
    let goals: Vec<String> = batch
        .rows
        .iter()
        .take(20)
        .map(|(r, _)| literal(&r.goal))
        .collect();
    let goals: Vec<&str> = goals.iter().map(String::as_str).collect();
    let found = task_labels(&goals);
    if let Some(shared) = &found.shared {
        lines.push(Line::styled(
            cut_to_width(&format!("  Task: {shared}"), width),
            style.hint,
        ));
    }
    for ((row, depth), label) in batch.rows.iter().take(20).zip(&found.labels) {
        let mut text = format!(
            "{}{} {}",
            " ".repeat(2 + 2 * usize::from(*depth)),
            row.state.glyph(),
            cut_to_width(label, width.saturating_sub(4).min(56)),
        );
        if shared_route.is_none() {
            text.push_str(&format!(" · {}", route_label(row, width / 2)));
        }
        if let Some(d) = row.duration_s {
            text.push_str(&format!(" · {}", format_elapsed(d)));
        }
        if row.tool_count.is_some() {
            text.push_str(&format!(" · {}", tools(row)));
        }
        let preview = literal(&outcome_preview(row.summary.as_deref().unwrap_or_default()));
        let outcome = match row.state {
            RowState::Completed => preview,
            RowState::Failed(_) if preview.is_empty() => status(row.state).into(),
            RowState::Failed(_) => format!("{}: {preview}", status(row.state)),
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
/// JSON keys that say how a child's task went, most telling first.
const VERDICT_KEYS: [&str; 5] = ["verdict", "status", "result", "outcome", "summary"];
/// The short result to show for a finished child.
///
/// Hermes cuts a summary to 500 characters, so a JSON reply arrives as broken JSON and its
/// first line is just `{`. A recognised key gives `verdict: pass`; other JSON says so; plain
/// text gives its first sentence.
fn outcome_preview(summary: &str) -> String {
    let trimmed = summary.trim_start();
    let body = match trimmed.strip_prefix("```") {
        Some(fence) => fence
            .split_once('\n')
            .map_or("", |(_, rest)| rest)
            .trim_start(),
        None => trimmed,
    };
    if looks_like_json(body) {
        return json_outcome(body).unwrap_or_else(|| "JSON result".to_owned());
    }
    first_sentence(body)
}
/// Whether text opens like a JSON object or array, not a bracketed tag such as `[PASS]`.
fn looks_like_json(body: &str) -> bool {
    let Some(first) = body.chars().next() else {
        return false;
    };
    let after = body[first.len_utf8()..].trim_start();
    match first {
        '{' => after.is_empty() || after.starts_with(['"', '}']),
        '[' => {
            after.is_empty()
                || after.starts_with(['{', '[', '"', ']', '-'])
                || after.starts_with(|c: char| c.is_ascii_digit())
                || ["true", "false", "null"].iter().any(|word| {
                    after.starts_with(word)
                        && !after[word.len()..].starts_with(char::is_alphanumeric)
                })
        }
        _ => false,
    }
}
pub(super) fn first_sentence(text: &str) -> String {
    let line = text
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .unwrap_or_default();
    let end = line
        .char_indices()
        .find(|&(at, c)| {
            matches!(c, '.' | '!' | '?')
                && line[at + c.len_utf8()..].starts_with(char::is_whitespace)
        })
        .map_or(line.len(), |(at, c)| at + c.len_utf8());
    line[..end].to_owned()
}
/// Read the first recognised top-level key of a JSON object that may be cut short.
fn json_outcome(body: &str) -> Option<String> {
    if !body.starts_with('{') {
        return None;
    }
    let keys = top_level_keys(body);
    VERDICT_KEYS.into_iter().find_map(|wanted| {
        let (_, at) = keys.iter().find(|(key, _)| key == wanted)?;
        let value = serde_json::Deserializer::from_str(&body[*at..])
            .into_iter::<serde_json::Value>()
            .next()?
            .ok()?;
        let text = match value {
            serde_json::Value::String(text) => first_sentence(&text),
            serde_json::Value::Number(number) => number.to_string(),
            serde_json::Value::Bool(flag) => flag.to_string(),
            _ => return None,
        };
        (!text.is_empty()).then(|| format!("{wanted}: {text}"))
    })
}
/// Each key of the outermost object with the offset just after its colon.
fn top_level_keys(text: &str) -> Vec<(String, usize)> {
    let mut keys = Vec::new();
    let mut depth = 0usize;
    let mut chars = text.char_indices();
    while let Some((at, c)) = chars.next() {
        match c {
            '{' | '[' => depth += 1,
            '}' | ']' => depth = depth.saturating_sub(1),
            '"' => {
                let mut escaped = false;
                let mut end = None;
                for (to, d) in chars.by_ref() {
                    if escaped {
                        escaped = false;
                    } else if d == '\\' {
                        escaped = true;
                    } else if d == '"' {
                        end = Some(to);
                        break;
                    }
                }
                let Some(end) = end else { break };
                if depth == 1
                    && let Some(rest) = text[end + 1..].trim_start().strip_prefix(':')
                {
                    keys.push((text[at + 1..end].to_owned(), text.len() - rest.len()));
                }
            }
            _ => {}
        }
    }
    keys
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
    fn pane_target_uses_the_selected_child_session_and_its_owners_profile() {
        use talaria_core::subagents::{SubagentEvent, SubagentEventKind};
        let mut tree = SubagentTree::new();
        let mut event = SubagentEvent {
            kind: SubagentEventKind::Start,
            payload: serde_json::from_value(serde_json::json!({
                "subagent_id": "child", "goal": "child task", "task_index": 0, "task_count": 1
            }))
            .expect("contract payload"),
        };
        let now = Clock {
            mono_ms: 1,
            wall_s: 1.0,
        };
        tree.apply(&event, now);
        let mut panel = panel();
        assert_eq!(panel.pane_target(&tree), None);
        event.payload.child_session_id = Some("local-child".to_owned());
        tree.apply(&event, now);
        let target = Some((
            RowKey {
                profile: "fixture".to_owned(),
                session_id: "local-child".to_owned(),
            },
            "child task".to_owned(),
        ));
        assert_eq!(panel.pane_target(&tree), target);
        panel.handle(Action::AgentsDetail, &tree);
        assert_eq!(panel.pane_target(&tree), target);
        panel.handle(Action::AgentsTail, &tree);
        assert_eq!(panel.pane_target(&tree), target);
        panel.handle(Action::AgentsPause, &tree);
        assert_eq!(panel.pane_target(&tree), None);
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
        assert!(panel.footer().contains("Ctrl+X stop"));
        assert!(!panel.footer().contains("steer"));
    }

    #[test]
    fn outcome_preview_reads_verdict_from_truncated_json() {
        let cut = "{\n  \"item\": \"alpha\",\n  \"verdict\": \"pass\",\n  \"evidence\": [\"cut mid";
        assert_eq!(outcome_preview(cut), "verdict: pass");
        let fenced = "```json\n{\"verdict\": \"fail\", \"why\": \"x";
        assert_eq!(outcome_preview(fenced), "verdict: fail");
    }

    #[test]
    fn outcome_preview_prefers_keys_in_order_and_ignores_nested_ones() {
        let both = r#"{"status": "ok", "verdict": "pass"}"#;
        assert_eq!(outcome_preview(both), "verdict: pass");
        let status = r#"{"result": "done", "status": "ok"}"#;
        assert_eq!(outcome_preview(status), "status: ok");
        let nested = r#"{"detail": {"verdict": "pass"}, "notes": "cut"#;
        assert_eq!(outcome_preview(nested), "JSON result");
        let number = r#"{"result": 3, "x": "#;
        assert_eq!(outcome_preview(number), "result: 3");
    }

    #[test]
    fn outcome_preview_without_known_key_says_json_result() {
        assert_eq!(
            outcome_preview("{\n  \"problem_id\": \"p-vs-np\",\n  \"notes\": \"cut"),
            "JSON result"
        );
        assert_eq!(outcome_preview("{"), "JSON result");
        assert_eq!(outcome_preview("[1, 2"), "JSON result");
        assert_eq!(
            outcome_preview(r#"{"verdict": "cut mid wor"#),
            "JSON result"
        );
    }

    #[test]
    fn outcome_preview_takes_first_sentence() {
        assert_eq!(
            outcome_preview("Found the file. Then I read it.\nSecond line"),
            "Found the file."
        );
        assert_eq!(outcome_preview("\n\nRead it twice"), "Read it twice");
        assert_eq!(
            outcome_preview("Version 1.2 works fine"),
            "Version 1.2 works fine"
        );
        assert_eq!(outcome_preview(""), "");
    }

    #[test]
    fn outcome_preview_keeps_plain_text_that_starts_with_a_bracket() {
        assert_eq!(
            outcome_preview("[PASS] all green. More"),
            "[PASS] all green."
        );
        assert_eq!(
            outcome_preview("[README](README.md) updated with the new flags."),
            "[README](README.md) updated with the new flags."
        );
        assert_eq!(
            outcome_preview("[todo] wire it up. Later"),
            "[todo] wire it up."
        );
        assert_eq!(outcome_preview("{note} kept. Next"), "{note} kept.");
        assert_eq!(outcome_preview("[true, false]"), "JSON result");
        assert_eq!(outcome_preview("[ {\"a\": 1}"), "JSON result");
    }

    #[test]
    fn footer_groups_hints() {
        let panel = panel();
        assert_eq!(
            panel.footer(),
            "Enter tail · d detail · o new pane   e steer · x stop · X stop all · p pause spawns   Tab session · Esc close"
        );
    }
}
