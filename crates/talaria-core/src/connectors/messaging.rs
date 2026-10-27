//! Messaging platforms. Stored credential text never enters this crate.

use std::collections::{BTreeMap, BTreeSet};

use serde::Deserialize;

use crate::settings::ReviewRow;

/// One platform after the local path has been dropped.
#[derive(Clone, PartialEq, Deserialize)]
pub struct PlatformRow {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub docs_url: Option<String>,
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub state: Option<String>,
    #[serde(default)]
    pub error_message: Option<String>,
    #[serde(default)]
    pub home_channel: Option<HomeChannel>,
    #[serde(default)]
    pub ingress_url: Option<String>,
    #[serde(default)]
    pub env_vars: Vec<EnvVar>,
}

impl std::fmt::Debug for PlatformRow {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("PlatformRow")
            .field("id", &self.id)
            .field("name_len", &self.name.len())
            .field("enabled", &self.enabled)
            .field("state", &self.state)
            .field("fields", &self.env_vars.len())
            .field("docs", &self.docs_url.as_ref().map(|_| "[link]"))
            .finish()
    }
}

/// Home channel name only.
#[derive(Clone, PartialEq, Eq, Deserialize)]
pub struct HomeChannel {
    #[serde(default)]
    pub name: Option<String>,
}

impl std::fmt::Debug for HomeChannel {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("HomeChannel")
            .field("name_len", &self.name.as_ref().map(String::len))
            .finish()
    }
}

/// One environment field. `value` is present only for list fields.
#[derive(Clone, PartialEq, Deserialize)]
pub struct EnvVar {
    pub key: String,
    #[serde(default)]
    pub prompt: String,
    #[serde(default)]
    pub required: bool,
    #[serde(default)]
    pub is_set: bool,
    #[serde(default)]
    pub is_list: bool,
    #[serde(default)]
    pub is_password: bool,
    #[serde(default)]
    pub advanced: bool,
    #[serde(default)]
    pub value: Option<String>,
    #[serde(default)]
    pub help: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub url: Option<String>,
}

impl std::fmt::Debug for EnvVar {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("EnvVar")
            .field("key", &self.key)
            .field("is_set", &self.is_set)
            .field("is_list", &self.is_list)
            .field("required", &self.required)
            .field("advanced", &self.advanced)
            .field(
                "entries",
                &self.value.as_ref().map(|_| "[entries]"),
            )
            .finish()
    }
}

/// Platforms list. The gateway start command is shared. The local path is not a field.
#[derive(Clone, Debug, PartialEq, Deserialize)]
pub struct PlatformsReply {
    #[serde(default)]
    pub gateway_start_command: Option<String>,
    #[serde(default)]
    pub platforms: Vec<PlatformRow>,
}

/// Drafts held in core. Typed secrets are a key and a presence flag, never the characters.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Drafts {
    pub typed: BTreeSet<String>,
    pub lists: BTreeMap<String, Vec<String>>,
    pub clear: BTreeSet<String>,
}

/// Save or toggle answer.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
pub struct UpdateReply {
    #[serde(default)]
    pub hot_served: bool,
}

/// Test answer.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
pub struct TestReply {
    pub ok: bool,
    #[serde(default)]
    pub state: String,
    #[serde(default)]
    pub message: String,
    #[serde(default)]
    pub auth_posture: Option<String>,
}

/// Label for a platform row.
pub fn platform_label(row: &PlatformRow) -> String {
    let _ = row;
    todo!("connectors stub")
}

/// Split a list field on commas and newlines.
pub fn parse_list(text: &str) -> Vec<String> {
    let _ = text;
    todo!("connectors stub")
}

/// Review rows for a platform save. The effect label is empty.
pub fn review_rows(platform: &PlatformRow, drafts: &Drafts) -> Vec<ReviewRow> {
    let _ = (platform, drafts);
    todo!("connectors stub")
}
