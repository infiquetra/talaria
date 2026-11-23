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
    /// Tear down the gateway's live session once its call has settled. The stored session stays
    /// resumable. A port with no such call leaves the session as it is.
    fn close_session(
        &self,
        _profile: String,
        _session_id: String,
    ) -> BoxFuture<'static, Result<(), PortError>> {
        Box::pin(async { Ok(()) })
    }
    /// Make sure nothing on the call's route keeps running once the call has ended. Hermes can
    /// reset a conversation on its own (a context overflow) and start the next queued input on
    /// the route's fresh stored session, which the run does not follow. The port reads the live
    /// session, cancels any admission this run did not make, and reports what it found.
    /// `stored_session_id` is the stored session the call was bound to. A port with no such
    /// call finds nothing.
    fn sweep_route(
        &self,
        _profile: String,
        _session_id: String,
        _stored_session_id: String,
    ) -> BoxFuture<'static, Result<RouteSweep, PortError>> {
        Box::pin(async { Ok(RouteSweep::default()) })
    }
}
/// What `GatewayPort::sweep_route` found on a call's route after the call ended.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct RouteSweep {
    /// The stored session now on the route, when Hermes moved it off the one the call used.
    pub moved_to: Option<String>,
    /// Admissions the run never made that were queued or running; each was cancelled.
    pub cancelled: u32,
    /// Tokens Hermes reports for the moved session, when it reports any.
    pub moved_tokens: Option<u64>,
    /// Something is still running or queued on the route and could not be stopped.
    pub still_active: bool,
    /// Calls that failed while sweeping, in plain words.
    pub problems: Vec<String>,
}
impl RouteSweep {
    /// Whether the sweep found anything the run was not tracking.
    pub fn found_untracked(&self) -> bool {
        self.moved_to.is_some() || self.cancelled > 0 || self.still_active
    }
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
    /// Why the budget ends calls that have not completed; `None` while work may continue.
    fn budget_stop(&self) -> Option<String> {
        crate::engine::exhausted(&self.budget())
    }
    fn retry_after_failure(
        &self,
        _request: &AgentRequest,
        _launch: &ResolvedLaunch,
        _last: &SettledTurn,
        _attempt: u32,
    ) -> bool {
        false
    }
    /// Whether an agent whose turn ran out of context may be restarted in a fresh session.
    /// `restart` counts from 1 and never exceeds `max`; the caller has already ruled out
    /// further restarts beyond `max`. The default allows a restart while the budget has room.
    fn authorise_restart(
        &self,
        _request: &AgentRequest,
        _launch: &ResolvedLaunch,
        _last: &SettledTurn,
        _attempt: u32,
        _restart: u32,
        _max: u32,
    ) -> bool {
        self.budget_stop().is_none()
    }
    /// Count a session's tokens that the run did not start: work found on a route after an
    /// agent call ended. `id` names it; `tokens` is the cumulative reading Hermes reported.
    fn count_untracked_usage(&self, _id: &str, _tokens: u64) {}
}
/// How often an agent that ran out of context is started again in a fresh session.
pub const MAX_OVERFLOW_RESTARTS: u32 = 3;
/// The `failure_reason` Hermes settles a turn with when the model refused its conversation as
/// too long.
pub const CONTEXT_OVERFLOW: &str = "context_overflow";
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
    fn close_session(&self, p: String, s: String) -> BoxFuture<'static, Result<(), PortError>> {
        (**self).close_session(p, s)
    }
    fn sweep_route(
        &self,
        p: String,
        s: String,
        stored: String,
    ) -> BoxFuture<'static, Result<RouteSweep, PortError>> {
        (**self).sweep_route(p, s, stored)
    }
}
