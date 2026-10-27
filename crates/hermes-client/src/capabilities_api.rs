//! Profile-scoped capability routes on the dashboard API.

use crate::rest::{Method, RequestOpts, RestClient, RestError};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use serde_json::{Value, json};
use std::time::Duration;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SkillDto {
    pub name: String,
    #[serde(default, deserialize_with = "empty_text")]
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

fn empty_text<'de, D: serde::Deserializer<'de>>(de: D) -> Result<String, D::Error> {
    Ok(Option::<String>::deserialize(de)?.unwrap_or_default())
}
fn decode<T: DeserializeOwned>(value: Value) -> Result<T, RestError> {
    serde_json::from_value(value).map_err(|_| RestError::BadBody("invalid capability reply".into()))
}
fn opts(seconds: u64) -> RequestOpts {
    RequestOpts {
        deadline: Some(Duration::from_secs(seconds)),
        ..RequestOpts::default()
    }
}
fn encode(text: &str) -> String {
    let mut out = String::new();
    for byte in text.bytes() {
        if byte.is_ascii_alphanumeric() || b"-._~".contains(&byte) {
            out.push(char::from(byte));
        } else {
            use std::fmt::Write;
            let _ = write!(out, "%{byte:02X}");
        }
    }
    out
}
async fn read<T: DeserializeOwned>(
    rest: &RestClient,
    route: String,
    seconds: u64,
) -> Result<T, RestError> {
    decode(
        rest.request(Method::Get, &route, None, opts(seconds))
            .await?,
    )
}
pub async fn skills_list(rest: &RestClient, profile: &str) -> Result<Vec<SkillDto>, RestError> {
    read(rest, format!("/api/skills?profile={}", encode(profile)), 30).await
}
pub async fn skill_toggle(
    rest: &RestClient,
    profile: &str,
    name: &str,
    enabled: bool,
) -> Result<SkillToggleDto, RestError> {
    let reply: SkillToggleDto = decode(
        rest.request(
            Method::Put,
            "/api/skills/toggle",
            Some(&json!({"name":name,"enabled":enabled,"profile":profile})),
            opts(30),
        )
        .await?,
    )?;
    if !reply.ok {
        return Err(RestError::BadBody("invalid skill toggle reply".into()));
    }
    Ok(reply)
}
pub async fn skill_content(
    rest: &RestClient,
    profile: &str,
    name: &str,
) -> Result<String, RestError> {
    #[derive(Deserialize)]
    struct Content {
        content: String,
    }
    let reply: Content = read(
        rest,
        format!(
            "/api/skills/content?name={}&profile={}",
            encode(name),
            encode(profile)
        ),
        30,
    )
    .await?;
    Ok(cap_content(&reply.content))
}
pub async fn hub_search(
    rest: &RestClient,
    profile: &str,
    q: &str,
) -> Result<HubSearchDto, RestError> {
    read(
        rest,
        format!(
            "/api/skills/hub/search?q={}&source=all&limit=20&profile={}",
            encode(q),
            encode(profile)
        ),
        45,
    )
    .await
}
pub async fn hub_preview(
    rest: &RestClient,
    profile: &str,
    identifier: &str,
) -> Result<HubPreviewDto, RestError> {
    let mut reply: HubPreviewDto = read(
        rest,
        format!(
            "/api/skills/hub/preview?identifier={}&profile={}",
            encode(identifier),
            encode(profile)
        ),
        45,
    )
    .await?;
    reply.skill_md = cap_content(&reply.skill_md);
    Ok(reply)
}
pub async fn hub_scan(
    rest: &RestClient,
    profile: &str,
    identifier: &str,
) -> Result<HubScanDto, RestError> {
    read(
        rest,
        format!(
            "/api/skills/hub/scan?identifier={}&profile={}",
            encode(identifier),
            encode(profile)
        ),
        45,
    )
    .await
}
pub async fn toolsets_list(rest: &RestClient, profile: &str) -> Result<Vec<ToolsetDto>, RestError> {
    read(
        rest,
        format!("/api/tools/toolsets?profile={}", encode(profile)),
        30,
    )
    .await
}
pub async fn toolset_toggle(
    rest: &RestClient,
    profile: &str,
    name: &str,
    enabled: bool,
) -> Result<ToolsetToggleDto, RestError> {
    let reply: ToolsetToggleDto = decode(
        rest.request(
            Method::Put,
            &format!("/api/tools/toolsets/{}", encode(name)),
            Some(&json!({"enabled":enabled,"profile":profile})),
            opts(30),
        )
        .await?,
    )?;
    if !reply.ok {
        return Err(RestError::BadBody("invalid toolset toggle reply".into()));
    }
    Ok(reply)
}
pub async fn toolset_config(
    rest: &RestClient,
    profile: &str,
    name: &str,
) -> Result<ToolsetConfigDto, RestError> {
    read(
        rest,
        format!(
            "/api/tools/toolsets/{}/config?profile={}",
            encode(name),
            encode(profile)
        ),
        30,
    )
    .await
}
pub async fn plugin_catalog(rest: &RestClient) -> Result<PluginCatalogDto, RestError> {
    read(rest, "/api/dashboard/plugins/catalog".into(), 30).await
}
pub fn cap_content(text: &str) -> String {
    const LIMIT: usize = 256 * 1024;
    if text.len() <= LIMIT {
        return text.to_owned();
    }
    let mut end = LIMIT;
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}\n(showing the first 256 KiB)", &text[..end])
}
