//! Narrow, transport-independent ports implemented by the Hermes binding.
use crate::engine::{AgentOutcome, AgentRequest, BudgetSnapshot, Effort, Isolation};
use futures::future::BoxFuture;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{sync::Arc, time::Duration};

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
#[derive(Clone, Debug, PartialEq, thiserror::Error, Serialize, Deserialize)]
pub enum PortError {
    #[error("transport: {0}")]
    Transport(String),
    #[error("gateway timeout")]
    Timeout,
    #[error("{reason}: {detail:?}")]
    Refused {
        reason: String,
        field: Option<String>,
        detail: Option<String>,
    },
}
impl PortError {
    #[cfg(feature = "fake-gateway")]
    pub(crate) fn refused(reason: &str) -> Self {
        Self::Refused {
            reason: reason.into(),
            field: None,
            detail: None,
        }
    }
    pub(crate) fn reason(&self) -> &str {
        match self {
            Self::Refused { reason, .. } => reason,
            _ => "",
        }
    }
    pub(crate) fn retryable(&self) -> bool {
        matches!(self, Self::Transport(_) | Self::Timeout) || self.reason() == "runtime_draining"
    }
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
pub trait TurnPolicy: Send + Sync + 'static {
    fn first_turn(&self, request: &AgentRequest, launch: &ResolvedLaunch) -> String;
    fn after_turn(&self, request: &AgentRequest, turns: &[SettledTurn]) -> NextStep;
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
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
    fn retry_after_failure(
        &self,
        _request: &AgentRequest,
        _launch: &ResolvedLaunch,
        _last: &SettledTurn,
        _attempt: u32,
    ) -> bool {
        false
    }
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
pub struct GatePermit {
    release: Option<Box<dyn FnOnce() + Send>>,
}
impl GatePermit {
    pub fn new(release: impl FnOnce() + Send + 'static) -> Self {
        Self {
            release: Some(Box::new(release)),
        }
    }
    pub fn noop() -> Self {
        Self { release: None }
    }
}
impl Drop for GatePermit {
    fn drop(&mut self) {
        if let Some(release) = self.release.take() {
            release();
        }
    }
}
pub struct NoGate;
impl AdmissionGate for NoGate {
    fn acquire(
        &self,
        _: &AgentRequest,
        _: &ResolvedLaunch,
        _: GateReason,
    ) -> BoxFuture<'static, GateDecision> {
        Box::pin(async { GateDecision::Proceed(GatePermit::noop()) })
    }
    fn budget(&self) -> BudgetSnapshot {
        BudgetSnapshot::default()
    }
}
#[derive(Clone, Debug)]
pub struct RetryPolicy {
    pub max_outage: Duration,
}
impl Default for RetryPolicy {
    fn default() -> Self {
        Self {
            max_outage: Duration::from_secs(120),
        }
    }
}
impl<P: GatewayPort> GatewayPort for Arc<P> {
    fn create_session(
        &self,
        r: CreateSession,
    ) -> BoxFuture<'static, Result<SessionBound, PortError>> {
        (**self).create_session(r)
    }
    fn attach(&self, p: String, s: String) -> BoxFuture<'static, Result<(), PortError>> {
        (**self).attach(p, s)
    }
    fn submit(&self, r: SubmitTurn) -> BoxFuture<'static, Result<Receipt, PortError>> {
        (**self).submit(r)
    }
    fn receipt(
        &self,
        p: String,
        s: String,
        a: String,
    ) -> BoxFuture<'static, Result<Receipt, PortError>> {
        (**self).receipt(p, s, a)
    }
    fn wait_settled(
        &self,
        p: String,
        s: String,
        a: String,
    ) -> BoxFuture<'static, Result<Receipt, PortError>> {
        (**self).wait_settled(p, s, a)
    }
    fn resolve_unknown(
        &self,
        p: String,
        s: String,
        a: String,
        g: i64,
    ) -> BoxFuture<'static, Result<Receipt, PortError>> {
        (**self).resolve_unknown(p, s, a, g)
    }
}
