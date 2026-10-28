use super::blobs::BlobRef;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet, VecDeque};
use talaria_core::subagents::SubagentTree;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentState {
    #[default]
    Queued,
    Running,
    Waiting,
    Unknown,
    Ended,
    Stopping,
    Done,
    Failed,
    Skipped,
}
impl AgentState {
    pub fn terminal(self) -> bool {
        matches!(self, Self::Done | Self::Failed | Self::Skipped)
    }
    pub fn needs_you(self) -> bool {
        matches!(self, Self::Unknown | Self::Waiting)
    }
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Queued => "queued",
            Self::Running => "running",
            Self::Waiting => "waiting",
            Self::Unknown => "unknown",
            Self::Ended => "ended",
            Self::Stopping => "stopping",
            Self::Done => "done",
            Self::Failed => "failed",
            Self::Skipped => "skipped",
        }
    }
    pub fn glyph(self) -> char {
        match self {
            Self::Done => '✓',
            Self::Failed => '✗',
            Self::Skipped => '—',
            Self::Running => '◐',
            Self::Unknown | Self::Waiting => '●',
            Self::Queued => '○',
            Self::Stopping => '◌',
            Self::Ended => '?',
        }
    }
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RunState {
    #[default]
    Running,
    Completed,
    Failed,
    Aborted,
}
impl RunState {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Running => "running",
            Self::Completed => "completed",
            Self::Failed => "failed",
            Self::Aborted => "aborted",
        }
    }
}
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct Counts {
    pub total: usize,
    pub done: usize,
    pub running: usize,
    pub needs_you: usize,
    pub failed: usize,
    pub queued: usize,
    pub skipped: usize,
    pub ended: usize,
}
impl Counts {
    pub fn add(&mut self, state: AgentState) {
        self.total += 1;
        match state {
            AgentState::Done => self.done += 1,
            AgentState::Running | AgentState::Stopping => self.running += 1,
            AgentState::Unknown | AgentState::Waiting => self.needs_you += 1,
            AgentState::Failed => self.failed += 1,
            AgentState::Queued => self.queued += 1,
            AgentState::Skipped => self.skipped += 1,
            AgentState::Ended => self.ended += 1,
        }
    }
}
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub struct RunNode {
    pub name: String,
    pub state: RunState,
    pub previous_execution: Option<RunState>,
    pub started_at_ms: Option<i64>,
    pub finished_at_ms: Option<i64>,
    pub counts: Counts,
    pub tokens: u64,
    pub tokens_unknown: bool,
    pub money: Option<f64>,
    pub unpriced: usize,
    pub logs: VecDeque<String>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PhaseNode {
    pub title: String,
    pub workflow_path: Vec<String>,
    pub agents: Vec<String>,
    pub counts: Counts,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChildNode {
    pub workflow_path: Vec<String>,
    pub name: String,
}
pub struct AgentNode {
    pub key: String,
    pub(crate) order: u64,
    pub label: String,
    pub phase: Option<String>,
    pub workflow_path: Vec<String>,
    pub call_id: Option<crate::engine::CallId>,
    pub model: Option<String>,
    pub provider: Option<String>,
    pub effort: Option<String>,
    pub state: AgentState,
    pub attempt: u32,
    pub turn: u32,
    pub session_id: Option<String>,
    pub stored_session_id: Option<String>,
    pub admission_id: Option<String>,
    pub admission_status: Option<String>,
    pub execution_generation: Option<i64>,
    pub started_at_ms: i64,
    pub finished_at_ms: Option<i64>,
    pub settled_tokens: u64,
    pub tokens: u64,
    pub tokens_unknown: bool,
    pub money: Option<f64>,
    pub last_outcome: Option<String>,
    pub attention: Option<String>,
    pub last_tool: Option<String>,
    pub prompt: Option<String>,
    pub prompt_ref: Option<BlobRef>,
    pub tail: VecDeque<String>,
    pub subagents: SubagentTree,
    pub restart_requested: bool,
    pub stopped: bool,
    pub controls_disabled: bool,
    pub(crate) timeout_seq: Option<u64>,
    pub(crate) ended: bool,
    pub(crate) live_active: bool,
    pub(crate) stopping: bool,
    pub(crate) prompts: HashSet<(String, String, i64)>,
    pub(crate) settlements: HashSet<String>,
    pub(crate) settled_sessions: HashMap<String, u64>,
    pub(crate) live_sessions: HashMap<String, u64>,
}
impl AgentNode {
    pub(crate) fn new(key: String, at_ms: i64) -> Self {
        Self {
            label: key.clone(),
            key,
            order: u64::MAX,
            phase: None,
            workflow_path: vec![],
            call_id: None,
            model: None,
            provider: None,
            effort: None,
            state: AgentState::Queued,
            attempt: 0,
            turn: 0,
            session_id: None,
            stored_session_id: None,
            admission_id: None,
            admission_status: None,
            execution_generation: None,
            started_at_ms: at_ms,
            finished_at_ms: None,
            settled_tokens: 0,
            tokens: 0,
            tokens_unknown: false,
            money: None,
            last_outcome: None,
            attention: None,
            last_tool: None,
            prompt: None,
            prompt_ref: None,
            tail: VecDeque::new(),
            subagents: SubagentTree::new(),
            restart_requested: false,
            stopped: false,
            controls_disabled: false,
            timeout_seq: None,
            ended: false,
            live_active: false,
            stopping: false,
            prompts: HashSet::new(),
            settlements: HashSet::new(),
            settled_sessions: HashMap::new(),
            live_sessions: HashMap::new(),
        }
    }
    pub fn duration_ms(&self, now_ms: i64) -> i64 {
        self.finished_at_ms
            .unwrap_or(now_ms)
            .saturating_sub(self.started_at_ms)
            .max(0)
    }
    pub(crate) fn derive(&mut self) {
        self.state = match self.last_outcome.as_deref() {
            Some("completed") => AgentState::Done,
            Some("failed" | "interrupted" | "stopped" | "cancelled") => AgentState::Failed,
            Some("skipped") => AgentState::Skipped,
            _ if self.attention.is_some() => AgentState::Unknown,
            _ if self.ended => AgentState::Ended,
            _ if !self.prompts.is_empty() => AgentState::Waiting,
            _ if self.stopping => AgentState::Stopping,
            _ if self.live_active || self.admission_status.as_deref() == Some("started") => {
                AgentState::Running
            }
            _ => AgentState::Queued,
        };
        self.tokens = self.settled_tokens.saturating_add(
            self.live_sessions
                .iter()
                .map(|(s, n)| n.saturating_sub(*self.settled_sessions.get(s).unwrap_or(&0)))
                .sum::<u64>(),
        );
    }
    pub(crate) fn push_tail(&mut self, text: &str) {
        for line in text.lines().filter(|s| !s.is_empty()) {
            self.tail.push_back(line.chars().take(200).collect());
            while self.tail.len() > 4 {
                self.tail.pop_front();
            }
        }
    }
}
pub fn format_tokens(n: u64) -> String {
    if n < 1000 {
        n.to_string()
    } else {
        let tenths = (u128::from(n) * 10 + 500) / 1000;
        format!("{}.{}k", tenths / 10, tenths % 10)
    }
}
pub fn format_money(n: Option<f64>) -> String {
    n.map_or_else(|| "cost unknown".into(), |n| format!("${n:.2}"))
}
