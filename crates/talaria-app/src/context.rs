//! The context service: breakdown, stats and plan reads (#234).

use std::time::Duration;

use hermes_client::{CallError, CallOptions, Client};
use hermes_contract::methods::canonical::session_stats;
use hermes_contract::methods::legacy::{session_context_breakdown, usage_bars};
use hermes_contract::schemas::{
    ProfileParams, SessionContextBreakdownParams, SessionContextBreakdownResult, SessionStats,
    SessionStatsParams, UsageModel,
};
use talaria_core::context::breakdown::Breakdown;
use talaria_core::context::compact::CompactResult;
use talaria_core::context::text::ReadError;
use talaria_core::context::tracker::ContextNotice;
use talaria_core::context::usage::{ContextFigure, UsageReport};

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

/// One block the screen draws after a local command.
#[derive(Clone, Debug, PartialEq)]
#[allow(clippy::large_enum_variant)]
pub enum ContextBlock {
    /// A `/context` block for a breakdown.
    Breakdown { typed: String, breakdown: Breakdown },
    /// A `/context` block before the agent exists.
    Prebuild {
        typed: String,
        last: Option<ContextFigure>,
    },
    /// A `/usage` block.
    Usage { typed: String, report: UsageReport },
    /// A `/compact` block.
    Compact {
        typed: String,
        result: CompactResult,
    },
    /// A failed read.
    Error {
        typed: String,
        command: String,
        method: String,
        sid: String,
        error: ReadError,
    },
    /// A notice the tracker raised.
    Notice(ContextNotice),
}

/// Reads context data through the Hermes client.
pub struct ContextService {
    client: Client,
    timeouts: ContextTimeouts,
}

impl ContextService {
    /// A service over this client with these budgets.
    pub fn new(client: Client, timeouts: ContextTimeouts) -> Self {
        ContextService { client, timeouts }
    }

    /// Read the context breakdown of a session.
    pub async fn breakdown(&self, sid: &str) -> Result<SessionContextBreakdownResult, ReadError> {
        let params = SessionContextBreakdownParams {
            profile: None,
            session_id: sid.to_owned(),
        };
        self.client
            .call_with::<session_context_breakdown::Marker>(
                params,
                CallOptions {
                    timeout: Some(self.timeouts.breakdown),
                },
            )
            .await
            .map_err(|error| read_error(&error))
    }

    /// Read the usage stats of a session.
    pub async fn stats(&self, sid: &str) -> Result<SessionStats, ReadError> {
        let params = SessionStatsParams {
            session_id: sid.to_owned(),
        };
        self.client
            .call_with::<session_stats::Marker>(
                params,
                CallOptions {
                    timeout: Some(self.timeouts.stats),
                },
            )
            .await
            .map_err(|error| read_error(&error))
    }

    /// Read the subscription plan. Any error gives `None`: the view fails open.
    pub async fn plan(&self) -> Option<UsageModel> {
        self.client
            .call_with::<usage_bars::Marker>(
                ProfileParams { profile: None },
                CallOptions {
                    timeout: Some(self.timeouts.plan),
                },
            )
            .await
            .ok()
    }

    /// The budgets this service was built with.
    pub fn timeouts(&self) -> ContextTimeouts {
        self.timeouts
    }
}

/// Map a client call error onto a context read error.
pub fn read_error(e: &CallError) -> ReadError {
    match e {
        CallError::NotConnected | CallError::Unknown { .. } => ReadError::NotConnected,
        CallError::Encode | CallError::InvalidTimeout => ReadError::NotSent,
        CallError::BadResult { .. } => ReadError::Unreadable,
        CallError::TimedOut { after, .. } => ReadError::TimedOut {
            secs: after.as_secs() + u64::from(after.subsec_nanos() > 0),
        },
        CallError::Rpc(error) => {
            if error.code == -32601 {
                return ReadError::MethodMissing;
            }
            let reason = error
                .reason
                .clone()
                .unwrap_or_else(|| error.message.clone());
            match reason.as_str() {
                "not_found" => ReadError::NotFound,
                "permission_denied" => ReadError::PermissionDenied,
                _ => ReadError::Refused { reason },
            }
        }
    }
}
