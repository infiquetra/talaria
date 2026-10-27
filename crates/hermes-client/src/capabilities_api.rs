//! Hand-typed capability routes for the dashboard API.

use crate::rest::{RestClient, RestError};
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SkillDto {
    pub name: String,
    pub description: String,
    pub category: Option<String>,
    pub enabled: bool,
    pub usage: u64,
    pub provenance: String,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SkillToggleDto {
    pub ok: bool,
    pub name: String,
    pub enabled: bool,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct HubSearchDto {
    pub results: Vec<Value>,
    pub timed_out: Vec<String>,
    pub installed: Value,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct HubPreviewDto {
    pub name: String,
    pub description: String,
    pub source: String,
    pub identifier: Option<String>,
    pub trust_level: Option<String>,
    pub tags: Vec<String>,
    pub skill_md: String,
    pub files: Vec<String>,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct HubScanDto {
    pub name: String,
    pub identifier: Option<String>,
    pub source: String,
    pub trust_level: Option<String>,
    pub verdict: String,
    pub summary: String,
    pub policy: String,
    pub policy_reason: String,
    pub findings: Vec<Value>,
    pub severity_counts: Value,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ToolsetDto {
    pub name: String,
    pub label: String,
    pub description: String,
    pub platform: String,
    pub platform_label: String,
    pub enabled: bool,
    pub configured: bool,
    pub tools: Vec<String>,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ToolsetToggleDto {
    pub ok: bool,
    pub enabled: bool,
    pub post_setup_started: Option<String>,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ToolsetConfigDto {
    pub has_category: bool,
    pub active_provider: Option<String>,
    pub providers: Vec<Value>,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PluginCatalogDto {
    pub entries: Vec<Value>,
}

pub async fn skills_list(_rest: &RestClient, _profile: &str) -> Result<Vec<SkillDto>, RestError> {
    todo!("capabilities stub")
}
pub async fn skill_toggle(
    _rest: &RestClient,
    _profile: &str,
    _name: &str,
    _enabled: bool,
) -> Result<SkillToggleDto, RestError> {
    todo!("capabilities stub")
}
pub async fn skill_content(
    _rest: &RestClient,
    _profile: &str,
    _name: &str,
) -> Result<String, RestError> {
    todo!("capabilities stub")
}
pub async fn hub_search(
    _rest: &RestClient,
    _profile: &str,
    _q: &str,
) -> Result<HubSearchDto, RestError> {
    todo!("capabilities stub")
}
pub async fn hub_preview(
    _rest: &RestClient,
    _profile: &str,
    _identifier: &str,
) -> Result<HubPreviewDto, RestError> {
    todo!("capabilities stub")
}
pub async fn hub_scan(
    _rest: &RestClient,
    _profile: &str,
    _identifier: &str,
) -> Result<HubScanDto, RestError> {
    todo!("capabilities stub")
}
pub async fn toolsets_list(
    _rest: &RestClient,
    _profile: &str,
) -> Result<Vec<ToolsetDto>, RestError> {
    todo!("capabilities stub")
}
pub async fn toolset_toggle(
    _rest: &RestClient,
    _profile: &str,
    _name: &str,
    _enabled: bool,
) -> Result<ToolsetToggleDto, RestError> {
    todo!("capabilities stub")
}
pub async fn toolset_config(
    _rest: &RestClient,
    _profile: &str,
    _name: &str,
) -> Result<ToolsetConfigDto, RestError> {
    todo!("capabilities stub")
}
pub async fn plugin_catalog(_rest: &RestClient) -> Result<PluginCatalogDto, RestError> {
    todo!("capabilities stub")
}
