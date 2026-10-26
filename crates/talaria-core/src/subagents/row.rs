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
        todo!("#242")
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
    /// Saw `spawn_requested` or `start` in this process.
    pub seen_live: bool,
}
