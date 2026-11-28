use super::{WorkflowTheme, agent_status, clean, put, subagent_status, workflow_block};
use crate::theme::surfaces::{ViewStyles, tokens};
use ratatui::{
    buffer::Buffer,
    layout::{Position, Rect},
    style::{Modifier, Style},
    text::Line,
    widgets::{Block, Widget},
};
use std::collections::HashMap;
use talaria_core::subagents::RowState;
use talaria_workflow::progress::{
    AgentState, RunState,
    live::{NodeKey, ProgressKey, ProgressState, VisibleNode},
};

const CARD_HEIGHT: u16 = 3;
const ROW_PITCH: u16 = CARD_HEIGHT + 1;
const COLUMN_GAP: u16 = 4;
const MIN_CARD_WIDTH: u16 = 19;

struct Node<'a> {
    key: &'a NodeKey,
    parent: Option<usize>,
    children: Vec<usize>,
    level: u16,
    first_leaf: usize,
    last_leaf: usize,
}

impl Node<'_> {
    fn lane(&self) -> usize {
        self.first_leaf + (self.last_leaf - self.first_leaf) / 2
    }
}

/// Graph coordinates are derived from the visible hierarchy once per input or draw.
/// Each leaf owns a lane and every ancestor sits at the center of its descendant lanes.
pub(super) struct Map<'a> {
    panel: Rect,
    canvas: Rect,
    nodes: Vec<Node<'a>>,
    first_lane: usize,
    lanes: usize,
    lane_count: usize,
    first_level: u16,
    levels: u16,
    level_count: u16,
    card_width: u16,
}

impl<'a> Map<'a> {
    pub(super) fn new(panel: Rect, state: &ProgressState, rows: &'a [VisibleNode]) -> Self {
        let inner = Block::bordered().inner(panel);
        let canvas = Rect::new(
            inner.x + 1,
            inner.y + 1,
            inner.width.saturating_sub(2),
            inner.height.saturating_sub(2),
        );
        let mut nodes: Vec<Node<'_>> = Vec::with_capacity(rows.len());
        let mut ancestors: Vec<usize> = Vec::new();
        let mut seen = HashMap::with_capacity(rows.len());
        for (index, row) in rows.iter().enumerate() {
            while ancestors
                .last()
                .is_some_and(|&i| rows[i].depth >= row.depth)
            {
                ancestors.pop();
            }
            let parent = match &row.key {
                // The outline caps indentation. Keep the actual parent edge at deeper levels.
                NodeKey::Subagent { key, id } => {
                    let parent = state
                        .progress
                        .agent(key)
                        .and_then(|agent| agent.subagents.get(id))
                        .and_then(|sub| sub.parent_id.as_ref())
                        .and_then(|id| {
                            seen.get(&NodeKey::Subagent {
                                key: key.clone(),
                                id: id.clone(),
                            })
                        });
                    parent
                        .or_else(|| seen.get(&NodeKey::Agent(key.clone())))
                        .copied()
                }
                _ => ancestors.last().copied(),
            };
            let level = if let Some(parent) = parent {
                nodes[parent].children.push(index);
                nodes[parent].level.saturating_add(1)
            } else {
                0
            };
            nodes.push(Node {
                key: &row.key,
                parent,
                children: Vec::new(),
                level,
                first_leaf: usize::MAX,
                last_leaf: 0,
            });
            ancestors.push(index);
            seen.insert(&row.key, index);
        }
        let mut lanes = 0;
        for node in &mut nodes {
            if node.children.is_empty() {
                node.first_leaf = lanes;
                node.last_leaf = lanes;
                lanes += 1;
            }
        }
        for index in (0..nodes.len()).rev() {
            if let Some(parent) = nodes[index].parent {
                nodes[parent].first_leaf = nodes[parent].first_leaf.min(nodes[index].first_leaf);
                nodes[parent].last_leaf = nodes[parent].last_leaf.max(nodes[index].last_leaf);
            }
        }
        let levels = nodes
            .iter()
            .map(|node| node.level)
            .max()
            .unwrap_or(0)
            .saturating_add(1);
        let level_count =
            ((canvas.width + COLUMN_GAP) / (MIN_CARD_WIDTH + COLUMN_GAP)).clamp(1, levels);
        let lane_count = usize::from((canvas.height + 1) / ROW_PITCH);
        let focus = nodes.iter().find(|node| node.key == &state.selection);
        let first_level = focus.map_or(0, |node| {
            node.level
                .saturating_sub(level_count.saturating_sub(2))
                .min(levels - level_count)
        });
        let first_lane = focus.map_or(0, |node| {
            node.lane()
                .saturating_sub(lane_count / 2)
                .min(lanes.saturating_sub(lane_count))
        });
        let card_width = ((canvas.width + COLUMN_GAP) / level_count - COLUMN_GAP).min(37);
        Self {
            panel,
            canvas,
            nodes,
            first_lane,
            lanes,
            lane_count,
            first_level,
            levels,
            level_count,
            card_width,
        }
    }

    fn x(&self, level: u16) -> Option<u16> {
        let index = level.checked_sub(self.first_level)?;
        if index >= self.level_count {
            return None;
        }
        let total = self.level_count * (self.card_width + COLUMN_GAP) - COLUMN_GAP;
        Some(
            self.canvas.x
                + (self.canvas.width - total) / 2
                + index * (self.card_width + COLUMN_GAP),
        )
    }

    fn y(&self, lane: usize) -> Option<u16> {
        let index = lane.checked_sub(self.first_lane)?;
        if index >= self.lane_count {
            return None;
        }
        Some(self.canvas.y + u16::try_from(index).ok()? * ROW_PITCH)
    }

    fn card_rect(&self, node: &Node<'_>) -> Option<Rect> {
        Some(Rect::new(
            self.x(node.level)?,
            self.y(node.lane())?,
            self.card_width,
            CARD_HEIGHT,
        ))
    }

    pub(super) fn click(&self, position: Position) -> Option<ProgressKey> {
        self.nodes
            .iter()
            .filter(|node| node.key != &NodeKey::Run)
            .find(|node| {
                self.card_rect(node)
                    .is_some_and(|area| area.contains(position))
            })
            .map(|node| ProgressKey::SelectNode(node.key.clone()))
    }

    pub(super) fn vertical_key(&self, selection: &NodeKey, key: ProgressKey) -> ProgressKey {
        let Some(selected) = self.nodes.iter().find(|node| node.key == selection) else {
            return key;
        };
        if selected.key == &NodeKey::Run {
            return key;
        }
        let moving_up = key == ProgressKey::Up;
        let target = self
            .nodes
            .iter()
            .filter(|node| {
                node.level == selected.level
                    && if moving_up {
                        node.lane() < selected.lane()
                    } else {
                        node.lane() > selected.lane()
                    }
            })
            .min_by_key(|node| node.lane().abs_diff(selected.lane()));
        ProgressKey::SelectNode(target.map_or(selected.key, |node| node.key).clone())
    }

    pub(super) fn draw(
        &self,
        state: &ProgressState,
        theme: &WorkflowTheme,
        styles: Option<&ViewStyles>,
        buf: &mut Buffer,
    ) {
        let block = workflow_block(" Branch map ", theme, styles).border_style(theme.muted);
        let inner = block.inner(self.panel);
        block.render(self.panel, buf);
        let step = |index: usize| Row::of(state, self.nodes[index].key);
        let mut focus = self
            .nodes
            .iter()
            .position(|node| node.key == &state.selection);
        let mut path = Vec::new();
        while let Some(index) = focus {
            path.push(step(index).label);
            focus = self.nodes[index].parent;
        }
        path.reverse();
        let mut crumb = path.join(" / ");
        if unicode_width::UnicodeWidthStr::width(crumb.as_str()) > usize::from(inner.width) {
            while path.len() > 1 {
                path.remove(0);
                crumb = format!("… / {}", path.join(" / "));
                if unicode_width::UnicodeWidthStr::width(crumb.as_str()) <= usize::from(inner.width)
                {
                    break;
                }
            }
        }
        put(inner, 0, &crumb, theme.muted, buf);
        for node in &self.nodes {
            self.draw_edges(node, theme, styles, buf);
        }
        for node in &self.nodes {
            if let Some(area) = self.card_rect(node) {
                self.draw_card(state, node, area, theme, styles, buf);
            }
        }
        let mut overflow = Vec::new();
        if self.first_lane > 0 {
            overflow.push("↑ more branches");
        }
        if self.first_lane + self.lane_count < self.lanes {
            overflow.push("↓ more branches");
        }
        if self.first_level > 0 {
            overflow.push("← parent levels");
        }
        if self.first_level + self.level_count < self.levels {
            overflow.push("→ deeper levels");
        }
        if !overflow.is_empty() {
            put(
                inner,
                inner.height.saturating_sub(1),
                &overflow.join("  ·  "),
                theme.muted,
                buf,
            );
        } else if self.nodes.len() == 1 {
            let message = if state.collapsed.contains(&NodeKey::Run) {
                "everything is collapsed · → expands".to_owned()
            } else if state.filter != Default::default() {
                format!(
                    "no agents match the {} filter · f changes it",
                    state.filter.as_str()
                )
            } else {
                "No agents recorded".into()
            };
            put(
                inner,
                inner.height.saturating_sub(1),
                &message,
                theme.muted,
                buf,
            );
        }
    }

    fn draw_card(
        &self,
        state: &ProgressState,
        node: &Node<'_>,
        area: Rect,
        theme: &WorkflowTheme,
        styles: Option<&ViewStyles>,
        buf: &mut Buffer,
    ) {
        let row = Row::of(state, node.key);
        let selected = node.key == &state.selection && node.key != &NodeKey::Run;
        let tone = row.tone.style(theme, styles);
        let label_style = theme.normal.add_modifier(Modifier::BOLD);
        let border_style = if selected {
            let tint = styles
                .filter(|view| view.active() && view.color())
                .and(theme.selected.bg)
                .or(theme.normal.fg);
            Style {
                fg: tint,
                ..Style::default()
            }
            .add_modifier(Modifier::BOLD)
        } else {
            tone
        };
        let prefix = if selected { "> " } else { "" };
        let fold = if state.collapsed.contains(node.key) {
            "▸ "
        } else {
            ""
        };
        let label = clean(&format!(" {prefix}{fold}{} ", row.label), area.width - 2);
        let block = workflow_block("", theme, styles)
            .title(Line::styled(label, label_style))
            .border_style(border_style);
        let inner = block.inner(area);
        block.render(area, buf);
        put(inner, 0, &format!(" {}", row.status), tone, buf);
        if node.parent.is_some() && self.x(node.level - 1).is_some() {
            set(buf, area.x, area.y + 1, "┤", border_style, styles);
        }
        if !node.children.is_empty() && self.x(node.level.saturating_add(1)).is_some() {
            set(buf, area.right() - 1, area.y + 1, "├", border_style, styles);
        }
    }

    fn draw_edges(
        &self,
        node: &Node<'_>,
        theme: &WorkflowTheme,
        styles: Option<&ViewStyles>,
        buf: &mut Buffer,
    ) {
        let Some(left) = self.x(node.level) else {
            return;
        };
        let Some(right) = self.x(node.level.saturating_add(1)) else {
            return;
        };
        let (Some(&first), Some(&last)) = (node.children.first(), node.children.last()) else {
            return;
        };
        let start_x = left + self.card_width;
        let rail = start_x + 1;
        let lane = node.lane();
        if let Some(y) = self.y(lane) {
            set(buf, start_x, y + 1, "─", theme.muted, styles);
        }
        let lowest = self.nodes[first].lane().min(lane);
        let highest = self.nodes[last].lane().max(lane);
        let mut children = node.children.iter().peekable();
        let first_slot = self.first_lane.max(lowest);
        let last_slot = (self.first_lane + self.lane_count).min(highest + 1);
        for slot in first_slot..last_slot {
            while children
                .peek()
                .is_some_and(|&&i| self.nodes[i].lane() < slot)
            {
                children.next();
            }
            let west = slot == lane;
            let east = children
                .peek()
                .is_some_and(|&&i| self.nodes[i].lane() == slot);
            let Some(y) = self.y(slot).map(|y| y + 1) else {
                continue;
            };
            let glyph = match (slot > lowest, slot < highest, west, east) {
                (true, true, true, true) => "┼",
                (true, true, true, false) => "┤",
                (true, true, false, true) => "├",
                (true, true, false, false) => "│",
                (false, false, _, _) => "─",
                (false, true, true, true) => "┬",
                (false, true, true, false) => "┐",
                (false, true, false, _) => "┌",
                (true, false, true, true) => "┴",
                (true, false, true, false) => "┘",
                (true, false, false, _) => "└",
            };
            set(buf, rail, y, glyph, theme.muted, styles);
            if east {
                for x in rail + 1..right {
                    set(buf, x, y, "─", theme.muted, styles);
                }
            }
            if slot == self.first_lane && slot > lowest {
                for top in self.canvas.y..y {
                    set(buf, rail, top, "│", theme.muted, styles);
                }
            }
            if slot < highest {
                for bottom in y + 1..(y + ROW_PITCH).min(self.canvas.bottom()) {
                    set(buf, rail, bottom, "│", theme.muted, styles);
                }
            }
        }
    }
}

fn set(buf: &mut Buffer, x: u16, y: u16, glyph: &str, style: Style, styles: Option<&ViewStyles>) {
    let glyph = styles.map_or(glyph, |view| view.border_symbol(glyph));
    buf[(x, y)].set_symbol(glyph).set_style(style);
}

#[derive(Clone, Copy)]
pub(super) enum Tone {
    Normal,
    Muted,
    Success,
    Running,
    Warning,
    Failure,
}

impl Tone {
    pub(super) fn style(self, theme: &WorkflowTheme, styles: Option<&ViewStyles>) -> Style {
        use Tone::{Failure, Muted, Running, Success, Warning};
        let fallback = match self {
            Muted => theme.muted,
            Running | Success | Tone::Normal => theme.normal,
            Warning | Failure => theme.warning,
        };
        let token = match self {
            Tone::Normal => return theme.normal,
            Muted => tokens::WORKFLOW_METADATA,
            Success => tokens::LIST_SUCCESS,
            Running => tokens::LIST_RUNNING,
            Warning => tokens::WORKFLOW_WARNING,
            Failure => tokens::LIST_ERROR,
        };
        styles.map_or(fallback, |view| view.style(token, fallback))
    }
}

pub(super) struct Row {
    pub(super) label: String,
    pub(super) status: String,
    pub(super) tone: Tone,
}

impl Row {
    pub(super) fn of(state: &ProgressState, key: &NodeKey) -> Self {
        use Tone::{Failure, Muted, Running, Success, Warning};
        let (label, status, tone) = match key {
            NodeKey::Run => {
                let run = &state.progress.run;
                let (glyph, tone) = match run.state {
                    RunState::Running => ('◐', Running),
                    RunState::Completed => ('✓', Success),
                    RunState::CompletedWithFailures | RunState::BudgetExhausted => ('!', Warning),
                    RunState::Failed | RunState::Aborted => ('✗', Failure),
                };
                (
                    run.name.clone(),
                    format!("{glyph} {}", run.state.as_str()),
                    tone,
                )
            }
            NodeKey::Phase { path, title } => {
                let phase = state
                    .progress
                    .phases
                    .iter()
                    .find(|p| &p.workflow_path == path && &p.title == title);
                let (status, tone) = phase.map_or(("Phase".to_owned(), Muted), |phase| {
                    let c = &phase.counts;
                    let attention = c.failed + c.needs_you + c.ended;
                    let mark = if attention > 0 { "!" } else { "◇" };
                    (
                        format!("{mark} Phase · {}/{} done", c.done, c.total),
                        if attention > 0 { Warning } else { Muted },
                    )
                });
                (title.clone(), status, tone)
            }
            NodeKey::Child(path) => {
                let label = state
                    .progress
                    .children
                    .iter()
                    .find(|child| &child.workflow_path == path)
                    .map_or_else(String::new, |child| child.name.clone());
                (label, "◇ Child workflow".into(), Muted)
            }
            NodeKey::Agent(key) => {
                let Some(agent) = state.progress.agent(key) else {
                    return Self {
                        label: String::new(),
                        status: String::new(),
                        tone: Muted,
                    };
                };
                let (glyph, status) = agent_status(agent);
                let tone = match agent.state {
                    AgentState::Done
                        if agent.cut_short.is_none() && agent.guardrail_halt.is_none() =>
                    {
                        Success
                    }
                    AgentState::Running | AgentState::Stopping => Running,
                    AgentState::Failed if !agent.stopped => Failure,
                    AgentState::Unknown
                    | AgentState::Waiting
                    | AgentState::Ended
                    | AgentState::Done => Warning,
                    _ => Muted,
                };
                let mut status = format!("{glyph} {status}");
                if agent.state == AgentState::Running
                    && let Some(note) = agent.progress_note()
                {
                    status.push_str(&format!(" · {note}"));
                }
                (agent.label.clone(), status, tone)
            }
            NodeKey::Subagent { key, id } => {
                let Some(row) = state
                    .progress
                    .agent(key)
                    .and_then(|agent| agent.subagents.get(id))
                else {
                    return Self {
                        label: String::new(),
                        status: String::new(),
                        tone: Muted,
                    };
                };
                let stopping = state.subagent_stopping(key, id);
                let (glyph, status) = subagent_status(&row.state, stopping);
                let tone = if stopping {
                    Running
                } else {
                    match row.state {
                        RowState::Completed => Success,
                        RowState::Running => Running,
                        RowState::Failed(_) => Failure,
                        RowState::Interrupted | RowState::Gone => Warning,
                        RowState::Queued => Muted,
                    }
                };
                (row.goal.clone(), format!("{glyph} {status}"), tone)
            }
        };
        Self {
            label,
            status,
            tone,
        }
    }
}
