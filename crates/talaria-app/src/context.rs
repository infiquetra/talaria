//! The context service: breakdown, stats and plan reads (#234).

use std::time::Duration;

use hermes_client::{CallError, Client};
use hermes_contract::schemas::{SessionContextBreakdownResult, SessionStats, UsageModel};
use talaria_core::context::text::ReadError;

/// Per-call budgets. The compaction budget is eleven minutes.
#[derive(Clone, Copy, Debug)]
pub struct ContextTimeouts {
    pub breakdown: Duration,
    pub stats: Duration,
    pub plan: Duration,
    pub compact: Duration,
}

impl Default for ContextTimeouts {
    fn default() -> Self {
        Self {
            breakdown: Duration::from_secs(30),
            stats: Duration::from_secs(15),
            plan: Duration::from_secs(15),
            compact: Duration::from_secs(660),
        }
    }
}

/// Reads context data through the Hermes client.
pub struct ContextService {
    _client: Client,
    _timeouts: ContextTimeouts,
}

impl ContextService {
    /// A service over this client with these budgets.
    pub fn new(_client: Client, _timeouts: ContextTimeouts) -> Self {
        unimplemented!("#234")
    }

    /// Read the context breakdown of a session.
    pub async fn breakdown(&self, _sid: &str) -> Result<SessionContextBreakdownResult, ReadError> {
        unimplemented!("#234")
    }

    /// Read the usage stats of a session.
    pub async fn stats(&self, _sid: &str) -> Result<SessionStats, ReadError> {
        unimplemented!("#234")
    }

    /// Read the subscription plan. Any error gives `None`: the view fails open.
    pub async fn plan(&self) -> Option<UsageModel> {
        unimplemented!("#234")
    }

    /// The budgets this service was built with.
    pub fn timeouts(&self) -> ContextTimeouts {
        unimplemented!("#234")
    }
}

/// Map a client call error onto a context read error.
pub fn read_error(_e: &CallError) -> ReadError {
    unimplemented!("#234")
}
