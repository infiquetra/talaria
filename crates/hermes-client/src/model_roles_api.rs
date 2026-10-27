//! Dashboard shapes for model roles. Decoding and encoding are not filled in yet.

use serde::de::Deserializer;
use serde::ser::Serializer;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::rest::{RestClient, RestError};

fn stub_de<'de, T, D: Deserializer<'de>>(deserializer: D) -> Result<T, D::Error> {
    let _ = deserializer;
    todo!("model roles stub")
}

fn stub_ser<T, S: Serializer>(value: &T, serializer: S) -> Result<S::Ok, S::Error> {
    let _ = (value, serializer);
    todo!("model roles stub")
}

#[derive(Clone, Debug, PartialEq)]
pub struct ModelInfo {
    pub model: String,
    pub provider: String,
    pub auto_context_length: u64,
    pub config_context_length: u64,
    pub effective_context_length: u64,
    pub capabilities: Map<String, Value>,
}

impl Serialize for ModelInfo {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        stub_ser(self, serializer)
    }
}

impl<'de> Deserialize<'de> for ModelInfo {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        stub_de(deserializer)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MainRoute {
    pub provider: String,
    pub model: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RouteFields {
    pub provider: String,
    pub model: String,
    pub base_url: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct AuxiliaryTask {
    pub task: String,
    pub provider: String,
    pub model: String,
    pub base_url: String,
    pub reasoning_effort: Option<String>,
    pub local_endpoint: bool,
    pub label: Option<String>,
    pub hint: Option<String>,
    pub plugin: Option<String>,
    pub inherit_from: Option<String>,
    pub effective: Option<RouteFields>,
    pub extra: Map<String, Value>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct AuxiliaryModels {
    pub tasks: Vec<AuxiliaryTask>,
    pub main: MainRoute,
}

impl Serialize for AuxiliaryModels {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        stub_ser(self, serializer)
    }
}

impl<'de> Deserialize<'de> for AuxiliaryModels {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        stub_de(deserializer)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct MoaSlot {
    pub provider: String,
    pub model: String,
    pub reasoning_effort: Option<String>,
    pub enabled: Option<bool>,
    pub extra: Map<String, Value>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct MoaPreset {
    pub enabled: bool,
    pub reference_models: Vec<MoaSlot>,
    pub aggregator: MoaSlot,
    pub reference_temperature: Option<f64>,
    pub aggregator_temperature: Option<f64>,
    pub reference_timeout: Option<f64>,
    pub degraded_reference_policy: String,
    pub fanout: String,
    pub extra: Map<String, Value>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct NamedPreset {
    pub name: String,
    pub preset: MoaPreset,
}

#[derive(Clone, Debug, PartialEq)]
pub struct MoaConfig {
    pub default_preset: String,
    pub active_preset: String,
    pub presets: Vec<NamedPreset>,
    pub privacy_filter: Option<String>,
}

impl Serialize for MoaConfig {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        stub_ser(self, serializer)
    }
}

impl<'de> Deserialize<'de> for MoaConfig {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        stub_de(deserializer)
    }
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
        stub_ser(self, serializer)
    }
}

impl<'de> Deserialize<'de> for ModelAssignment {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        stub_de(deserializer)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StaleAux {
    pub task: String,
    pub provider: String,
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

impl Serialize for AssignmentReply {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        stub_ser(self, serializer)
    }
}

impl<'de> Deserialize<'de> for AssignmentReply {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        stub_de(deserializer)
    }
}

pub async fn model_info(rest: &RestClient) -> Result<ModelInfo, RestError> {
    let _ = rest;
    todo!("model roles stub")
}

pub async fn auxiliary_models(rest: &RestClient) -> Result<AuxiliaryModels, RestError> {
    let _ = rest;
    todo!("model roles stub")
}

pub async fn moa_config(rest: &RestClient) -> Result<MoaConfig, RestError> {
    let _ = rest;
    todo!("model roles stub")
}

pub async fn save_moa(rest: &RestClient, config: &MoaConfig) -> Result<MoaConfig, RestError> {
    let _ = (rest, config);
    todo!("model roles stub")
}

pub async fn assign_model(
    rest: &RestClient,
    assignment: &ModelAssignment,
) -> Result<AssignmentReply, RestError> {
    let _ = (rest, assignment);
    todo!("model roles stub")
}
