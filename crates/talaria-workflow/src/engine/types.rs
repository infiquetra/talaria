use std::{collections::HashMap, sync::Arc, time::Duration};

use futures::future::BoxFuture;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::sync::{mpsc, oneshot};
use tokio_util::sync::CancellationToken;

#[derive(Clone, Debug, PartialEq)]
pub struct ParsedScript {
    pub meta: WorkflowMeta,
    pub body: String,
    pub warnings: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct WorkflowMeta {
    pub name: String,
    pub description: String,
    pub when_to_use: Option<String>,
    pub phases: Vec<PhaseMeta>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PhaseMeta {
    pub title: String,
    pub detail: Option<String>,
    pub model: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum ScriptError {
    #[error("meta header missing at line {line}")]
    MetaMissing { line: u32 },
    #[error("meta is not a literal: {found} at line {line}, column {column}")]
    MetaNotLiteral {
        line: u32,
        column: u32,
        found: String,
    },
    #[error("meta field {field}: {problem}")]
    MetaField { field: String, problem: String },
    #[error("import is not allowed at line {line}, column {column}")]
    ImportNotAllowed { line: u32, column: u32 },
}

#[derive(Clone, Debug)]
pub struct EngineConfig {
    pub run_id: String,
    pub args: Option<Value>,
    pub started_at_ms: i64,
    pub seed: u64,
    pub max_agents: u32,
    pub max_items_per_call: usize,
    pub memory_limit_bytes: usize,
    pub max_stack_bytes: usize,
    pub max_sync_slice: Duration,
    pub replay: Option<ReplayLog>,
}

impl EngineConfig {
    pub fn new(run_id: impl Into<String>, started_at_ms: i64, seed: u64) -> Self {
        Self {
            run_id: run_id.into(),
            args: None,
            started_at_ms,
            seed,
            max_agents: 1000,
            max_items_per_call: 4096,
            memory_limit_bytes: 256 * 1024 * 1024,
            max_stack_bytes: 1024 * 1024,
            max_sync_slice: Duration::from_secs(5),
            replay: None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ReplayLog {
    pub entries: Vec<ReplayEntry>,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ReplayEntry {
    pub call_id: CallId,
    pub outcome: AgentOutcome,
}

pub trait AgentBackend: Send + Sync + 'static {
    fn run_agent(&self, request: AgentRequest) -> BoxFuture<'static, AgentOutcome>;
    fn budget(&self) -> BudgetSnapshot;
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct AgentRequest {
    pub call_id: CallId,
    pub label: String,
    pub phase: Option<String>,
    pub prompt: String,
    pub schema: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub report_file: Option<Box<str>>,
    pub tier: Option<String>,
    pub model: Option<String>,
    pub provider: Option<String>,
    pub effort: Option<Effort>,
    pub profile: Option<String>,
    pub toolsets: Option<Vec<String>>,
    pub skills: Option<Vec<String>>,
    pub isolation: Option<Isolation>,
    pub max_turns: Option<u32>,
    pub workflow_path: Vec<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Effort {
    None,
    Minimal,
    Low,
    Medium,
    High,
    Xhigh,
    Max,
    Ultra,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Isolation {
    Worktree,
}
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct CallId {
    pub digest: String,
    pub occurrence: u32,
}

impl std::fmt::Display for CallId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}#{}",
            self.digest.chars().take(12).collect::<String>(),
            self.occurrence
        )
    }
}

/// A turn Hermes ended at its step cap: the work did not finish, but the agent left a reply.
///
/// The counts come from Hermes's exit reason, `max_iterations_reached(60/60)`; a reason that
/// does not carry them still marks the agent as cut short.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct CutShort {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub steps: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub limit: Option<u32>,
    /// The branch the agent's uncommitted worktree changes were saved to, when it ran with
    /// worktree isolation and left any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub saved_branch: Option<String>,
}
impl CutShort {
    /// The mark for Hermes's `turn_exit_reason`, if it names the step cap.
    pub fn from_exit_reason(reason: &str) -> Option<Self> {
        let counts = reason.strip_prefix("max_iterations_reached(")?;
        let counts = counts.split_once(')').map_or(counts, |(inside, _)| inside);
        let (steps, limit) = counts.split_once('/').unzip();
        let number = |text: Option<&str>| text.and_then(|t| t.trim().parse().ok());
        Some(Self {
            steps: number(steps),
            limit: number(limit),
            saved_branch: None,
        })
    }
    /// The plain words every view uses, such as "cut short (60 of 60 steps)".
    pub fn describe(&self) -> String {
        match (self.steps, self.limit) {
            (Some(steps), Some(limit)) => format!("cut short ({steps} of {limit} steps)"),
            _ => "cut short at its step limit".into(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct RecoveredReport {
    /// The worktree-relative path the script declared.
    pub path: String,
    /// Content address of the original file in the run's blob store.
    pub sha256: String,
    pub bytes: u64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum AgentOutcome {
    Completed(Value),
    /// The agent's reply, kept, from a turn Hermes stopped at its step cap. The script gets the
    /// value as it would for `Completed`, with the mark beside it.
    CutShort {
        value: Value,
        cut_short: CutShort,
    },
    Failed {
        reason: String,
    },
    /// A failed turn with a schema-validated report kept from its worktree.
    FailedWithReport {
        reason: String,
        value: Value,
        report: RecoveredReport,
    },
    Skipped,
    /// Ended because the run's budget ran out; the call rejects with `BudgetExhaustedError`.
    Stopped {
        reason: String,
    },
    Abort {
        reason: String,
    },
}
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct BudgetSnapshot {
    pub tokens_total: Option<u64>,
    pub tokens_spent: u64,
    pub money_total: Option<f64>,
    pub money_spent: f64,
}
/// Why a budget refuses new work, naming what was spent; `None` while work may continue.
pub fn exhausted(b: &BudgetSnapshot) -> Option<String> {
    if let Some(total) = b.tokens_total.filter(|total| b.tokens_spent >= *total) {
        return Some(format!(
            "token budget exhausted (spent {} of {total})",
            b.tokens_spent
        ));
    }
    b.money_total
        .filter(|total| b.money_spent >= *total)
        .map(|total| {
            format!(
                "money budget exhausted (spent {} of {total})",
                b.money_spent
            )
        })
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum WorkflowRef {
    Name(String),
    ScriptPath(String),
}
pub trait WorkflowResolver: Send + Sync + 'static {
    fn resolve(&self, reference: &WorkflowRef) -> Result<String, ResolveError>;
}
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum ResolveError {
    #[error("not found")]
    NotFound,
    #[error("unreadable: {0}")]
    Unreadable(String),
}
#[derive(Clone, Debug, Default)]
pub struct MapResolver {
    names: HashMap<String, String>,
    paths: HashMap<String, String>,
}
impl MapResolver {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn with_name(mut self, name: &str, source: &str) -> Self {
        self.names.insert(name.into(), source.into());
        self
    }
    pub fn with_path(mut self, path: &str, source: &str) -> Self {
        self.paths.insert(path.into(), source.into());
        self
    }
}
impl WorkflowResolver for MapResolver {
    fn resolve(&self, reference: &WorkflowRef) -> Result<String, ResolveError> {
        match reference {
            WorkflowRef::Name(n) => self.names.get(n),
            WorkflowRef::ScriptPath(p) => self.paths.get(p),
        }
        .cloned()
        .ok_or(ResolveError::NotFound)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum RunOutcome {
    Completed { result: Value },
    Failed { error: ScriptFailure },
    Aborted { reason: String },
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ScriptFailure {
    pub name: String,
    pub message: String,
    pub line: Option<u32>,
    pub column: Option<u32>,
    pub stack: Option<String>,
    pub workflow_path: Vec<String>,
}

pub struct RunHandle {
    pub events: mpsc::UnboundedReceiver<EngineEvent>,
    pub cancel: CancellationToken,
    pub outcome: oneshot::Receiver<RunOutcome>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum EngineEvent {
    Started {
        meta: WorkflowMeta,
        warnings: Vec<String>,
    },
    Phase {
        title: String,
        workflow_path: Vec<String>,
    },
    Log {
        message: String,
        workflow_path: Vec<String>,
    },
    AgentStarted {
        request: AgentRequest,
        replayed: bool,
    },
    AgentRefused {
        label: String,
        phase: Option<String>,
        workflow_path: Vec<String>,
        reason: String,
    },
    AgentFinished {
        call_id: CallId,
        completion_seq: Option<u64>,
        outcome: OutcomeKind,
    },
    ChildStarted {
        workflow_path: Vec<String>,
        meta: WorkflowMeta,
        warnings: Vec<String>,
    },
    ChildFinished {
        workflow_path: Vec<String>,
        ok: bool,
    },
    ReplayDiverged {
        at_seq: u64,
    },
    Finished {
        outcome: RunOutcomeKind,
    },
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum OutcomeKind {
    Completed,
    /// Completed with a kept reply, but Hermes stopped the agent at its step cap.
    CutShort {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        steps: Option<u32>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        limit: Option<u32>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        saved_branch: Option<String>,
    },
    Failed {
        reason: String,
    },
    Skipped,
    Stopped {
        reason: String,
    },
    Aborted,
    Abandoned,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RunOutcomeKind {
    Completed,
    /// The script returned, but the budget refused or stopped at least one agent.
    BudgetExhausted,
    /// The script returned, but at least one agent failed.
    CompletedWithFailures,
    Failed,
    Aborted,
}

pub(crate) fn failure(name: &str, message: impl Into<String>) -> RunOutcome {
    RunOutcome::Failed {
        error: ScriptFailure {
            name: name.into(),
            message: message.into(),
            line: None,
            column: None,
            stack: None,
            workflow_path: Vec::new(),
        },
    }
}

// Trait objects stay owned by the runner; callers can share backend implementations.
pub(crate) type Backend = Arc<dyn AgentBackend>;

#[cfg(test)]
mod tests {
    use super::CutShort;

    #[test]
    fn the_step_cap_exit_reason_gives_the_counts() {
        let cut = CutShort::from_exit_reason("max_iterations_reached(12/60)").unwrap();
        assert_eq!((cut.steps, cut.limit), (Some(12), Some(60)));
        assert_eq!(cut.describe(), "cut short (12 of 60 steps)");
    }

    #[test]
    fn an_exit_reason_without_counts_is_still_cut_short() {
        for reason in [
            "max_iterations_reached(",
            "max_iterations_reached(x/y)",
            "max_iterations_reached(5)",
        ] {
            let cut = CutShort::from_exit_reason(reason).unwrap();
            assert_eq!(cut.describe(), "cut short at its step limit", "{reason}");
        }
    }

    #[test]
    fn other_exit_reasons_are_not_cut_short() {
        for reason in [
            "completed",
            "budget_exhausted",
            "max_iterations",
            "error_near_max_iterations(1/2)",
        ] {
            assert_eq!(CutShort::from_exit_reason(reason), None, "{reason}");
        }
    }
}
