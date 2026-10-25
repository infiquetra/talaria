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

/// Queue fanout of `session.info`.
///
/// Issue 232. Queue and steer. The generated session-info type lacks the queue
/// fields and defaults `running` to false, so this frame is decoded from the
/// raw payload. Every field is optional and has no default: absent means unchanged.
#[derive(Debug, Clone, Default, Deserialize, PartialEq)]
pub struct SessionInfoFrame {
    /// Rows the gateway currently holds, including the running turn.
    pub pending: Option<Vec<crate::schemas::CanonicalPendingAdmission>>,
    /// Whether a turn is running, when the frame says so.
    pub running: Option<bool>,
    /// Generation of the running turn, when the frame says so.
    pub execution_generation: Option<i64>,
    /// Gateway revision of this fanout.
    pub revision: Option<i64>,
    /// Authority epoch of this fanout.
    pub authority_epoch: Option<i64>,
    /// Stored session id, when present.
    pub stored_session_id: Option<String>,
    /// Wire protocol name. `hermes-gateway-v1` on the pinned gateway.
    pub desktop_protocol: Option<String>,
}

impl SessionInfoFrame {
    /// Decode the event payload object. Unknown keys are ignored.
    ///
    /// Issue 232. Queue and steer.
    pub fn from_payload(
        payload: &serde_json::Value,
    ) -> Result<SessionInfoFrame, crate::events::DecodeError> {
        let _ = payload;
        todo!("Issue 232. Queue and steer.")
    }
}
