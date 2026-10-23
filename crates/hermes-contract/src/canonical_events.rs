//! Hand-written payloads for the five events the contract omits.
//!
//! Shapes follow `apps/shared/src/gateway-events.ts` at the pinned commit.
//! `timeout_s` is not in that file; the producer adds it when a timeout is known
//! (`gateway/session_pending_controls.py`). `f64` accepts both whole and fractional
//! seconds. `request_id` is a desktop-local field and is not on the wire.

/// `approval.request`, produced by `gateway/session_pending_controls.py`.
/// Field list follows `apps/shared/src/gateway-events.ts`, plus producer `timeout_s`.
#[derive(Debug, serde::Serialize)]
pub struct ApprovalRequest {
    pub prompt_id: String,
    #[serde(default)]
    pub kind: Option<String>,
    #[serde(default)]
    pub execution_generation: Option<i64>,
    #[serde(default)]
    pub command: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub choices: Option<Vec<String>>,
    #[serde(default)]
    pub edit: Option<serde_json::Value>,
    #[serde(default)]
    pub timeout_s: Option<f64>,
}

/// `approval.settled`, produced by `snapshot` in `gateway/session_pending_controls.py`.
/// Field list follows `apps/shared/src/gateway-events.ts`.
#[derive(Debug, serde::Serialize)]
pub struct ApprovalSettled {
    pub prompt_id: String,
    #[serde(default)]
    pub kind: Option<String>,
    #[serde(default)]
    pub execution_generation: Option<i64>,
}

/// `clarify.request`, produced by `register_clarify` in `gateway/session_pending_controls.py`.
/// Field list follows `apps/shared/src/gateway-events.ts`, plus producer `timeout_s`.
#[derive(Debug, serde::Serialize)]
pub struct ClarifyRequest {
    pub prompt_id: String,
    #[serde(default)]
    pub kind: Option<String>,
    #[serde(default)]
    pub execution_generation: Option<i64>,
    #[serde(default)]
    pub question: Option<String>,
    #[serde(default)]
    pub choices: Option<Vec<String>>,
    #[serde(default)]
    pub multi_select: Option<bool>,
    #[serde(default)]
    pub timeout_s: Option<f64>,
}

/// `clarify.settled`, produced by `snapshot` in `gateway/session_pending_controls.py`.
/// Field list follows `apps/shared/src/gateway-events.ts`.
#[derive(Debug, serde::Serialize)]
pub struct ClarifySettled {
    pub prompt_id: String,
    #[serde(default)]
    pub kind: Option<String>,
    #[serde(default)]
    pub execution_generation: Option<i64>,
}

/// `session.replay_gap`, produced by `gateway/session_events.py`.
/// This event has no `prompt_id`. Field list follows `apps/shared/src/gateway-events.ts`.
#[derive(Debug, serde::Serialize)]
pub struct SessionReplayGap {
    #[serde(default)]
    pub latest_seq: Option<i64>,
    #[serde(default)]
    pub replay_epoch: Option<String>,
}

macro_rules! todo_decode {
    ($ty:ident) => {
        impl<'de> serde::Deserialize<'de> for $ty {
            fn deserialize<D>(_deserializer: D) -> Result<Self, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                todo!(concat!("decode ", stringify!($ty)))
            }
        }
    };
}

todo_decode!(ApprovalRequest);
todo_decode!(ApprovalSettled);
todo_decode!(ClarifyRequest);
todo_decode!(ClarifySettled);
todo_decode!(SessionReplayGap);
