//! Schema types the attach handshake decodes and sends.
//!
//! Unknown fields are ignored, which is what remains after the contract
//! generator strips `additionalProperties: false`.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SkinPayload {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub colors: Option<serde_json::Map<String, serde_json::Value>>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GatewayReadyPayload {
    pub skin: SkinPayload,
    pub change_events: bool,
    pub replay_epoch: String,
    #[serde(default)]
    pub heartbeat: Option<bool>,
    #[serde(default)]
    pub session_authority: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuntimeDescribeResult {
    pub instance_id: String,
    pub profile_id: String,
    pub authority_epoch: u64,
    pub capabilities: Vec<String>,
    pub session_create: serde_json::Value,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct RuntimeDescribeParams {}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClientCapabilitiesParams {
    pub server_requests: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClientCapabilitiesResult {
    pub server_requests: Vec<String>,
    #[serde(default)]
    pub declines_not_shown: Option<bool>,
}
