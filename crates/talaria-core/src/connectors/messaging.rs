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
#[serde(from = "EnvVarWire")]
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

#[derive(Deserialize)]
struct EnvVarWire {
    key: String,
    #[serde(default)]
    prompt: String,
    #[serde(default)]
    required: bool,
    #[serde(default)]
    is_set: bool,
    #[serde(default)]
    is_list: bool,
    #[serde(default)]
    is_password: bool,
    #[serde(default)]
    advanced: bool,
    #[serde(default)]
    value: Option<String>,
    #[serde(default)]
    help: Option<String>,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    url: Option<String>,
}

impl From<EnvVarWire> for EnvVar {
    fn from(wire: EnvVarWire) -> Self {
        Self {
            key: wire.key,
            prompt: wire.prompt,
            required: wire.required,
            is_set: wire.is_set,
            is_list: wire.is_list,
            is_password: wire.is_password,
            advanced: wire.advanced,
            value: if wire.is_list { wire.value } else { None },
            help: wire.help,
            description: wire.description,
            url: wire.url,
        }
    }
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
            .field("entries", &self.value.as_ref().map(|_| "[entries]"))
            .finish()
    }
}

/// Platforms list. The gateway start command is shared. The local path is not a field.
#[derive(Clone, PartialEq, Deserialize)]
pub struct PlatformsReply {
    #[serde(default)]
    pub gateway_start_command: Option<String>,
    #[serde(default)]
    pub platforms: Vec<PlatformRow>,
}

impl std::fmt::Debug for PlatformsReply {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PlatformsReply")
            .field("platforms", &self.platforms.len())
            .finish()
    }
}

/// Drafts held in core. Typed secrets are a key and a presence flag, never the characters.
#[derive(Clone, Default, PartialEq, Eq)]
pub struct Drafts {
    pub typed: BTreeSet<String>,
    pub lists: BTreeMap<String, Vec<String>>,
    pub clear: BTreeSet<String>,
}

impl std::fmt::Debug for Drafts {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Drafts")
            .field("typed", &self.typed)
            .field("lists", &self.lists.keys().collect::<Vec<_>>())
            .field("clear", &self.clear)
            .finish()
    }
}

/// Save or toggle answer.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
pub struct UpdateReply {
    #[serde(default)]
    pub hot_served: bool,
}

/// Test answer.
#[derive(Clone, PartialEq, Eq, Deserialize)]
pub struct TestReply {
    pub ok: bool,
    #[serde(default)]
    pub state: String,
    #[serde(default)]
    pub message: String,
    #[serde(default)]
    pub auth_posture: Option<String>,
}

impl std::fmt::Debug for TestReply {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TestReply")
            .field("ok", &self.ok)
            .field("message_len", &self.message.len())
            .finish()
    }
}

/// Label for a platform row.
pub fn platform_label(row: &PlatformRow) -> String {
    let error_label = |prefix: &str| match row.error_message.as_deref().filter(|s| !s.is_empty()) {
        Some(error) => format!("{prefix}: {}", super::text::sanitize(error)),
        None => prefix.into(),
    };
    match row.state.as_deref() {
        Some("connected") => "connected".into(),
        Some("connecting") => "connecting…".into(),
        Some("retrying") => error_label("retrying"),
        Some("fatal") => error_label("error"),
        Some("disabled") => "off".into(),
        Some("not_configured") => "needs setup".into(),
        Some("pending_restart") => "restart needed".into(),
        Some("gateway_stopped") => "gateway stopped".into(),
        Some("startup_failed") => error_label("startup failed"),
        Some("paused") => "paused".into(),
        Some("disconnected") => "disconnected".into(),
        Some(other) => format!("unknown ({})", super::text::sanitize(other)),
        None => "unknown".into(),
    }
}

/// Split a list field on commas and newlines.
pub fn parse_list(text: &str) -> Vec<String> {
    let mut seen = BTreeSet::new();
    text.split([',', '\n'])
        .map(str::trim)
        .filter(|item| !item.is_empty() && seen.insert(*item))
        .map(str::to_owned)
        .collect()
}

/// Review rows for a platform save. The effect label is empty.
pub fn review_rows(platform: &PlatformRow, drafts: &Drafts) -> Vec<ReviewRow> {
    let keys: BTreeSet<_> = drafts
        .typed
        .iter()
        .chain(drafts.lists.keys())
        .chain(drafts.clear.iter())
        .collect();
    keys.into_iter()
        .filter_map(|key| {
            let field = platform.env_vars.iter().find(|field| &field.key == key)?;
            let old = if field.is_list {
                field
                    .value
                    .as_deref()
                    .map(parse_list)
                    .unwrap_or_default()
                    .into_iter()
                    .map(|item| super::text::sanitize(&item))
                    .collect::<Vec<_>>()
                    .join(", ")
            } else if field.is_set {
                "set".into()
            } else {
                "not set".into()
            };
            let new = if drafts.clear.contains(key) {
                "cleared".into()
            } else if let Some(items) = drafts.lists.get(key) {
                if items.is_empty() {
                    "cleared".into()
                } else {
                    items
                        .iter()
                        .map(|item| super::text::sanitize(item))
                        .collect::<Vec<_>>()
                        .join(", ")
                }
            } else {
                "new value (hidden)".into()
            };
            Some(ReviewRow {
                key: super::text::sanitize(key),
                old,
                new,
                effect_label: String::new(),
                warnings: Vec::new(),
            })
        })
        .collect()
}
