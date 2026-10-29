//! One row of the subagent tree. Issue #242.

use std::collections::VecDeque;

/// How a row failed: the `failed`, `error` and `timeout` status words.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FailKind {
    Failed,
    Error,
    Timeout,
}

/// A row's lifecycle state.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RowState {
    Queued,
    Running,
    Completed,
    Failed(FailKind),
    Interrupted,
    Gone,
}

impl RowState {
    /// True for the four states a row never leaves.
    #[must_use]
    pub const fn is_terminal(self) -> bool {
        matches!(
            self,
            Self::Completed | Self::Failed(_) | Self::Interrupted | Self::Gone
        )
    }

    /// True while the child may still act.
    #[must_use]
    pub const fn is_live(self) -> bool {
        matches!(self, Self::Queued | Self::Running)
    }

    /// The one-column glyph the dock, panel and block draw for this state.
    #[must_use]
    pub fn glyph(self) -> char {
        match self {
            Self::Queued => '○',
            Self::Running => '◐',
            Self::Completed => '✓',
            Self::Failed(_) => '✗',
            Self::Interrupted => '■',
            Self::Gone => '?',
        }
    }
}

/// What one activity line reports.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ActivityKind {
    Tool,
    Progress,
    Thinking,
    Summary,
}

/// One activity line: newest last, at most 24 per row.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Activity {
    pub kind: ActivityKind,
    pub text: String,
    pub is_error: bool,
}

/// Everything the tree knows about one child.
#[derive(Clone, Debug, PartialEq)]
pub struct SubagentRow {
    pub id: String,
    pub parent_id: Option<String>,
    pub depth: Option<i64>,
    pub goal: String,
    pub delegation_id: Option<String>,
    pub child_session_id: Option<String>,
    pub model: Option<String>,
    pub task_index: i64,
    pub task_count: i64,
    pub state: RowState,
    pub stop_requested: bool,
    pub started_at: Option<f64>,
    pub first_seen_ms: u64,
    pub first_seen_s: f64,
    pub last_event_ms: u64,
    pub duration_s: Option<f64>,
    pub tool_count: Option<i64>,
    pub last_tool: Option<String>,
    pub toolsets: Vec<String>,
    pub input_tokens: Option<i64>,
    pub output_tokens: Option<i64>,
    pub reasoning_tokens: Option<i64>,
    pub api_calls: Option<i64>,
    pub files_read: Vec<String>,
    pub files_written: Vec<String>,
    pub summary: Option<String>,
    pub accepting_steer: Option<bool>,
    /// Newest last, at most 24.
    pub activity: VecDeque<Activity>,
    /// Saw a spawn, start, or completion for a child not discovered by a list.
    pub seen_live: bool,
}

pub(super) fn cut(text: &str, limit: usize) -> String {
    if text.chars().count() <= limit {
        return text.to_owned();
    }
    if limit == 0 {
        return String::new();
    }
    text.chars().take(limit - 1).chain(['…']).collect()
}

fn collapsed(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

pub(super) fn merge_string(to: &mut Option<String>, from: &Option<String>) {
    if let Some(value) = from.as_ref().filter(|s| !s.is_empty()) {
        *to = Some(value.clone());
    }
}

pub(super) fn merge_count(to: &mut Option<i64>, from: Option<i64>) {
    if let Some(value) = from {
        *to = Some(to.map_or(value, |old| old.max(value)));
    }
}

pub(super) fn terminal(status: Option<hermes_contract::schemas::SubagentStatus>) -> RowState {
    use hermes_contract::schemas::SubagentStatus as S;
    match status {
        Some(S::Completed) => RowState::Completed,
        Some(S::Interrupted) => RowState::Interrupted,
        Some(S::Error) => RowState::Failed(FailKind::Error),
        Some(S::Timeout) => RowState::Failed(FailKind::Timeout),
        _ => RowState::Failed(FailKind::Failed),
    }
}

pub(super) fn listed_terminal(status: Option<hermes_contract::schemas::SubagentStatus>) -> bool {
    use hermes_contract::schemas::SubagentStatus as S;
    matches!(
        status,
        Some(S::Completed | S::Failed | S::Error | S::Timeout | S::Interrupted)
    )
}

impl SubagentRow {
    pub(super) fn new(id: String, now: super::Clock, state: RowState) -> Self {
        Self {
            id,
            parent_id: None,
            depth: None,
            goal: String::new(),
            delegation_id: None,
            child_session_id: None,
            model: None,
            task_index: 0,
            task_count: 1,
            state,
            stop_requested: false,
            started_at: None,
            first_seen_ms: now.mono_ms,
            first_seen_s: now.wall_s,
            last_event_ms: now.mono_ms,
            duration_s: None,
            tool_count: None,
            last_tool: None,
            toolsets: Vec::new(),
            input_tokens: None,
            output_tokens: None,
            reasoning_tokens: None,
            api_calls: None,
            files_read: Vec::new(),
            files_written: Vec::new(),
            summary: None,
            accepting_steer: None,
            activity: VecDeque::new(),
            seen_live: false,
        }
    }

    pub(super) fn push_activity(&mut self, kind: ActivityKind, text: String, is_error: bool) {
        if text.is_empty() {
            return;
        }
        let activity = Activity {
            kind,
            text,
            is_error,
        };
        if self.activity.back() == Some(&activity) {
            return;
        }
        self.activity.push_back(activity);
        if self.activity.len() > super::tree::MAX_ACTIVITY {
            self.activity.pop_front();
        }
    }

    fn tool_activity(&mut self, tool: &str, preview: &str, is_error: bool) {
        let preview = cut(&collapsed(preview), 96);
        let text = if preview.is_empty() {
            tool.to_owned()
        } else {
            format!("{tool}(\"{preview}\")")
        };
        self.push_activity(ActivityKind::Tool, text, is_error);
    }

    pub(super) fn merge_event(&mut self, event: &super::SubagentEvent) {
        use super::SubagentEventKind as K;
        let p = &event.payload;
        if !p.goal.is_empty() {
            self.goal.clone_from(&p.goal);
        }
        merge_string(&mut self.parent_id, &p.parent_id);
        merge_string(&mut self.delegation_id, &p.delegation_id);
        merge_string(&mut self.child_session_id, &p.child_session_id);
        merge_string(&mut self.model, &p.model);
        if p.depth.is_some() {
            self.depth = p.depth;
        }
        self.task_index = p.task_index;
        self.task_count = p.task_count;
        merge_count(&mut self.tool_count, p.tool_count);
        if p.input_tokens.is_some() {
            self.input_tokens = p.input_tokens;
        }
        if p.output_tokens.is_some() {
            self.output_tokens = p.output_tokens;
        }
        if p.reasoning_tokens.is_some() {
            self.reasoning_tokens = p.reasoning_tokens;
        }
        if p.api_calls.is_some() {
            self.api_calls = p.api_calls;
        }
        if p.summary.is_some() {
            self.summary.clone_from(&p.summary);
        }
        if let Some(value) = p.duration_seconds {
            self.duration_s = Some(value);
        }
        if let Some(value) = &p.files_read {
            self.files_read.clone_from(value);
        }
        if let Some(value) = &p.files_written {
            self.files_written.clone_from(value);
        }
        if let Some(value) = &p.toolsets {
            self.toolsets.clone_from(value);
        }
        if let Some(tail) = &p.output_tail {
            for item in tail {
                self.tool_activity(&item.tool, &item.preview, item.is_error);
            }
        }
        match event.kind {
            K::Tool => {
                let tool = p.tool_name.as_deref().unwrap_or_default();
                if !tool.is_empty() {
                    self.last_tool = Some(tool.to_owned());
                    let preview = p
                        .tool_preview
                        .as_deref()
                        .filter(|s| !s.is_empty())
                        .or(p.text.as_deref())
                        .unwrap_or_default();
                    self.tool_activity(tool, preview, false);
                }
            }
            K::Progress | K::Thinking => {
                let kind = if event.kind == K::Progress {
                    ActivityKind::Progress
                } else {
                    ActivityKind::Thinking
                };
                self.push_activity(
                    kind,
                    cut(&collapsed(p.text.as_deref().unwrap_or_default()), 220),
                    false,
                );
            }
            K::Complete => {
                let fallback = if self.state == RowState::Failed(FailKind::Timeout) {
                    p.duration_seconds
                        .map(|s| format!("Timed out after {s}s"))
                        .unwrap_or_default()
                } else {
                    String::new()
                };
                let summary = p
                    .summary
                    .as_deref()
                    .filter(|s| !s.is_empty())
                    .or(p.text.as_deref().filter(|s| !s.is_empty()))
                    .unwrap_or(&fallback);
                self.push_activity(
                    ActivityKind::Summary,
                    cut(&collapsed(summary), 220),
                    matches!(self.state, RowState::Failed(_)),
                );
            }
            _ => {}
        }
    }

    pub(super) fn merge_snapshot(&mut self, p: &hermes_contract::schemas::SubagentSnapshot) {
        if let Some(goal) = p.goal.as_ref().filter(|s| !s.is_empty()) {
            self.goal.clone_from(goal);
        }
        merge_string(&mut self.parent_id, &p.parent_id);
        merge_string(&mut self.delegation_id, &p.delegation_id);
        merge_string(&mut self.model, &p.model);
        merge_string(&mut self.last_tool, &p.last_tool);
        if p.depth.is_some() {
            self.depth = p.depth;
        }
        if p.started_at.is_some() {
            self.started_at = p.started_at;
        }
        merge_count(&mut self.tool_count, p.tool_count);
        self.accepting_steer = p.accepting_steer;
        if self.activity.is_empty()
            && let Some(tool) = self.last_tool.clone()
        {
            self.push_activity(ActivityKind::Tool, tool, false);
        }
    }
}
