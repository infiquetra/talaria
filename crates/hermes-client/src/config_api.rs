use serde_json::Value;

use crate::rest::{RestClient, RestError};

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

#[derive(Clone, Debug, PartialEq)]
pub struct ConfigSchema {
    pub fields: Vec<ConfigField>,
    pub category_order: Vec<String>,
}

impl<'de> serde::Deserialize<'de> for ConfigSchema {
    fn deserialize<D: serde::Deserializer<'de>>(_deserializer: D) -> Result<Self, D::Error> {
        todo!("settings stub")
    }
}

pub async fn config_schema(_rest: &RestClient) -> Result<ConfigSchema, RestError> {
    todo!("settings stub")
}

pub async fn config_defaults(_rest: &RestClient) -> Result<Value, RestError> {
    todo!("settings stub")
}

pub async fn config_read(_rest: &RestClient, _include_defaults: bool) -> Result<Value, RestError> {
    todo!("settings stub")
}

pub async fn config_write(_rest: &RestClient, _patch: &Value) -> Result<(), RestError> {
    todo!("settings stub")
}
