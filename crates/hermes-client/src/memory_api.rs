//! Dashboard memory and curator routes.

use crate::rest::{RestClient, RestError};
use serde_json::{Map, Value};

#[derive(Clone, Debug, PartialEq)]
pub struct MemoryStatus {
    pub active: String,
    pub providers: Vec<MemoryProviderRow>,
    pub builtin_files: BuiltinFiles,
}
#[derive(Clone, Debug, PartialEq)]
pub struct MemoryProviderRow {
    pub name: String,
    pub description: String,
    pub available: bool,
    pub configured: bool,
    pub status: String,
    pub setup: ProviderSetup,
}
#[derive(Clone, Debug, PartialEq)]
pub struct ProviderSetup {
    pub pip_dependencies: Vec<String>,
    pub python_dependencies_declared: bool,
    pub external_dependencies: Vec<ExternalDependency>,
    pub required_env: Vec<String>,
    pub dependencies_installed: bool,
}
#[derive(Clone, Debug, PartialEq)]
pub struct ExternalDependency {
    pub name: String,
    pub install: String,
    pub check: String,
}
#[derive(Clone, Debug, PartialEq)]
pub struct BuiltinFiles {
    pub memory: u64,
    pub user: u64,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConfigSurface {
    Declared,
    Plain,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FieldKind {
    Text,
    Secret,
    Select,
    Bool,
    Integer,
    Number,
    Json,
    Other(String),
}
#[derive(Clone, Debug, PartialEq)]
pub struct FieldOption {
    pub value: String,
    pub label: String,
    pub description: String,
}
#[derive(Clone, Debug, PartialEq)]
pub struct ProviderField {
    pub key: String,
    pub label: String,
    pub kind: FieldKind,
    pub description: String,
    pub placeholder: String,
    pub required: bool,
    pub value: Value,
    pub is_set: bool,
    pub options: Vec<FieldOption>,
    pub when: Option<Map<String, Value>>,
    pub minimum: Option<f64>,
    pub maximum: Option<f64>,
}
#[derive(Clone, Debug, PartialEq)]
pub struct ProviderConfig {
    pub name: String,
    pub label: String,
    pub surface: ConfigSurface,
    pub fields: Vec<ProviderField>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ResetTarget {
    Memory,
    User,
    All,
}
#[derive(Clone, Debug, PartialEq)]
pub struct CuratorStatus {
    pub enabled: bool,
    pub paused: bool,
    pub interval_hours: Option<f64>,
    pub last_run_at: Option<String>,
    pub min_idle_hours: Option<f64>,
    pub stale_after_days: Option<f64>,
    pub archive_after_days: Option<f64>,
}
#[derive(Clone, Debug, PartialEq)]
pub struct ActionStarted {
    pub name: String,
    pub pid: Option<i64>,
}
#[derive(Clone, Debug, PartialEq)]
pub struct ActionStatus {
    pub name: String,
    pub running: bool,
    pub exit_code: Option<i64>,
    pub pid: Option<i64>,
    pub lines: Vec<String>,
}
#[derive(Debug, thiserror::Error)]
pub enum MemoryApiError {
    #[error(transparent)]
    Rest(#[from] RestError),
    #[error("invalid provider name")]
    BadProviderName,
    #[error("profile is required")]
    MissingProfile,
    #[error("invalid reply field: {0}")]
    BadReply(String),
}

pub fn parse_memory_status(_v: &Value) -> Result<MemoryStatus, MemoryApiError> {
    todo!()
}
pub fn parse_provider_config(
    _v: &Value,
    _surface: ConfigSurface,
) -> Result<ProviderConfig, MemoryApiError> {
    todo!()
}
pub fn parse_curator(_v: &Value) -> Result<CuratorStatus, MemoryApiError> {
    todo!()
}
pub fn parse_action_status(_v: &Value) -> Result<ActionStatus, MemoryApiError> {
    todo!()
}
pub async fn memory_status(_rest: &RestClient) -> Result<MemoryStatus, MemoryApiError> {
    todo!()
}
pub async fn set_memory_provider(
    _rest: &RestClient,
    _provider: &str,
) -> Result<String, MemoryApiError> {
    todo!()
}
pub async fn reset_memory(
    _rest: &RestClient,
    _profile: &str,
    _target: ResetTarget,
) -> Result<Vec<String>, MemoryApiError> {
    todo!()
}
pub async fn provider_config(
    _rest: &RestClient,
    _name: &str,
) -> Result<ProviderConfig, MemoryApiError> {
    todo!()
}
pub async fn save_provider_config(
    _rest: &RestClient,
    _name: &str,
    _surface: ConfigSurface,
    _values: &Map<String, Value>,
) -> Result<Value, MemoryApiError> {
    todo!()
}
pub async fn curator_status(_rest: &RestClient) -> Result<CuratorStatus, MemoryApiError> {
    todo!()
}
pub async fn set_curator_paused(_rest: &RestClient, _paused: bool) -> Result<bool, MemoryApiError> {
    todo!()
}
pub async fn run_curator(
    _rest: &RestClient,
    _profile: &str,
) -> Result<ActionStarted, MemoryApiError> {
    todo!()
}
pub async fn action_status(
    _rest: &RestClient,
    _name: &str,
    _lines: u32,
) -> Result<ActionStatus, MemoryApiError> {
    todo!()
}
