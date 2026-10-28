//! Profile-routed gateway port.
use super::feed::{AgentFeedItem, AgentFeedReceiver};
use super::wire::{LinkWireFactory, WireError, WireFactory};
use crate::journal::{CreateSession, GatewayPort, PortError, Receipt, SessionBound, SubmitTurn};
use futures::future::BoxFuture;
use std::{path::PathBuf, sync::Arc, time::Duration};
use tokio::sync::mpsc;

#[derive(Clone, Debug)]
pub struct PollConfig {
    pub start: Duration,
    pub cap: Duration,
}

impl Default for PollConfig {
    fn default() -> Self {
        Self {
            start: Duration::from_millis(250),
            cap: Duration::from_secs(2),
        }
    }
}

#[derive(Clone, Debug)]
pub struct RpcTimeouts {
    pub describe: Duration,
    pub resume: Duration,
    pub interrupt: Duration,
    pub cancel: Duration,
    pub list: Duration,
    pub poll: Duration,
    pub receipt: Duration,
    pub resolve_unknown: Duration,
    pub create: Duration,
    pub submit: Duration,
}

impl Default for RpcTimeouts {
    fn default() -> Self {
        let short = Duration::from_secs(10);
        let final_read = Duration::from_secs(30);
        Self {
            describe: short,
            resume: short,
            interrupt: short,
            cancel: short,
            list: short,
            poll: short,
            receipt: final_read,
            resolve_unknown: final_read,
            create: Duration::from_secs(120),
            submit: Duration::from_secs(120),
        }
    }
}

#[derive(Clone, Debug)]
pub struct PortConfig {
    pub home: PathBuf,
    pub poll: PollConfig,
    pub timeouts: RpcTimeouts,
}

impl Default for PortConfig {
    fn default() -> Self {
        Self {
            home: hermes_client::default_home(),
            poll: PollConfig::default(),
            timeouts: RpcTimeouts::default(),
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PortStats {
    pub runtime_describe: u64,
    pub session_create: u64,
    pub session_resume: u64,
    pub prompt_submit: u64,
    pub prompt_receipt: u64,
    pub prompt_cancel: u64,
    pub session_interrupt: u64,
    pub prompt_resolve_unknown: u64,
    pub session_list: u64,
    pub dropped_items: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SessionInfo {
    pub session_id: String,
    pub request_id: String,
    pub label: String,
    pub source: Option<String>,
    pub model: Option<String>,
    pub provider: Option<String>,
    pub reasoning_effort: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StopResult {
    Sent,
    NotRunning,
    Unknown,
}

#[derive(Debug, PartialEq, thiserror::Error)]
pub enum SetupError {
    #[error("Hermes gateway not found")]
    NotFound(String),
    #[error("gateway unsupported: {missing:?}")]
    Unsupported { missing: Vec<String> },
    #[error("gateway connection failed")]
    Connect(String),
}

#[derive(Clone)]
pub struct HermesPort;

impl HermesPort {
    pub async fn connect(
        cfg: PortConfig,
        run_profile: &str,
    ) -> Result<(Self, AgentFeedReceiver), SetupError> {
        let factory = Arc::new(LinkWireFactory::new(cfg.home.clone()));
        Self::with_wires(cfg, factory, run_profile).await
    }

    pub async fn with_wires(
        cfg: PortConfig,
        wires: Arc<dyn WireFactory>,
        run_profile: &str,
    ) -> Result<(Self, AgentFeedReceiver), SetupError> {
        let _ = (cfg, wires, run_profile);
        let (_tx, rx) = mpsc::unbounded_channel::<AgentFeedItem>();
        Ok((Self, rx))
    }

    pub async fn stop(&self, profile: &str, session_id: &str) -> StopResult {
        let _ = (profile, session_id);
        todo!("hermes binding stub")
    }

    pub fn session_info(&self, session_id: &str) -> Option<SessionInfo> {
        let _ = session_id;
        None
    }

    pub async fn live_session_ids(&self, profile: &str) -> Result<Vec<String>, PortError> {
        let _ = profile;
        todo!("hermes binding stub")
    }

    pub fn stats(&self) -> PortStats {
        PortStats::default()
    }
}

impl GatewayPort for HermesPort {
    fn create_session(
        &self,
        req: CreateSession,
    ) -> BoxFuture<'static, Result<SessionBound, PortError>> {
        let _ = req;
        todo!("hermes binding stub")
    }

    fn attach(
        &self,
        profile: String,
        session_id: String,
    ) -> BoxFuture<'static, Result<(), PortError>> {
        let _ = (profile, session_id);
        todo!("hermes binding stub")
    }

    fn submit(&self, req: SubmitTurn) -> BoxFuture<'static, Result<Receipt, PortError>> {
        let _ = req;
        todo!("hermes binding stub")
    }

    fn receipt(
        &self,
        profile: String,
        session_id: String,
        admission_id: String,
    ) -> BoxFuture<'static, Result<Receipt, PortError>> {
        let _ = (profile, session_id, admission_id);
        todo!("hermes binding stub")
    }

    fn wait_settled(
        &self,
        profile: String,
        session_id: String,
        admission_id: String,
    ) -> BoxFuture<'static, Result<Receipt, PortError>> {
        let _ = (profile, session_id, admission_id);
        todo!("hermes binding stub")
    }

    fn resolve_unknown(
        &self,
        profile: String,
        session_id: String,
        admission_id: String,
        execution_generation: i64,
    ) -> BoxFuture<'static, Result<Receipt, PortError>> {
        let _ = (profile, session_id, admission_id, execution_generation);
        todo!("hermes binding stub")
    }
}

pub fn map_wire_error(method: &str, error: WireError) -> PortError {
    let _ = (method, error);
    todo!("hermes binding stub")
}
