//! Wire events omitted from Hermes's published contract.

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Hermes `apps/shared/src/gateway-events.ts:37` at commit
/// `4d04303818b61bf3aa09278af48356315515965b`; produced by
/// `gateway/session_pending_controls.py:18-38` at the same revision.
/// The producer adds `timeout_s` when known; `f64` accepts integer and fractional seconds.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct ApprovalRequest {
    pub prompt_id: String,
    pub kind: Option<String>,
    pub execution_generation: Option<i64>,
    pub command: Option<String>,
    pub description: Option<String>,
    pub choices: Option<Vec<String>>,
    pub edit: Option<Value>,
    pub timeout_s: Option<f64>,
}

/// Hermes `apps/shared/src/gateway-events.ts:38` at commit
/// `4d04303818b61bf3aa09278af48356315515965b`; produced by
/// `gateway/session_pending_controls.py:52-63` at the same revision.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct ApprovalSettled {
    pub prompt_id: String,
    pub kind: Option<String>,
    pub execution_generation: Option<i64>,
}

/// Hermes `apps/shared/src/gateway-events.ts:39` at commit
/// `4d04303818b61bf3aa09278af48356315515965b`; produced by
/// `gateway/session_pending_controls.py:40-50` at the same revision.
/// The producer adds `timeout_s` when known; `f64` accepts integer and fractional seconds.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct ClarifyRequest {
    pub prompt_id: String,
    pub kind: Option<String>,
    pub execution_generation: Option<i64>,
    pub question: Option<String>,
    pub choices: Option<Vec<String>>,
    pub multi_select: Option<bool>,
    pub timeout_s: Option<f64>,
}

/// Hermes `apps/shared/src/gateway-events.ts:40` at commit
/// `4d04303818b61bf3aa09278af48356315515965b`; produced by
/// `gateway/session_pending_controls.py:52-63` at the same revision.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct ClarifySettled {
    pub prompt_id: String,
    pub kind: Option<String>,
    pub execution_generation: Option<i64>,
}

/// Hermes `apps/shared/src/gateway-events.ts:49` at commit
/// `4d04303818b61bf3aa09278af48356315515965b`; produced by
/// `gateway/session_events.py:62-64` at the same revision.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct SessionReplayGap {
    pub latest_seq: Option<i64>,
    pub replay_epoch: Option<String>,
}
