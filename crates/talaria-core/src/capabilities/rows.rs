use super::*;
use serde_json::Value;

fn field<'a>(v: &'a Value, key: &str) -> Result<&'a Value, CapError> {
    let object = v.as_object().ok_or(CapError::BadBody)?;
    Ok(object.get(key).unwrap_or(&Value::Null))
}
fn string(v: &Value, key: &str) -> Result<String, CapError> {
    match field(v, key)? {
        Value::Null => Ok(String::new()),
        Value::String(s) => Ok(s.clone()),
        _ => Err(CapError::BadBody),
    }
}
fn optional(v: &Value, key: &str) -> Result<Option<String>, CapError> {
    match field(v, key)? {
        Value::Null => Ok(None),
        Value::String(s) => Ok(Some(s.clone())),
        _ => Err(CapError::BadBody),
    }
}
fn required(v: &Value, key: &str) -> Result<String, CapError> {
    field(v, key)?
        .as_str()
        .map(str::to_owned)
        .ok_or(CapError::BadBody)
}
fn boolean(v: &Value, key: &str) -> Result<bool, CapError> {
    match field(v, key)? {
        Value::Null => Ok(false),
        Value::Bool(b) => Ok(*b),
        _ => Err(CapError::BadBody),
    }
}
fn number(v: &Value, key: &str) -> Result<u64, CapError> {
    match field(v, key)? {
        Value::Null => Ok(0),
        n => n.as_u64().ok_or(CapError::BadBody),
    }
}
fn array<'a>(v: &'a Value, key: &str) -> Result<&'a [Value], CapError> {
    match field(v, key)? {
        Value::Null => Ok(&[]),
        Value::Array(a) => Ok(a),
        _ => Err(CapError::BadBody),
    }
}
fn strings(v: &Value, key: &str) -> Result<Vec<String>, CapError> {
    array(v, key)?
        .iter()
        .map(|x| x.as_str().map(str::to_owned).ok_or(CapError::BadBody))
        .collect()
}

impl CapData {
    pub fn from_json(tab: CapTab, value: &Value) -> Result<Self, CapError> {
        match tab {
            CapTab::Skills => {
                let result: Vec<_> = value
                    .as_array()
                    .ok_or(CapError::BadBody)?
                    .iter()
                    .map(|v| {
                        Ok(SkillRow {
                            name: required(v, "name")?,
                            description: string(v, "description")?,
                            category: optional(v, "category")?,
                            enabled: field(v, "enabled")?.as_bool().ok_or(CapError::BadBody)?,
                            usage: number(v, "usage")?,
                            provenance: match string(v, "provenance")?.as_str() {
                                "hub" => Provenance::Hub,
                                "bundled" => Provenance::Bundled,
                                "external" => Provenance::External,
                                "agent" => Provenance::Local,
                                s => Provenance::Other(s.into()),
                            },
                        })
                    })
                    .collect::<Result<_, CapError>>()?;
                Ok(Self::Skills(result))
            }
            CapTab::Toolsets => {
                let result: Vec<_> = value
                    .as_array()
                    .ok_or(CapError::BadBody)?
                    .iter()
                    .map(|v| {
                        Ok(ToolsetRow {
                            name: required(v, "name")?,
                            label: string(v, "label")?,
                            description: string(v, "description")?,
                            platform: string(v, "platform")?,
                            platform_label: string(v, "platform_label")?,
                            enabled: field(v, "enabled")?.as_bool().ok_or(CapError::BadBody)?,
                            configured: boolean(v, "configured")?,
                            tools: strings(v, "tools")?,
                        })
                    })
                    .collect::<Result<_, CapError>>()?;
                Ok(Self::Toolsets(result))
            }
            CapTab::Plugins => {
                let result = array(value, "plugins")?
                    .iter()
                    .map(plugin)
                    .collect::<Result<Vec<_>, _>>()?;
                Ok(Self::Plugins(result))
            }
            CapTab::Hub => {
                let search = value.get("results").is_some();
                let items = array(value, if search { "results" } else { "items" })?
                    .iter()
                    .map(|v| {
                        let name = required(v, "name")?;
                        let install_key =
                            optional(v, "identifier")?.unwrap_or_else(|| name.clone());
                        Ok(HubItem {
                            name,
                            description: string(v, "description")?,
                            source: string(v, "source")?,
                            trust: optional(v, if search { "trust_level" } else { "trust" })?,
                            install_key,
                        })
                    })
                    .collect::<Result<Vec<_>, CapError>>()?;
                let installed = match field(value, "installed")? {
                    Value::Null => Vec::new(),
                    Value::Object(m) => m.keys().cloned().collect(),
                    _ => return Err(CapError::BadBody),
                };
                Ok(Self::Hub(HubPage {
                    items,
                    page: number(value, "page")?,
                    total_pages: number(value, "total_pages")?,
                    total: number(value, "total")?,
                    query: optional(value, "query")?,
                    timed_out: strings(value, "timed_out")?,
                    installed,
                }))
            }
        }
    }
}

fn plugin(v: &Value) -> Result<PluginRow, CapError> {
    let settings = array(v, "settings_schema")?
        .iter()
        .map(|s| {
            let kind = string(s, "type")?;
            let has_value = boolean(s, "has_value")?;
            let display_value = if kind == "secret" {
                if has_value {
                    "set".into()
                } else {
                    "not set".into()
                }
            } else {
                match field(s, "value")? {
                    Value::Null => String::new(),
                    Value::String(s) => s.clone(),
                    v => v.to_string(),
                }
            };
            Ok(PluginSetting {
                key: string(s, "key")?,
                label: string(s, "label")?,
                kind,
                has_value,
                display_value,
            })
        })
        .collect::<Result<Vec<_>, CapError>>()?;
    let servers = array(v, "servers")?
        .iter()
        .map(|s| {
            Ok(PluginServer {
                name: string(s, "name")?,
                state: string(s, "state")?,
                sentence: string(s, "sentence")?,
            })
        })
        .collect::<Result<Vec<_>, CapError>>()?;
    Ok(PluginRow {
        name: required(v, "name")?,
        key: optional(v, "key")?,
        version: string(v, "version")?,
        description: string(v, "description")?,
        source: match string(v, "source")?.as_str() {
            "bundled" => PluginSource::Bundled,
            "user" => PluginSource::User,
            "git" => PluginSource::Git,
            "entrypoint" => PluginSource::Entrypoint,
            s => PluginSource::Other(s.into()),
        },
        status: match string(v, "status")?.as_str() {
            "enabled" => PluginStatus::Enabled,
            "disabled" => PluginStatus::Disabled,
            "not enabled" => PluginStatus::NotEnabled,
            s => PluginStatus::Other(s.into()),
        },
        servers,
        catalog_tier: optional(v, "catalog_tier")?,
        update_available: boolean(v, "update_available")?,
        settings,
    })
}

impl Provenance {
    pub fn label(&self) -> &str {
        match self {
            Self::Hub => "hub",
            Self::Bundled => "bundled",
            Self::External => "external",
            Self::Local => "local",
            Self::Other(s) if s.is_empty() => "-",
            Self::Other(s) => s,
        }
    }
}
impl PluginSource {
    pub fn label(&self) -> &str {
        match self {
            Self::Bundled => "bundled",
            Self::User | Self::Git => "yours",
            Self::Entrypoint => "package",
            Self::Other(s) if s.is_empty() => "-",
            Self::Other(s) => s,
        }
    }
}
impl PluginStatus {
    pub fn enabled(&self) -> Option<bool> {
        match self {
            Self::Enabled => Some(true),
            Self::Disabled | Self::NotEnabled => Some(false),
            Self::Other(_) => None,
        }
    }
}
