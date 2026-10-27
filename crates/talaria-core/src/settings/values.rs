use super::{
    FieldKind, FieldView, contains_secret_key, env_reference_names, secrets::has_env_reference,
};
use serde_json::Value;
use std::fmt;
#[derive(Clone, Debug, PartialEq)]
pub enum ValueKind {
    Bool,
    Integer,
    Float,
    Text,
    StringList,
    JsonList,
    Nullable,
    Choice,
    ReadOnly(ReadOnlyReason),
}
#[derive(Clone, Debug, PartialEq)]
pub enum ReadOnlyReason {
    OwnedElsewhere,
    EnvReference(Vec<String>),
    HoldsSecret,
    Unsupported(String),
}
#[derive(Clone, PartialEq)]
pub struct InputError {
    pub message: String,
}
impl fmt::Debug for InputError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("InputError")
    }
}
impl fmt::Display for InputError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}
impl std::error::Error for InputError {}
pub(crate) fn owned_elsewhere(key: &str) -> bool {
    [
        "model",
        "model_context_length",
        "fallback_providers",
        "agent.reasoning_effort",
        "providers",
        "custom_providers",
    ]
    .contains(&key)
        || ["delegation.", "auxiliary.", "moa."]
            .iter()
            .any(|p| key.starts_with(p))
}
pub fn value_kind(
    field: &FieldView,
    current: Option<&Value>,
    default: Option<&Value>,
    raw: Option<&Value>,
) -> ValueKind {
    if owned_elsewhere(&field.key) {
        return ValueKind::ReadOnly(ReadOnlyReason::OwnedElsewhere);
    }
    if let Some(raw) = raw.filter(|v| has_env_reference(v)) {
        return ValueKind::ReadOnly(ReadOnlyReason::EnvReference(env_reference_names(raw)));
    }
    if current.is_some_and(contains_secret_key) {
        return ValueKind::ReadOnly(ReadOnlyReason::HoldsSecret);
    }
    let decided = current
        .filter(|v| !v.is_null())
        .or_else(|| default.filter(|v| !v.is_null()));
    match &field.kind {
        FieldKind::Boolean => ValueKind::Bool,
        FieldKind::Number => {
            if decided.is_some_and(|v| v.is_i64() || v.is_u64()) {
                ValueKind::Integer
            } else {
                ValueKind::Float
            }
        }
        FieldKind::Select => ValueKind::Choice,
        FieldKind::String => {
            if decided.is_none() {
                ValueKind::Nullable
            } else {
                ValueKind::Text
            }
        }
        FieldKind::List => {
            if decided.and_then(Value::as_array).is_none_or(|v| {
                v.iter()
                    .all(|v| v.as_str().is_some_and(|s| !s.contains(',')))
            }) {
                ValueKind::StringList
            } else {
                ValueKind::JsonList
            }
        }
        other => ValueKind::ReadOnly(ReadOnlyReason::Unsupported(other.name().into())),
    }
}
pub(crate) fn same_kind(left: &Value, right: &Value) -> bool {
    matches!(
        (left, right),
        (Value::Null, Value::Null)
            | (Value::Bool(_), Value::Bool(_))
            | (Value::Number(_), Value::Number(_))
            | (Value::String(_), Value::String(_))
            | (Value::Array(_), Value::Array(_))
            | (Value::Object(_), Value::Object(_))
    )
}
pub fn parse_input(
    kind: &ValueKind,
    field: &FieldView,
    text: &str,
    current: Option<&Value>,
) -> Result<Value, InputError> {
    let error = |message: &str| InputError {
        message: message.into(),
    };
    if text.chars().any(char::is_control) {
        return Err(error("Use one line, without control characters."));
    }
    match kind {
        ValueKind::Bool => match text.to_lowercase().as_str() {
            "true" | "on" | "yes" | "1" => Ok(Value::Bool(true)),
            "false" | "off" | "no" | "0" => Ok(Value::Bool(false)),
            _ => Err(error("Choose one of the listed values.")),
        },
        ValueKind::Integer => text
            .parse::<i64>()
            .map(Value::from)
            .map_err(|_| error("Enter a whole number.")),
        ValueKind::Float => text
            .parse::<f64>()
            .ok()
            .and_then(serde_json::Number::from_f64)
            .map(Value::Number)
            .ok_or_else(|| error("Enter a number.")),
        ValueKind::Text => Ok(Value::String(text.into())),
        ValueKind::StringList => Ok(Value::Array(
            text.split(',')
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(|s| Value::String(s.into()))
                .collect(),
        )),
        ValueKind::JsonList => {
            let parsed = serde_json::from_str::<Value>(text).ok();
            let compatible = parsed
                .as_ref()
                .and_then(Value::as_array)
                .is_some_and(|items| {
                    current.and_then(Value::as_array).is_none_or(|old| {
                        items
                            .iter()
                            .all(|v| old.is_empty() || old.iter().any(|o| same_kind(v, o)))
                    })
                });
            if compatible {
                Ok(parsed.unwrap_or(Value::Null))
            } else {
                Err(error(
                    "Enter a JSON array like the current one, for example [].",
                ))
            }
        }
        ValueKind::Nullable => {
            if text.is_empty() {
                Ok(Value::Null)
            } else {
                Ok(serde_json::from_str::<Value>(text)
                    .ok()
                    .filter(|v| !v.is_array() && !v.is_object())
                    .unwrap_or_else(|| Value::String(text.into())))
            }
        }
        ValueKind::Choice => {
            if text.is_empty() && field.clearable {
                Ok(Value::String(String::new()))
            } else {
                field
                    .options
                    .as_ref()
                    .and_then(|options| options.iter().find(|v| option_text(v) == text))
                    .cloned()
                    .ok_or_else(|| error("Choose one of the listed values."))
            }
        }
        ValueKind::ReadOnly(_) => Err(error("This setting cannot be edited here.")),
    }
}
pub(crate) fn option_text(value: &Value) -> String {
    value
        .as_str()
        .map(str::to_owned)
        .unwrap_or_else(|| value.to_string())
}
pub fn lookup<'a>(value: &'a Value, key: &str) -> Option<&'a Value> {
    key.split('.')
        .try_fold(value, |value, part| value.get(part))
}
pub(crate) fn put(value: &mut Value, key: &str, new: Value) {
    let mut parts = key.splitn(2, '.');
    let Some(first) = parts.next() else { return };
    if !value.is_object() {
        *value = Value::Object(Default::default())
    }
    if let Some(map) = value.as_object_mut() {
        if let Some(rest) = parts.next() {
            put(map.entry(first).or_insert(Value::Null), rest, new)
        } else {
            map.insert(first.into(), new);
        }
    }
}
