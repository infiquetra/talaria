use futures_util::future::BoxFuture;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{sync::Arc, time::Duration};
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
    #[error("meta missing at line {line}")]
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
pub fn parse_script(_source: &str) -> Result<ParsedScript, ScriptError> {
    todo!()
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
    pub fn new(_run_id: impl Into<String>, _started_at_ms: i64, _seed: u64) -> Self {
        todo!()
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
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum AgentOutcome {
    Completed(Value),
    Failed { reason: String },
    Skipped,
    Abort { reason: String },
}
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct BudgetSnapshot {
    pub tokens_total: Option<u64>,
    pub tokens_spent: u64,
    pub money_total: Option<f64>,
    pub money_spent: f64,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum WorkflowRef {
    Name(String),
    ScriptPath(String),
}
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum ResolveError {
    #[error("not found")]
    NotFound,
    #[error("unreadable: {0}")]
    Unreadable(String),
}
pub trait WorkflowResolver: Send + Sync + 'static {
    fn resolve(&self, r: &WorkflowRef) -> Result<String, ResolveError>;
}
#[derive(Clone, Debug, Default)]
pub struct MapResolver;
impl MapResolver {
    pub fn new() -> Self {
        todo!()
    }
    pub fn with_name(self, _name: &str, _source: &str) -> Self {
        todo!()
    }
    pub fn with_path(self, _script_path: &str, _source: &str) -> Self {
        todo!()
    }
}
impl WorkflowResolver for MapResolver {
    fn resolve(&self, _r: &WorkflowRef) -> Result<String, ResolveError> {
        todo!()
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
pub async fn run(
    _parsed: ParsedScript,
    _config: EngineConfig,
    _backend: Arc<dyn AgentBackend>,
    _resolver: Arc<dyn WorkflowResolver>,
    _events: mpsc::UnboundedSender<EngineEvent>,
    _cancel: CancellationToken,
) -> RunOutcome {
    todo!()
}
pub struct RunHandle {
    pub events: mpsc::UnboundedReceiver<EngineEvent>,
    pub cancel: CancellationToken,
    pub outcome: oneshot::Receiver<RunOutcome>,
}
pub fn spawn(
    _parsed: ParsedScript,
    _config: EngineConfig,
    _backend: Arc<dyn AgentBackend>,
    _resolver: Arc<dyn WorkflowResolver>,
) -> RunHandle {
    todo!()
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
    Failed { reason: String },
    Skipped,
    Aborted,
    Abandoned,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RunOutcomeKind {
    Completed,
    Failed,
    Aborted,
}
