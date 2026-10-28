use crate::engine::{
    AgentBackend, AgentOutcome, AgentRequest, BudgetSnapshot, CallId, Effort, EngineConfig,
    EngineEvent, Isolation, RunOutcomeKind,
};
use futures_util::future::BoxFuture;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{marker::PhantomData, path::Path, sync::Arc, time::Duration};

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct RunId(String);
impl RunId {
    pub fn new() -> RunId {
        todo!()
    }
    pub fn parse(_s: &str) -> Result<RunId, JournalError> {
        todo!()
    }
    pub fn as_str(&self) -> &str {
        todo!()
    }
}
impl Default for RunId {
    fn default() -> Self {
        todo!()
    }
}
#[derive(Clone)]
pub struct RunDir;
impl RunDir {
    pub fn create(
        _parent: &Path,
        _manifest: NewManifest,
        _script_source: &str,
    ) -> Result<RunDir, JournalError> {
        todo!()
    }
    #[doc(hidden)]
    pub fn create_with_id(
        _parent: &Path,
        _id: RunId,
        _manifest: NewManifest,
        _script_source: &str,
    ) -> Result<RunDir, JournalError> {
        todo!()
    }
    pub fn open(_path: &Path) -> Result<RunDir, JournalError> {
        todo!()
    }
    pub fn run_id(&self) -> &RunId {
        todo!()
    }
    pub fn path(&self) -> &Path {
        todo!()
    }
    pub fn manifest(&self) -> &Manifest {
        todo!()
    }
    pub fn store_script(&self, _source: &str) -> Result<String, JournalError> {
        todo!()
    }
    pub fn request_resolution(
        &self,
        _key: &str,
        _attempt: u32,
        _episode: u32,
        _r: Resolution,
    ) -> Result<(), JournalError> {
        todo!()
    }
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct NewManifest {
    pub workflow_name: String,
    pub args: Option<Value>,
    pub started_at_ms: i64,
    pub seed: u64,
    pub profile: String,
    pub on_unknown: OnUnknown,
    pub talaria_version: String,
    pub hermes: Value,
    pub roster: Value,
    pub approval_policy: Value,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Manifest {
    pub v: u32,
    pub run_id: RunId,
    pub created_at_ms: i64,
    pub workflow: WorkflowManifest,
    pub args: Option<Value>,
    pub started_at_ms: i64,
    pub seed: u64,
    pub profile: String,
    pub on_unknown: OnUnknown,
    pub talaria_version: String,
    pub hermes: Value,
    pub roster: Value,
    pub approval_policy: Value,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct WorkflowManifest {
    pub name: String,
    pub script_sha256: String,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OnUnknown {
    Wait,
    Fail,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Resolution {
    Fail,
    Rerun,
    Abort,
}

pub struct RunJournal;
impl Drop for RunJournal {
    fn drop(&mut self) {}
}
impl RunJournal {
    pub async fn open(
        _dir: RunDir,
        _script_source: &str,
    ) -> Result<(RunJournal, ReplaySummary, Vec<JournalWarning>), JournalError> {
        todo!()
    }
    pub fn dir(&self) -> &RunDir {
        todo!()
    }
    #[cfg(feature = "fake-gateway")]
    #[doc(hidden)]
    pub fn observe_appends(
        &self,
        _observer: impl Fn(&Record) -> BoxFuture<'static, ()> + Send + Sync + 'static,
    ) {
        todo!()
    }
    #[cfg(feature = "fake-gateway")]
    #[doc(hidden)]
    pub fn stop_resolution_poller(&self) {
        todo!()
    }
    pub fn engine_config(
        &self,
        _resolver: &dyn LaunchResolver,
    ) -> Result<EngineConfig, JournalError> {
        todo!()
    }
    pub async fn record_engine_event(&self, _e: &EngineEvent) -> Result<(), JournalError> {
        todo!()
    }
    pub fn forward_events(
        self: &Arc<Self>,
        _rx: tokio::sync::mpsc::UnboundedReceiver<EngineEvent>,
    ) -> tokio::task::JoinHandle<Result<(), JournalError>> {
        todo!()
    }
    pub async fn started_durable(&self, _call: &CallId) -> Result<(), JournalError> {
        todo!()
    }
    pub async fn finish(&self, _outcome: RunOutcomeKind) -> Result<(), JournalError> {
        todo!()
    }
    pub fn notices(&self) -> tokio::sync::broadcast::Receiver<JournalNotice> {
        todo!()
    }
    pub fn summary(&self) -> ReplaySummary {
        todo!()
    }
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Default)]
pub struct ReplaySummary {
    pub resume_count: u32,
    pub finished: usize,
    pub in_flight: usize,
    pub parked: Vec<ParkedCall>,
    pub orphans: Vec<OrphanCall>,
    pub running_orphans: Vec<OrphanCall>,
    pub settled_usage: Vec<TurnUsage>,
    pub unsettled: Vec<UnsettledTurn>,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ParkedCall {
    pub key: String,
    pub label: String,
    pub attempt: u32,
    pub episode: u32,
    pub cause: AttentionCause,
    pub turn: Option<u32>,
    pub session_id: Option<String>,
    pub admission_id: Option<String>,
    pub execution_generation: Option<i64>,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct OrphanCall {
    pub key: String,
    pub label: String,
    pub attempt: u32,
    pub last_record: String,
    pub session_id: Option<String>,
    pub admission_id: Option<String>,
    pub last_status: Option<AdmissionState>,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TurnUsage {
    pub key: String,
    pub attempt: u32,
    pub turn: u32,
    pub admission_id: String,
    pub outcome: String,
    pub usage: Option<Value>,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct UnsettledTurn {
    pub key: String,
    pub attempt: u32,
    pub turn: u32,
    pub admission_id: String,
    pub last_status: AdmissionState,
}
#[derive(Clone, Debug, PartialEq)]
pub enum JournalNotice {
    NeedsOperator {
        key: String,
        label: String,
        attempt: u32,
        episode: u32,
        cause: AttentionCause,
        session_id: Option<String>,
        admission_id: Option<String>,
    },
    Resolved {
        key: String,
        attempt: u32,
        episode: u32,
        resolution: Resolution,
    },
    TurnSettled {
        key: String,
        attempt: u32,
        turn: u32,
        admission_id: String,
        usage: Option<Value>,
    },
    Warning(JournalWarning),
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AttentionCause {
    Unknown,
    Lost,
    IdentityConflict,
}
#[derive(Debug, thiserror::Error)]
pub enum JournalError {
    #[error("run {run_id} is already open in another runner")]
    Locked { run_id: String },
    #[error("run {run_id} already exists")]
    Exists { run_id: String },
    #[error("corrupt journal at line {line}: {reason}")]
    Corrupt { line: u64, reason: String },
    #[error("journal closed")]
    Closed,
    #[error("journal record too large: {t} ({bytes})")]
    RecordTooLarge { t: String, bytes: u64 },
    #[error("resolution pending: {file}")]
    ResolutionPending { file: String },
    #[error("journal I/O: {0}")]
    Io(#[from] std::io::Error),
    #[error("manifest: {0}")]
    Manifest(String),
    #[error("bad run id: {0}")]
    BadRunId(String),
}
#[derive(Clone, Debug, PartialEq)]
pub enum JournalWarning {
    TornTail { line: u64, bytes: u64 },
    StaleResolution { key: String },
    BadResolution { file: String, reason: String },
}

pub struct JournalReader;
impl JournalReader {
    pub fn read_from(_path: &Path, _offset: u64) -> Result<(Vec<RecordLine>, u64), JournalError> {
        todo!()
    }
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct RecordLine {
    pub v: u32,
    pub seq: u64,
    pub at_ms: i64,
    #[serde(flatten)]
    pub record: Record,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct BlobRef {
    pub sha256: String,
    pub bytes: u64,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct FinishSummary {
    pub completed: usize,
    pub failed: usize,
    pub skipped: usize,
    pub parked: usize,
    pub unfinished: usize,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum CallOutcome {
    Completed {
        #[serde(skip_serializing_if = "Option::is_none")]
        value: Option<Value>,
        #[serde(skip_serializing_if = "Option::is_none")]
        value_ref: Option<BlobRef>,
    },
    Failed {
        reason: String,
    },
    Skipped,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ResolutionSource {
    Operator,
    Policy,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "t", rename_all = "snake_case")]
pub enum Record {
    RunStarted {
        run_id: String,
        script_sha256: String,
    },
    RunResumed {
        script_sha256: String,
        resume_count: u32,
    },
    Engine {
        #[serde(skip_serializing_if = "Option::is_none")]
        event: Option<EngineEvent>,
        #[serde(skip_serializing_if = "Option::is_none")]
        event_ref: Option<BlobRef>,
    },
    CallIntent {
        key: String,
        attempt: u32,
        call_id: CallId,
        label: String,
        phase: Option<String>,
        workflow_path: Vec<String>,
        launch: ResolvedLaunch,
        request_id: String,
    },
    SessionBound {
        key: String,
        attempt: u32,
        session_id: String,
        stored_session_id: String,
    },
    TurnIntent {
        key: String,
        attempt: u32,
        turn: u32,
        submission_id: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        text: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        text_ref: Option<BlobRef>,
    },
    TurnAdmitted {
        key: String,
        attempt: u32,
        turn: u32,
        admission_id: String,
        status: AdmissionState,
        outcome: Option<String>,
        execution_generation: Option<i64>,
        authority_epoch: i64,
    },
    TurnSettled {
        key: String,
        attempt: u32,
        turn: u32,
        admission_id: String,
        outcome: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        final_text: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        final_text_ref: Option<BlobRef>,
        result_meta: Value,
        usage: Option<Value>,
    },
    CallAttention {
        key: String,
        attempt: u32,
        episode: u32,
        cause: AttentionCause,
        turn: Option<u32>,
        admission_id: Option<String>,
        execution_generation: Option<i64>,
        detail: String,
    },
    CallResolved {
        key: String,
        attempt: u32,
        episode: u32,
        resolution: Resolution,
        source: ResolutionSource,
    },
    CallFinished {
        key: String,
        attempt: u32,
        outcome: CallOutcome,
        turns: u32,
    },
    RunFinished {
        outcome: RunOutcomeKind,
        summary: FinishSummary,
    },
}
#[doc(hidden)]
pub mod testing {
    use super::{JournalError, Record};
    pub trait JournalFile: Send + 'static {
        fn write_all(&mut self, buf: &[u8]) -> std::io::Result<()>;
        fn sync_data(&mut self) -> std::io::Result<()>;
    }
    impl JournalFile for std::fs::File {
        fn write_all(&mut self, _buf: &[u8]) -> std::io::Result<()> {
            todo!()
        }
        fn sync_data(&mut self) -> std::io::Result<()> {
            todo!()
        }
    }
    pub struct Writer;
    impl Writer {
        pub fn spawn<F: JournalFile>(_file: F, _next_seq: u64) -> Writer {
            todo!()
        }
        pub async fn append(&self, _record: Record) -> Result<u64, JournalError> {
            todo!()
        }
    }
}
pub fn journal_key(_call: &CallId, _launch: &ResolvedLaunch) -> String {
    todo!()
}
pub fn request_id(_run: &RunId, _key: &str, _attempt: u32) -> String {
    todo!()
}
pub fn submission_id(_run: &RunId, _key: &str, _attempt: u32, _turn: u32) -> String {
    todo!()
}

pub trait GatewayPort: Send + Sync + 'static {
    fn create_session(
        &self,
        req: CreateSession,
    ) -> BoxFuture<'static, Result<SessionBound, PortError>>;
    fn attach(
        &self,
        profile: String,
        session_id: String,
    ) -> BoxFuture<'static, Result<(), PortError>>;
    fn submit(&self, req: SubmitTurn) -> BoxFuture<'static, Result<Receipt, PortError>>;
    fn receipt(
        &self,
        profile: String,
        session_id: String,
        admission_id: String,
    ) -> BoxFuture<'static, Result<Receipt, PortError>>;
    fn wait_settled(
        &self,
        profile: String,
        session_id: String,
        admission_id: String,
    ) -> BoxFuture<'static, Result<Receipt, PortError>>;
    fn resolve_unknown(
        &self,
        profile: String,
        session_id: String,
        admission_id: String,
        execution_generation: i64,
    ) -> BoxFuture<'static, Result<Receipt, PortError>>;
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ResolvedLaunch {
    pub profile: String,
    pub model: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provider: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub effort: Option<Effort>,
    pub toolsets: Vec<String>,
    pub skills: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_turns: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub isolation: Option<Isolation>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cwd: Option<String>,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CreateSession {
    pub request_id: String,
    pub launch: ResolvedLaunch,
    pub label: String,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SessionBound {
    pub session_id: String,
    pub stored_session_id: String,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SubmitTurn {
    pub profile: String,
    pub session_id: String,
    pub submission_id: String,
    pub text: String,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Receipt {
    pub admission_id: String,
    pub session_id: String,
    pub status: AdmissionState,
    pub outcome: Option<String>,
    pub authority_epoch: i64,
    pub execution_generation: Option<i64>,
    pub result: Option<Value>,
    pub usage: Option<Value>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AdmissionState {
    Queued,
    Started,
    Unknown,
    Terminal,
}
#[derive(Clone, Debug, PartialEq)]
pub enum PortError {
    Transport(String),
    Timeout,
    Refused {
        reason: String,
        field: Option<String>,
        detail: Option<String>,
    },
}
pub trait TurnPolicy: Send + Sync + 'static {
    fn first_turn(&self, request: &AgentRequest, launch: &ResolvedLaunch) -> String;
    fn after_turn(&self, request: &AgentRequest, turns: &[SettledTurn]) -> NextStep;
}
#[derive(Clone, Debug, PartialEq)]
pub struct SettledTurn {
    pub turn: u32,
    pub outcome: String,
    pub final_text: Option<String>,
    pub result_meta: Value,
    pub usage: Option<Value>,
}
#[derive(Clone, Debug, PartialEq)]
pub enum NextStep {
    Finish(AgentOutcome),
    Turn { text: String },
}
pub const MAX_TURNS_PER_CALL: u32 = 2;
pub trait LaunchResolver: Send + Sync + 'static {
    fn resolve(&self, request: &AgentRequest) -> Result<ResolvedLaunch, String>;
}
pub trait AdmissionGate: Send + Sync + 'static {
    fn acquire(
        &self,
        request: &AgentRequest,
        launch: &ResolvedLaunch,
        reason: GateReason,
    ) -> BoxFuture<'static, GateDecision>;
    fn budget(&self) -> BudgetSnapshot;
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GateReason {
    Fresh,
    Reattach,
}
pub enum GateDecision {
    Proceed(GatePermit),
    Skip,
    Abort { reason: String },
}
pub struct GatePermit;
impl Drop for GatePermit {
    fn drop(&mut self) {}
}
impl GatePermit {
    pub fn new(_release: impl FnOnce() + Send + 'static) -> GatePermit {
        todo!()
    }
    pub fn noop() -> GatePermit {
        todo!()
    }
}
pub struct NoGate;
impl AdmissionGate for NoGate {
    fn acquire(
        &self,
        _request: &AgentRequest,
        _launch: &ResolvedLaunch,
        _reason: GateReason,
    ) -> BoxFuture<'static, GateDecision> {
        todo!()
    }
    fn budget(&self) -> BudgetSnapshot {
        todo!()
    }
}
pub struct JournaledBackend<P, T, L, G>(PhantomData<(P, T, L, G)>);
impl<P: GatewayPort, T: TurnPolicy, L: LaunchResolver, G: AdmissionGate>
    JournaledBackend<P, T, L, G>
{
    pub fn new(
        _journal: Arc<RunJournal>,
        _port: P,
        _policy: T,
        _resolver: L,
        _gate: G,
        _retry: RetryPolicy,
    ) -> Self {
        todo!()
    }
}
impl<P: GatewayPort, T: TurnPolicy, L: LaunchResolver, G: AdmissionGate> AgentBackend
    for JournaledBackend<P, T, L, G>
{
    fn run_agent(&self, _request: AgentRequest) -> BoxFuture<'static, AgentOutcome> {
        todo!()
    }
    fn budget(&self) -> BudgetSnapshot {
        todo!()
    }
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RetryPolicy {
    pub max_outage: Duration,
}
impl Default for RetryPolicy {
    fn default() -> Self {
        todo!()
    }
}

#[cfg(feature = "fake-gateway")]
pub mod fake_gateway;
