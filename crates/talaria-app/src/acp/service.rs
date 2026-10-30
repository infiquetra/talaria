//! Owned agent processes and sessions.

use super::{AcpAgentEntry, AcpConfig};
use crate::fleet::{FleetHandle, FleetSnapshot};
use std::path::Path;
use std::sync::Arc;
use talaria_core::fleet::acp::{AcpCell, AcpSessionKey};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PermissionResponder {
    pub generation: u64,
    pub request_id: String,
    pub key: AcpSessionKey,
    pub tool_call_id: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProbeReport {
    pub agent_name: String,
    pub protocol_version: u32,
}

#[derive(Debug, thiserror::Error)]
#[error("{0}")]
pub struct AcpError(pub String);

pub struct AcpService {
    _config: AcpConfig,
    fleet: FleetHandle,
}

impl AcpService {
    pub fn start(cfg: AcpConfig, fleet: FleetHandle) -> AcpService {
        AcpService {
            _config: cfg,
            fleet,
        }
    }
    pub async fn new_session(
        &mut self,
        _agent_id: &str,
        _cwd: &Path,
        _first_prompt: &str,
    ) -> Result<AcpSessionKey, AcpError> {
        todo!("add agent session service")
    }
    pub async fn prompt(&mut self, _key: &AcpSessionKey, _text: &str) -> Result<(), AcpError> {
        todo!("add agent session service")
    }
    pub async fn cancel(&mut self, _key: &AcpSessionKey) -> Result<(), AcpError> {
        todo!("add agent session service")
    }
    pub async fn answer_permission(
        &mut self,
        _responder: &PermissionResponder,
        _option_id: &str,
    ) -> Result<(), AcpError> {
        todo!("add agent session service")
    }
    pub fn transcript(&self, _key: &AcpSessionKey) -> Vec<AcpCell> {
        todo!("add agent session service")
    }
    pub fn fleet_snapshot(&self) -> Arc<FleetSnapshot> {
        self.fleet.snapshot()
    }
    pub async fn close(&mut self) {
        todo!("add agent session service")
    }
}

pub async fn probe_agent(_entry: &AcpAgentEntry) -> Result<ProbeReport, AcpError> {
    todo!("add agent protocol probe")
}
