//! Dashboard memory and curator routes.

use crate::rest::{Method, RequestOpts, RestClient, RestError};
use serde_json::{Map, Value, json};
use std::time::Duration;

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
#[derive(Clone, PartialEq)]
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
impl std::fmt::Debug for ProviderField {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ProviderField")
            .field("key", &self.key)
            .field("kind", &self.kind)
            .field("is_set", &self.is_set)
            .finish_non_exhaustive()
    }
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

pub fn parse_memory_status(v: &Value) -> Result<MemoryStatus, MemoryApiError> {
    object(v, "reply")?;
    let providers = array(v.get("providers"), "providers")?
        .iter()
        .enumerate()
        .map(|(i, row)| {
            let path = format!("providers[{i}]");
            object(row, &path)?;
            Ok(MemoryProviderRow {
                name: required_string(row.get("name"), &format!("{path}.name"))?,
                description: string(row, "description", &path, "")?,
                available: boolean(row, "available", &path, false)?,
                configured: boolean(row, "configured", &path, false)?,
                status: string(row, "status", &path, "")?,
                setup: parse_setup(row.get("setup"), &format!("{path}.setup"))?,
            })
        })
        .collect::<Result<_, MemoryApiError>>()?;
    let files = v.get("builtin_files");
    if let Some(files) = files {
        object(files, "builtin_files")?;
    }
    let size = |key: &str| match files.and_then(|files| files.get(key)) {
        None => Ok(0),
        Some(value) => value
            .as_u64()
            .ok_or_else(|| bad(&format!("builtin_files.{key}"))),
    };
    Ok(MemoryStatus {
        active: string(v, "active", "", "")?,
        providers,
        builtin_files: BuiltinFiles {
            memory: size("memory")?,
            user: size("user")?,
        },
    })
}

fn parse_setup(v: Option<&Value>, path: &str) -> Result<ProviderSetup, MemoryApiError> {
    let empty = json!({});
    let v = v.unwrap_or(&empty);
    object(v, path)?;
    let external_dependencies = optional_array(
        v.get("external_dependencies"),
        &format!("{path}.external_dependencies"),
    )?
    .iter()
    .enumerate()
    .map(|(i, value)| {
        let path = format!("{path}.external_dependencies[{i}]");
        object(value, &path)?;
        Ok(ExternalDependency {
            name: required_string(value.get("name"), &format!("{path}.name"))?,
            install: string(value, "install", &path, "")?,
            check: string(value, "check", &path, "")?,
        })
    })
    .collect::<Result<_, MemoryApiError>>()?;
    Ok(ProviderSetup {
        pip_dependencies: strings(
            v.get("pip_dependencies"),
            &format!("{path}.pip_dependencies"),
            true,
        )?,
        python_dependencies_declared: boolean(v, "python_dependencies_declared", path, false)?,
        external_dependencies,
        required_env: strings(v.get("required_env"), &format!("{path}.required_env"), true)?,
        dependencies_installed: boolean(v, "dependencies_installed", path, true)?,
    })
}

pub fn parse_provider_config(
    v: &Value,
    surface: ConfigSurface,
) -> Result<ProviderConfig, MemoryApiError> {
    object(v, "reply")?;
    let name = required_string(v.get("name"), "name")?;
    let fields = array(v.get("fields"), "fields")?
        .iter()
        .enumerate()
        .map(|(i, value)| parse_field(value, &format!("fields[{i}]")))
        .collect::<Result<_, _>>()?;
    Ok(ProviderConfig {
        label: string(v, "label", "", &name)?,
        name,
        surface,
        fields,
    })
}

fn parse_field(v: &Value, path: &str) -> Result<ProviderField, MemoryApiError> {
    object(v, path)?;
    let key = required_string(v.get("key"), &format!("{path}.key"))?;
    let kind = match string(v, "kind", path, "text")?.as_str() {
        "text" => FieldKind::Text,
        "secret" => FieldKind::Secret,
        "select" => FieldKind::Select,
        "bool" | "boolean" => FieldKind::Bool,
        "integer" => FieldKind::Integer,
        "number" => FieldKind::Number,
        "json" => FieldKind::Json,
        other => FieldKind::Other(other.into()),
    };
    let options = optional_array(v.get("options"), &format!("{path}.options"))?
        .iter()
        .enumerate()
        .map(|(i, option)| {
            let path = format!("{path}.options[{i}]");
            object(option, &path)?;
            let value = required_string(option.get("value"), &format!("{path}.value"))?;
            Ok(FieldOption {
                label: string(option, "label", &path, &value)?,
                description: string(option, "description", &path, "")?,
                value,
            })
        })
        .collect::<Result<_, MemoryApiError>>()?;
    let when = match v.get("when") {
        None | Some(Value::Null) => None,
        Some(value) => Some(object(value, &format!("{path}.when"))?.clone()),
    };
    // Never retain a peer's purported secret value, even on a malformed server.
    let value = if kind == FieldKind::Secret {
        Value::String(String::new())
    } else {
        v.get("value").cloned().unwrap_or_else(|| json!(""))
    };
    Ok(ProviderField {
        label: string(v, "label", path, &key)?,
        key,
        kind,
        description: string(v, "description", path, "")?,
        placeholder: string(v, "placeholder", path, "")?,
        required: boolean(v, "required", path, false)?,
        value,
        is_set: boolean(v, "is_set", path, false)?,
        options,
        when,
        minimum: number(v, "minimum", path)?,
        maximum: number(v, "maximum", path)?,
    })
}

pub fn parse_curator(v: &Value) -> Result<CuratorStatus, MemoryApiError> {
    object(v, "reply")?;
    Ok(CuratorStatus {
        enabled: boolean(v, "enabled", "", true)?,
        paused: boolean(v, "paused", "", false)?,
        interval_hours: number(v, "interval_hours", "")?,
        last_run_at: match v.get("last_run_at") {
            None | Some(Value::Null) => None,
            value => Some(required_string(value, "last_run_at")?),
        },
        min_idle_hours: number(v, "min_idle_hours", "")?,
        stale_after_days: number(v, "stale_after_days", "")?,
        archive_after_days: number(v, "archive_after_days", "")?,
    })
}

pub fn parse_action_status(v: &Value) -> Result<ActionStatus, MemoryApiError> {
    object(v, "reply")?;
    Ok(ActionStatus {
        name: required_string(v.get("name"), "name")?,
        running: v
            .get("running")
            .and_then(Value::as_bool)
            .ok_or_else(|| bad("running"))?,
        exit_code: integer(v, "exit_code")?,
        pid: integer(v, "pid")?,
        lines: strings(v.get("lines"), "lines", false)?,
    })
}

fn bad(path: &str) -> MemoryApiError {
    MemoryApiError::BadReply(path.into())
}
fn object<'a>(v: &'a Value, path: &str) -> Result<&'a Map<String, Value>, MemoryApiError> {
    v.as_object().ok_or_else(|| bad(path))
}
fn array<'a>(v: Option<&'a Value>, path: &str) -> Result<&'a [Value], MemoryApiError> {
    v.and_then(Value::as_array)
        .map(Vec::as_slice)
        .ok_or_else(|| bad(path))
}
fn optional_array<'a>(v: Option<&'a Value>, path: &str) -> Result<&'a [Value], MemoryApiError> {
    match v {
        None => Ok(&[]),
        Some(_) => array(v, path),
    }
}
fn required_string(v: Option<&Value>, path: &str) -> Result<String, MemoryApiError> {
    v.and_then(Value::as_str)
        .map(str::to_owned)
        .ok_or_else(|| bad(path))
}
fn field_path(parent: &str, key: &str) -> String {
    if parent.is_empty() {
        key.into()
    } else {
        format!("{parent}.{key}")
    }
}
fn string(v: &Value, key: &str, parent: &str, default: &str) -> Result<String, MemoryApiError> {
    match v.get(key) {
        None => Ok(default.into()),
        value => required_string(value, &field_path(parent, key)),
    }
}
fn boolean(v: &Value, key: &str, parent: &str, default: bool) -> Result<bool, MemoryApiError> {
    match v.get(key) {
        None => Ok(default),
        Some(value) => value.as_bool().ok_or_else(|| bad(&field_path(parent, key))),
    }
}
fn number(v: &Value, key: &str, parent: &str) -> Result<Option<f64>, MemoryApiError> {
    match v.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(value) => value
            .as_f64()
            .map(Some)
            .ok_or_else(|| bad(&field_path(parent, key))),
    }
}
fn integer(v: &Value, key: &str) -> Result<Option<i64>, MemoryApiError> {
    match v.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(value) => value.as_i64().map(Some).ok_or_else(|| bad(key)),
    }
}
fn strings(v: Option<&Value>, path: &str, optional: bool) -> Result<Vec<String>, MemoryApiError> {
    let values = if optional {
        optional_array(v, path)?
    } else {
        array(v, path)?
    };
    values
        .iter()
        .enumerate()
        .map(|(i, value)| required_string(Some(value), &format!("{path}[{i}]")))
        .collect()
}
fn valid_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 64
        && name.as_bytes()[0].is_ascii_alphanumeric()
        && name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-'))
}
fn check_name(name: &str) -> Result<(), MemoryApiError> {
    if valid_name(name) {
        Ok(())
    } else {
        Err(MemoryApiError::BadProviderName)
    }
}
fn profile_query(profile: &str) -> Result<String, MemoryApiError> {
    if profile.is_empty() {
        return Err(MemoryApiError::MissingProfile);
    }
    Ok(url::form_urlencoded::Serializer::new(String::new())
        .append_pair("profile", profile)
        .finish())
}
async fn request(
    rest: &RestClient,
    method: Method,
    path: &str,
    body: Option<&Value>,
    seconds: u64,
) -> Result<Value, MemoryApiError> {
    Ok(rest
        .request(
            method,
            path,
            body,
            RequestOpts {
                deadline: Some(Duration::from_secs(seconds)),
                ..RequestOpts::default()
            },
        )
        .await?)
}
fn check_ok(v: &Value) -> Result<(), MemoryApiError> {
    if v.get("ok").and_then(Value::as_bool) == Some(true) {
        Ok(())
    } else {
        Err(bad("ok"))
    }
}

pub async fn memory_status(rest: &RestClient) -> Result<MemoryStatus, MemoryApiError> {
    parse_memory_status(&request(rest, Method::Get, "/api/memory", None, 45).await?)
}
pub async fn set_memory_provider(
    rest: &RestClient,
    provider: &str,
) -> Result<String, MemoryApiError> {
    if !provider.is_empty() {
        check_name(provider)?;
    }
    let v = request(
        rest,
        Method::Put,
        "/api/memory/provider",
        Some(&json!({"provider":provider})),
        45,
    )
    .await?;
    check_ok(&v)?;
    required_string(v.get("active"), "active")
}
pub async fn reset_memory(
    rest: &RestClient,
    profile: &str,
    target: ResetTarget,
) -> Result<Vec<String>, MemoryApiError> {
    let path = format!("/api/memory/reset?{}", profile_query(profile)?);
    let target = match target {
        ResetTarget::Memory => "memory",
        ResetTarget::User => "user",
        ResetTarget::All => "all",
    };
    let v = request(
        rest,
        Method::Post,
        &path,
        Some(&json!({"target":target})),
        15,
    )
    .await?;
    check_ok(&v)?;
    strings(v.get("deleted"), "deleted", false)
}
pub async fn provider_config(
    rest: &RestClient,
    name: &str,
) -> Result<ProviderConfig, MemoryApiError> {
    check_name(name)?;
    let path = format!("/api/memory/providers/{name}/config");
    let declared = request(
        rest,
        Method::Get,
        &format!("{path}?surface=declared"),
        None,
        15,
    )
    .await?;
    let declared = parse_provider_config(&declared, ConfigSurface::Declared)?;
    if !declared.fields.is_empty() {
        return Ok(declared);
    }
    parse_provider_config(
        &request(rest, Method::Get, &path, None, 45).await?,
        ConfigSurface::Plain,
    )
}
pub async fn save_provider_config(
    rest: &RestClient,
    name: &str,
    surface: ConfigSurface,
    values: &Map<String, Value>,
) -> Result<Value, MemoryApiError> {
    check_name(name)?;
    let suffix = if surface == ConfigSurface::Declared {
        "?surface=declared"
    } else {
        ""
    };
    let v = request(
        rest,
        Method::Put,
        &format!("/api/memory/providers/{name}/config{suffix}"),
        Some(&json!({"values":values})),
        45,
    )
    .await?;
    check_ok(&v)?;
    if surface == ConfigSurface::Plain && required_string(v.get("active"), "active")? != name {
        return Err(bad("active"));
    }
    // Return the documented receipt only; unknown response keys may echo submitted secrets.
    Ok(if surface == ConfigSurface::Plain {
        json!({"ok":true,"active":name})
    } else {
        json!({"ok":true})
    })
}
pub async fn curator_status(rest: &RestClient) -> Result<CuratorStatus, MemoryApiError> {
    parse_curator(&request(rest, Method::Get, "/api/curator", None, 15).await?)
}
pub async fn set_curator_paused(rest: &RestClient, paused: bool) -> Result<bool, MemoryApiError> {
    let v = request(
        rest,
        Method::Put,
        "/api/curator/paused",
        Some(&json!({"paused":paused})),
        15,
    )
    .await?;
    check_ok(&v)?;
    v.get("paused")
        .and_then(Value::as_bool)
        .ok_or_else(|| bad("paused"))
}
pub async fn run_curator(
    rest: &RestClient,
    profile: &str,
) -> Result<ActionStarted, MemoryApiError> {
    let path = format!("/api/curator/run?{}", profile_query(profile)?);
    let v = request(rest, Method::Post, &path, Some(&json!({})), 15).await?;
    check_ok(&v)?;
    Ok(ActionStarted {
        name: required_string(v.get("name"), "name")?,
        pid: integer(&v, "pid")?,
    })
}
pub async fn action_status(
    rest: &RestClient,
    name: &str,
    lines: u32,
) -> Result<ActionStatus, MemoryApiError> {
    check_name(name)?;
    let path = format!("/api/actions/{name}/status?lines={lines}");
    parse_action_status(&request(rest, Method::Get, &path, None, 5).await?)
}
