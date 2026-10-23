//! Envelope decoding. Notification and ServerRequest stay in the generated modules
//! and are re-exported here once those modules exist.

use serde_json::Value;

/// `event_type` is the wire `type` for an event and the method name for a server
/// request. It is empty when the event `type` is missing or not a string.
#[derive(Debug)]
pub struct DecodeError {
    pub event_type: String,
    pub reason: String,
    pub missing_fields: Vec<String>,
}

#[derive(Debug)]
pub struct EventFrame {
    pub session_id: Option<String>,
    pub seq: Option<u64>,
    pub replay_epoch: Option<String>,
    pub authority_epoch: Option<i64>,
    pub execution_generation: Option<i64>,
    pub admission_id: Option<String>,
    /// Replaced by the generated `Notification` enum when that type exists.
    pub notification: Value,
}

impl EventFrame {
    pub fn from_params(_params: &Value) -> Result<Self, DecodeError> {
        todo!("EventFrame::from_params")
    }

    pub fn type_name(&self) -> Option<&str> {
        todo!("EventFrame::type_name")
    }
}

#[derive(Debug)]
pub struct ServerRequestFrame {
    pub id: Value,
    /// Replaced by the generated `ServerRequest` enum when that type exists.
    pub request: Value,
}

impl ServerRequestFrame {
    pub fn from_request(_request: &Value) -> Result<Self, DecodeError> {
        todo!("ServerRequestFrame::from_request")
    }
}
