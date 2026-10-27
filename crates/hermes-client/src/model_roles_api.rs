//! Dashboard shapes for model roles and the calls that read and write them.

use serde::de::{Deserializer, MapAccess, Visitor};
use serde::ser::{SerializeMap, Serializer};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::rest::{Method, RequestOpts, RestClient, RestError};

/// A string field that Hermes may send as `null`.
fn text_or_empty<'de, D: Deserializer<'de>>(deserializer: D) -> Result<String, D::Error> {
    Ok(Option::<String>::deserialize(deserializer)?.unwrap_or_default())
}

/// A count field that Hermes may send as `null`.
fn count_or_zero<'de, D: Deserializer<'de>>(deserializer: D) -> Result<u64, D::Error> {
    Ok(Option::<u64>::deserialize(deserializer)?.unwrap_or_default())
}

/// A map field that Hermes may send as `null`.
fn map_or_empty<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> Result<Map<String, Value>, D::Error> {
    Ok(Option::<Map<String, Value>>::deserialize(deserializer)?.unwrap_or_default())
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ModelInfo {
    #[serde(default, deserialize_with = "text_or_empty")]
    pub model: String,
    #[serde(default, deserialize_with = "text_or_empty")]
    pub provider: String,
    #[serde(default, deserialize_with = "count_or_zero")]
    pub auto_context_length: u64,
    #[serde(default, deserialize_with = "count_or_zero")]
    pub config_context_length: u64,
    #[serde(default, deserialize_with = "count_or_zero")]
    pub effective_context_length: u64,
    #[serde(default, deserialize_with = "map_or_empty")]
    pub capabilities: Map<String, Value>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MainRoute {
    #[serde(default, deserialize_with = "text_or_empty")]
    pub provider: String,
    #[serde(default, deserialize_with = "text_or_empty")]
    pub model: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RouteFields {
    #[serde(default, deserialize_with = "text_or_empty")]
    pub provider: String,
    #[serde(default, deserialize_with = "text_or_empty")]
    pub model: String,
    #[serde(default, deserialize_with = "text_or_empty")]
    pub base_url: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct AuxiliaryTask {
    pub task: String,
    #[serde(default, deserialize_with = "text_or_empty")]
    pub provider: String,
    #[serde(default, deserialize_with = "text_or_empty")]
    pub model: String,
    #[serde(default, deserialize_with = "text_or_empty")]
    pub base_url: String,
    #[serde(default)]
    pub reasoning_effort: Option<String>,
    #[serde(default)]
    pub local_endpoint: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hint: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub plugin: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub inherit_from: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub effective: Option<RouteFields>,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct AuxiliaryModels {
    #[serde(default)]
    pub tasks: Vec<AuxiliaryTask>,
    pub main: MainRoute,
}

#[derive(Clone, Debug, PartialEq, Default, Serialize, Deserialize)]
pub struct MoaSlot {
    #[serde(default, deserialize_with = "text_or_empty")]
    pub provider: String,
    #[serde(default, deserialize_with = "text_or_empty")]
    pub model: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reasoning_effort: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub enabled: Option<bool>,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

fn loud() -> String {
    "loud".to_owned()
}

fn user_turn() -> String {
    "user_turn".to_owned()
}

fn enabled_by_default() -> bool {
    true
}

/// Temperatures and the timeout are sent as `null` when unset, never left out.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MoaPreset {
    #[serde(default = "enabled_by_default")]
    pub enabled: bool,
    #[serde(default)]
    pub reference_models: Vec<MoaSlot>,
    #[serde(default)]
    pub aggregator: MoaSlot,
    #[serde(default)]
    pub reference_temperature: Option<f64>,
    #[serde(default)]
    pub aggregator_temperature: Option<f64>,
    #[serde(default)]
    pub reference_timeout: Option<f64>,
    #[serde(default = "loud")]
    pub degraded_reference_policy: String,
    #[serde(default = "user_turn")]
    pub fanout: String,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct NamedPreset {
    pub name: String,
    pub preset: MoaPreset,
}

fn default_preset_name() -> String {
    "default".to_owned()
}

/// The preset map keeps the order it was read in, in both directions.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MoaConfig {
    #[serde(default = "default_preset_name", deserialize_with = "text_or_empty")]
    pub default_preset: String,
    #[serde(default, deserialize_with = "text_or_empty")]
    pub active_preset: String,
    #[serde(
        default,
        serialize_with = "serialize_presets",
        deserialize_with = "deserialize_presets"
    )]
    pub presets: Vec<NamedPreset>,
    /// Read-only: the payload cannot set it, so it is never sent.
    #[serde(default, skip_serializing)]
    pub privacy_filter: Option<String>,
}

fn serialize_presets<S: Serializer>(
    presets: &[NamedPreset],
    serializer: S,
) -> Result<S::Ok, S::Error> {
    let mut map = serializer.serialize_map(Some(presets.len()))?;
    for item in presets {
        map.serialize_entry(&item.name, &item.preset)?;
    }
    map.end()
}

fn deserialize_presets<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> Result<Vec<NamedPreset>, D::Error> {
    struct PresetsVisitor;
    impl<'de> Visitor<'de> for PresetsVisitor {
        type Value = Vec<NamedPreset>;
        fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            formatter.write_str("a map of named presets")
        }
        fn visit_unit<E: serde::de::Error>(self) -> Result<Self::Value, E> {
            Ok(Vec::new())
        }
        fn visit_none<E: serde::de::Error>(self) -> Result<Self::Value, E> {
            Ok(Vec::new())
        }
        fn visit_map<M: MapAccess<'de>>(self, mut map: M) -> Result<Self::Value, M::Error> {
            let mut presets = Vec::new();
            while let Some((name, preset)) = map.next_entry::<String, MoaPreset>()? {
                presets.push(NamedPreset { name, preset });
            }
            Ok(presets)
        }
    }
    deserializer.deserialize_any(PresetsVisitor)
}

/// Omitted, JSON null, or a level string.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EffortField {
    Omit,
    Inherit,
    Level(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AssignScope {
    Main,
    Auxiliary,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ModelAssignment {
    pub scope: AssignScope,
    pub provider: String,
    pub model: String,
    pub task: String,
    pub reasoning_effort: EffortField,
    pub base_url: String,
    pub confirm_expensive_model: bool,
}

impl Serialize for ModelAssignment {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let auxiliary = self.scope == AssignScope::Auxiliary;
        let mut map = serializer.serialize_map(None)?;
        map.serialize_entry(
            "scope",
            match self.scope {
                AssignScope::Main => "main",
                AssignScope::Auxiliary => "auxiliary",
            },
        )?;
        map.serialize_entry("provider", &self.provider)?;
        map.serialize_entry("model", &self.model)?;
        if auxiliary {
            map.serialize_entry("task", &self.task)?;
            match &self.reasoning_effort {
                EffortField::Omit => {}
                EffortField::Inherit => map.serialize_entry("reasoning_effort", &Value::Null)?,
                EffortField::Level(level) => map.serialize_entry("reasoning_effort", level)?,
            }
        }
        if !self.base_url.is_empty() {
            map.serialize_entry("base_url", &self.base_url)?;
        }
        if self.confirm_expensive_model {
            map.serialize_entry("confirm_expensive_model", &true)?;
        }
        map.end()
    }
}

/// Distinguishes an absent key from a `null` one.
fn present<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Option<Option<String>>, D::Error> {
    Option::<String>::deserialize(deserializer).map(Some)
}

#[derive(Deserialize)]
struct AssignmentWire {
    scope: String,
    #[serde(default, deserialize_with = "text_or_empty")]
    provider: String,
    #[serde(default, deserialize_with = "text_or_empty")]
    model: String,
    #[serde(default)]
    task: String,
    #[serde(default, deserialize_with = "present")]
    reasoning_effort: Option<Option<String>>,
    #[serde(default, deserialize_with = "text_or_empty")]
    base_url: String,
    #[serde(default)]
    confirm_expensive_model: bool,
}

impl<'de> Deserialize<'de> for ModelAssignment {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let wire = AssignmentWire::deserialize(deserializer)?;
        let scope = match wire.scope.as_str() {
            "main" => AssignScope::Main,
            "auxiliary" => AssignScope::Auxiliary,
            _ => return Err(serde::de::Error::custom("scope must be main or auxiliary")),
        };
        Ok(Self {
            scope,
            provider: wire.provider,
            model: wire.model,
            task: wire.task,
            reasoning_effort: match wire.reasoning_effort {
                None => EffortField::Omit,
                Some(None) => EffortField::Inherit,
                Some(Some(level)) => EffortField::Level(level),
            },
            base_url: wire.base_url,
            confirm_expensive_model: wire.confirm_expensive_model,
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct StaleAux {
    #[serde(default, deserialize_with = "text_or_empty")]
    pub task: String,
    #[serde(default, deserialize_with = "text_or_empty")]
    pub provider: String,
    #[serde(default, deserialize_with = "text_or_empty")]
    pub model: String,
}

#[allow(clippy::large_enum_variant)]
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AssignmentReply {
    Main {
        provider: String,
        model: String,
        base_url: String,
        gateway_tools: Vec<String>,
        stale_aux: Vec<StaleAux>,
    },
    Auxiliary {
        tasks: Vec<String>,
        provider: String,
        model: String,
    },
    Reset,
    ConfirmRequired {
        message: String,
    },
}

#[derive(Serialize, Deserialize)]
struct ReplyWire {
    #[serde(default)]
    ok: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    scope: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    provider: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    model: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    base_url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    gateway_tools: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    stale_aux: Option<Vec<StaleAux>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    tasks: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    reset: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    confirm_required: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    confirm_message: Option<String>,
}

impl ReplyWire {
    fn empty(ok: bool) -> Self {
        Self {
            ok,
            scope: None,
            provider: None,
            model: None,
            base_url: None,
            gateway_tools: None,
            stale_aux: None,
            tasks: None,
            reset: None,
            confirm_required: None,
            confirm_message: None,
        }
    }
}

impl Serialize for AssignmentReply {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let wire = match self {
            Self::Main {
                provider,
                model,
                base_url,
                gateway_tools,
                stale_aux,
            } => ReplyWire {
                scope: Some("main".to_owned()),
                provider: Some(provider.clone()),
                model: Some(model.clone()),
                base_url: Some(base_url.clone()),
                gateway_tools: Some(gateway_tools.clone()),
                stale_aux: Some(stale_aux.clone()),
                ..ReplyWire::empty(true)
            },
            Self::Auxiliary {
                tasks,
                provider,
                model,
            } => ReplyWire {
                scope: Some("auxiliary".to_owned()),
                provider: Some(provider.clone()),
                model: Some(model.clone()),
                tasks: Some(tasks.clone()),
                ..ReplyWire::empty(true)
            },
            Self::Reset => ReplyWire {
                scope: Some("auxiliary".to_owned()),
                reset: Some(true),
                ..ReplyWire::empty(true)
            },
            Self::ConfirmRequired { message } => ReplyWire {
                confirm_required: Some(true),
                confirm_message: Some(message.clone()),
                ..ReplyWire::empty(false)
            },
        };
        wire.serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for AssignmentReply {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let wire = ReplyWire::deserialize(deserializer)?;
        if wire.confirm_required == Some(true) {
            return Ok(Self::ConfirmRequired {
                message: wire.confirm_message.unwrap_or_default(),
            });
        }
        if !wire.ok {
            return Err(serde::de::Error::custom("assignment was not accepted"));
        }
        if wire.reset == Some(true) {
            return Ok(Self::Reset);
        }
        match wire.scope.as_deref() {
            Some("main") => Ok(Self::Main {
                provider: wire.provider.unwrap_or_default(),
                model: wire.model.unwrap_or_default(),
                base_url: wire.base_url.unwrap_or_default(),
                gateway_tools: wire.gateway_tools.unwrap_or_default(),
                stale_aux: wire.stale_aux.unwrap_or_default(),
            }),
            Some("auxiliary") => Ok(Self::Auxiliary {
                tasks: wire.tasks.unwrap_or_default(),
                provider: wire.provider.unwrap_or_default(),
                model: wire.model.unwrap_or_default(),
            }),
            _ => Err(serde::de::Error::custom("unknown assignment scope")),
        }
    }
}

fn decode<T: serde::de::DeserializeOwned>(value: Value, what: &str) -> Result<T, RestError> {
    serde_json::from_value(value).map_err(|_| RestError::BadBody(format!("invalid {what}")))
}

/// Decodes from the wire bytes, because a `Value` round trip sorts object keys.
fn decode_bytes<T: serde::de::DeserializeOwned>(bytes: &[u8], what: &str) -> Result<T, RestError> {
    serde_json::from_slice(bytes).map_err(|_| RestError::BadBody(format!("invalid {what}")))
}

pub async fn model_info(rest: &RestClient) -> Result<ModelInfo, RestError> {
    decode(rest.get_json("/api/model/info").await?, "model info")
}

pub async fn auxiliary_models(rest: &RestClient) -> Result<AuxiliaryModels, RestError> {
    decode(
        rest.get_json("/api/model/auxiliary").await?,
        "auxiliary models",
    )
}

pub async fn moa_config(rest: &RestClient) -> Result<MoaConfig, RestError> {
    let bytes = rest
        .request_bytes(Method::Get, "/api/model/moa", None, RequestOpts::default())
        .await?;
    decode_bytes(&bytes, "mixture of agents configuration")
}

pub async fn save_moa(rest: &RestClient, config: &MoaConfig) -> Result<MoaConfig, RestError> {
    // Encoded straight from the config so the presets go out in display order.
    let body = serde_json::to_vec(config)
        .map_err(|_| RestError::BadBody("invalid mixture of agents request".into()))?;
    let bytes = rest
        .request_encoded(
            Method::Put,
            "/api/model/moa",
            Some(body),
            RequestOpts::default(),
        )
        .await?;
    decode_bytes(&bytes, "mixture of agents configuration")
}

pub async fn assign_model(
    rest: &RestClient,
    assignment: &ModelAssignment,
) -> Result<AssignmentReply, RestError> {
    let body = serde_json::to_value(assignment)
        .map_err(|_| RestError::BadBody("invalid model assignment".into()))?;
    decode(
        rest.post_json("/api/model/set", &body).await?,
        "model assignment reply",
    )
}
