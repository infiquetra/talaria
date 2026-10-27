//! Hand-typed dashboard configuration routes; schema order is significant.

use crate::rest::{Method, RequestOpts, RestClient, RestError};
use serde::{
    Deserialize, Deserializer,
    de::{MapAccess, Visitor},
};
use serde_json::Value;

#[derive(Clone, Debug, PartialEq)]
pub struct ConfigField {
    pub key: String,
    pub kind: FieldKind,
    pub description: String,
    pub category: String,
    pub options: Option<Vec<Value>>,
    pub searchable: bool,
    pub clearable: bool,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FieldKind {
    Boolean,
    Number,
    String,
    Text,
    List,
    Select,
    Object,
    Other(String),
}
#[derive(Clone, Debug, PartialEq, Deserialize)]
pub struct ConfigSchema {
    #[serde(deserialize_with = "ordered_fields")]
    pub fields: Vec<ConfigField>,
    #[serde(default)]
    pub category_order: Vec<String>,
}

#[derive(Deserialize)]
struct WireField {
    #[serde(rename = "type", default)]
    kind: Option<String>,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    category: Option<String>,
    #[serde(default)]
    options: Option<Vec<Value>>,
    #[serde(default)]
    searchable: bool,
    #[serde(default)]
    clearable: bool,
}
fn ordered_fields<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> Result<Vec<ConfigField>, D::Error> {
    struct FieldsVisitor;
    impl<'de> Visitor<'de> for FieldsVisitor {
        type Value = Vec<ConfigField>;
        fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            formatter.write_str("an ordered configuration field map")
        }
        fn visit_map<M: MapAccess<'de>>(self, mut map: M) -> Result<Self::Value, M::Error> {
            let mut fields = Vec::new();
            while let Some((key, field)) = map.next_entry::<String, WireField>()? {
                let kind = match field.kind.as_deref().unwrap_or("string") {
                    "boolean" => FieldKind::Boolean,
                    "number" => FieldKind::Number,
                    "string" => FieldKind::String,
                    "text" => FieldKind::Text,
                    "list" => FieldKind::List,
                    "select" => FieldKind::Select,
                    "object" => FieldKind::Object,
                    other => FieldKind::Other(other.to_owned()),
                };
                fields.push(ConfigField {
                    description: field.description.unwrap_or_else(|| key.clone()),
                    category: field.category.unwrap_or_else(|| "general".into()),
                    key,
                    kind,
                    options: field.options,
                    searchable: field.searchable,
                    clearable: field.clearable,
                });
            }
            Ok(fields)
        }
    }
    deserializer.deserialize_map(FieldsVisitor)
}

pub async fn config_schema(rest: &RestClient) -> Result<ConfigSchema, RestError> {
    // Decode from the wire bytes: a Value round trip sorts object keys.
    let bytes = rest
        .request_bytes(
            Method::Get,
            "/api/config/schema",
            None,
            RequestOpts::default(),
        )
        .await?;
    serde_json::from_slice(&bytes)
        .map_err(|_| RestError::BadBody("invalid configuration schema".into()))
}
pub async fn config_defaults(rest: &RestClient) -> Result<Value, RestError> {
    config_object(rest, "/api/config/defaults").await
}
pub async fn config_read(rest: &RestClient, include_defaults: bool) -> Result<Value, RestError> {
    config_object(
        rest,
        if include_defaults {
            "/api/config"
        } else {
            "/api/config?include_defaults=false"
        },
    )
    .await
}
async fn config_object(rest: &RestClient, path: &str) -> Result<Value, RestError> {
    let value = rest.get_json(path).await?;
    if !value.is_object() {
        return Err(RestError::BadBody("invalid configuration record".into()));
    }
    Ok(value)
}
pub async fn config_write(rest: &RestClient, patch: &Value) -> Result<(), RestError> {
    let result = rest
        .put_json("/api/config", &serde_json::json!({"config": patch}))
        .await?;
    if result.get("ok").and_then(Value::as_bool) != Some(true) {
        return Err(RestError::BadBody(
            "invalid configuration write reply".into(),
        ));
    }
    Ok(())
}
