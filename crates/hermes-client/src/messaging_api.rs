//! Messaging and pairing reads and writes on the loopback dashboard.
//!
//! Credential characters stay in [`PlatformUpdate`]'s map until the caller builds a request.
//! Debug output names keys and counts. It does not print values.

use std::collections::BTreeMap;

use serde::Deserialize;
use zeroize::Zeroizing;

use crate::rest::{RestClient, RestError};

/// `GET /api/messaging/platforms`. The local path is not a field, so a reply that carries one drops it.
#[derive(Clone, Debug, Default, PartialEq, Deserialize)]
pub struct PlatformsReply {
    #[serde(default)]
    pub gateway_start_command: Option<String>,
    #[serde(default)]
    pub platforms: Vec<PlatformWire>,
}

/// One platform. Unknown fields, including a stored preview, are ignored.
#[derive(Clone, Debug, Default, PartialEq, Deserialize)]
pub struct PlatformWire {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub docs_url: Option<String>,
    #[serde(default)]
    pub enabled: Option<bool>,
    #[serde(default)]
    pub configured: Option<bool>,
    #[serde(default)]
    pub gateway_running: Option<bool>,
    #[serde(default)]
    pub state: Option<String>,
    #[serde(default)]
    pub error_code: Option<String>,
    #[serde(default)]
    pub error_message: Option<String>,
    #[serde(default)]
    pub updated_at: Option<String>,
    #[serde(default)]
    pub home_channel: Option<HomeChannelWire>,
    #[serde(default)]
    pub ingress_url: Option<String>,
    #[serde(default)]
    pub env_vars: Vec<EnvWire>,
}

/// Home channel. Extra fields are ignored.
#[derive(Clone, Debug, Default, PartialEq, Deserialize)]
pub struct HomeChannelWire {
    #[serde(default)]
    pub platform: Option<String>,
    #[serde(default)]
    pub chat_id: Option<String>,
    #[serde(default)]
    pub name: Option<String>,
}

/// One environment field. A stored preview is not a field, so it is dropped.
#[derive(Clone, Debug, Default, PartialEq, Deserialize)]
pub struct EnvWire {
    #[serde(default)]
    pub key: String,
    #[serde(default)]
    pub required: bool,
    #[serde(default)]
    pub is_set: bool,
    #[serde(default)]
    pub value: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub prompt: String,
    #[serde(default)]
    pub help: Option<String>,
    #[serde(default)]
    pub url: Option<String>,
    #[serde(default)]
    pub is_password: bool,
    #[serde(default)]
    pub is_list: bool,
    #[serde(default)]
    pub advanced: bool,
}

/// Body of a platform save or switch. Values are wiped when this value is dropped.
pub struct PlatformUpdate {
    pub enabled: Option<bool>,
    pub env: BTreeMap<String, Zeroizing<String>>,
    pub clear_env: Vec<String>,
}

impl std::fmt::Debug for PlatformUpdate {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("PlatformUpdate")
            .field("enabled", &self.enabled)
            .field(
                "env_keys",
                &self.env.keys().cloned().collect::<Vec<_>>(),
            )
            .field("clear_count", &self.clear_env.len())
            .finish()
    }
}

/// Save or switch answer. The nested platform keeps the same wire shape as the list.
#[derive(Clone, Debug, Default, PartialEq, Deserialize)]
pub struct UpdateReply {
    #[serde(default)]
    pub ok: bool,
    #[serde(default)]
    pub hot_served: bool,
    #[serde(default)]
    pub platform: Option<PlatformWire>,
}

/// Test answer. A missing posture is `None`, including JSON null.
#[derive(Clone, Debug, Default, PartialEq, Deserialize)]
pub struct TestReply {
    #[serde(default)]
    pub ok: bool,
    #[serde(default)]
    pub state: String,
    #[serde(default)]
    pub message: String,
    #[serde(default)]
    pub auth_posture: Option<String>,
}

/// Pending and approved pairing rows.
#[derive(Clone, Debug, Default, PartialEq, Deserialize)]
pub struct PairingReply {
    #[serde(default)]
    pub pending: Vec<PendingWire>,
    #[serde(default)]
    pub approved: Vec<ApprovedWire>,
}

/// Someone waiting to pair.
#[derive(Clone, Debug, Default, PartialEq, Deserialize)]
pub struct PendingWire {
    #[serde(default)]
    pub platform: String,
    #[serde(default)]
    pub request_id: String,
    #[serde(default)]
    pub user_id: String,
    #[serde(default)]
    pub user_name: Option<String>,
    #[serde(default)]
    pub age_minutes: Option<i64>,
}

/// Someone already paired. The approval time may be absent.
#[derive(Clone, Debug, Default, PartialEq, Deserialize)]
pub struct ApprovedWire {
    #[serde(default)]
    pub platform: String,
    #[serde(default)]
    pub user_id: String,
    #[serde(default)]
    pub user_name: Option<String>,
    #[serde(default)]
    pub approved_at: Option<String>,
}

/// Approve answer. The nested person is ignored.
#[derive(Clone, Debug, Default, PartialEq, Deserialize)]
pub struct ApproveReply {
    #[serde(default)]
    pub ok: bool,
}

/// Read platforms for `profile`. The caller supplies the ticket profile.
pub async fn list_platforms(
    rest: &RestClient,
    profile: &str,
) -> Result<PlatformsReply, RestError> {
    let _ = (rest, profile);
    todo!("messaging stub")
}

/// Save or switch one platform.
pub async fn update_platform(
    rest: &RestClient,
    profile: &str,
    id: &str,
    body: &PlatformUpdate,
) -> Result<UpdateReply, RestError> {
    let _ = (rest, profile, id, body);
    todo!("messaging stub")
}

/// Ask the gateway to test one platform.
pub async fn test_platform(
    rest: &RestClient,
    profile: &str,
    id: &str,
) -> Result<TestReply, RestError> {
    let _ = (rest, profile, id);
    todo!("messaging stub")
}

/// Read pairing rows for `profile`.
pub async fn list_pairing(rest: &RestClient, profile: &str) -> Result<PairingReply, RestError> {
    let _ = (rest, profile);
    todo!("messaging stub")
}

/// Approve one pending request.
pub async fn approve_pairing(
    rest: &RestClient,
    profile: &str,
    platform: &str,
    request_id: &str,
) -> Result<ApproveReply, RestError> {
    let _ = (rest, profile, platform, request_id);
    todo!("messaging stub")
}

/// Revoke one approval.
pub async fn revoke_pairing(
    rest: &RestClient,
    profile: &str,
    platform: &str,
    user_id: &str,
) -> Result<(), RestError> {
    let _ = (rest, profile, platform, user_id);
    todo!("messaging stub")
}
