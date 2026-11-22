use super::{
    ChoiceSpec, ChoiceValue, FieldKind, FieldView, ManyStore, OwnKind, Pick, SchemaList,
    choice_spec, contains_secret_key, env_reference_names, number_spec, secrets::has_env_reference,
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
    /// Any number of listed values, ticked one by one.
    Many,
    /// A listed word or an entry of the user's own.
    Mixed,
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
/// The leaves of a delegation or auxiliary entry that the Model roles screen writes.
const ROUTE_LEAVES: [&str; 7] = [
    "provider",
    "model",
    "base_url",
    "api_key",
    "api_mode",
    "reasoning_effort",
    "fallback_providers",
];
/// Whether another screen writes this key. Only those leaves are owned: the behaviour keys beside
/// them, such as the delegation limits and the auxiliary timeouts, are edited here.
pub(crate) fn owned_elsewhere(key: &str) -> bool {
    [
        "model",
        "model_context_length",
        "fallback_providers",
        "agent.reasoning_effort",
        "providers",
        "custom_providers",
        "moa.default_preset",
        "moa.active_preset",
    ]
    .contains(&key)
        || key.starts_with("moa.presets.")
        || key
            .strip_prefix("delegation.")
            .is_some_and(|leaf| ROUTE_LEAVES.contains(&leaf))
        || key.strip_prefix("auxiliary.").is_some_and(|rest| {
            rest.split_once('.')
                .is_some_and(|(_, leaf)| ROUTE_LEAVES.contains(&leaf))
        })
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
    if let Some(spec) = choice_spec(&field.key) {
        match spec.pick {
            Pick::One if matches!(field.kind, FieldKind::String | FieldKind::Select) => {
                return ValueKind::Choice;
            }
            Pick::Many { .. }
                if matches!(
                    field.kind,
                    FieldKind::String | FieldKind::List | FieldKind::Select
                ) =>
            {
                return ValueKind::Many;
            }
            Pick::Mixed { .. }
                if matches!(
                    field.kind,
                    FieldKind::String | FieldKind::List | FieldKind::Select
                ) =>
            {
                return ValueKind::Mixed;
            }
            _ => {}
        }
    }
    // A number with a stated range, or whose blank value means "not set": Hermes types the
    // second kind as a string because its default is null.
    if let Some(spec) = number_spec(&field.key)
        && (field.kind == FieldKind::Number
            || (field.kind == FieldKind::String && spec.blank.is_some()))
    {
        return if spec.whole() {
            ValueKind::Integer
        } else {
            ValueKind::Float
        };
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
        ValueKind::Integer => {
            let spec = number_spec(&field.key);
            if text.is_empty() && spec.is_some_and(|spec| spec.blank.is_some()) {
                return Ok(Value::Null);
            }
            let number = text.parse::<i64>().map_err(|_| match spec {
                Some(spec) => InputError {
                    message: spec.unreadable(),
                },
                None => error("Enter a whole number."),
            })?;
            match spec.filter(|spec| !spec.allows(number as f64)) {
                Some(spec) => Err(InputError {
                    message: spec.refusal(),
                }),
                None => Ok(Value::from(number)),
            }
        }
        ValueKind::Float => {
            let spec = number_spec(&field.key);
            if text.is_empty() && spec.is_some_and(|spec| spec.blank.is_some()) {
                return Ok(Value::Null);
            }
            let number = text
                .parse::<f64>()
                .ok()
                .filter(|number| spec.is_none_or(|spec| spec.allows(*number)))
                .and_then(serde_json::Number::from_f64);
            match (number, spec) {
                (Some(number), _) => Ok(Value::Number(number)),
                (None, Some(spec)) => Err(InputError {
                    message: spec.refusal(),
                }),
                (None, None) => Err(error("Enter a number.")),
            }
        }
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
            let spec = choice_spec(&field.key);
            let listed = listed_choices(field);
            if let Some((value, item)) = listed.iter().find(|(value, item)| match item {
                Some(item) => item.value == text || item.aliases.contains(&text),
                None => option_text(value) == text,
            }) {
                Ok(match item {
                    Some(item) if item.value != text => Value::String(text.into()),
                    _ => value.clone(),
                })
            } else if text.is_empty() && field.clearable {
                Ok(Value::String(String::new()))
            } else if let Some(current) = current.filter(|current| option_text(current) == text) {
                Ok(current.clone())
            } else if let Some(spec) = spec.filter(|spec| spec.custom.is_some()) {
                Ok(spec.saved(text))
            } else {
                let mut names: Vec<String> = listed
                    .iter()
                    .take(8)
                    .map(|(value, item)| match item {
                        Some(item) if item.value.is_empty() => "\"\"".into(),
                        Some(item) => item.value.into(),
                        None => option_text(value),
                    })
                    .collect();
                if listed.len() > 8 {
                    names.push("…".into());
                }
                Err(InputError {
                    message: format!("Choose one of: {}.", names.join(", ")),
                })
            }
        }
        ValueKind::Many => match choice_spec(&field.key) {
            Some(spec) => parse_many(spec, text, current),
            None => Err(error("This setting cannot be edited here.")),
        },
        ValueKind::Mixed => match choice_spec(&field.key) {
            Some(spec) => parse_mixed(spec, text),
            None => Err(error("This setting cannot be edited here.")),
        },
        ValueKind::ReadOnly(_) => Err(error("This setting cannot be edited here.")),
    }
}
/// The names a multiple choice key holds now, whatever its storage.
pub(crate) fn picked_names(value: &Value) -> Vec<String> {
    match value {
        Value::Array(items) => items.iter().map(option_text).collect(),
        Value::String(text) => text
            .split(',')
            .map(str::trim)
            .filter(|name| !name.is_empty())
            .map(str::to_owned)
            .collect(),
        _ => Vec::new(),
    }
}
/// The value to save for these picks.
pub(crate) fn many_value(store: ManyStore, picks: &[String]) -> Value {
    match store {
        ManyStore::List => Value::Array(picks.iter().cloned().map(Value::String).collect()),
        ManyStore::CommaString => Value::String(picks.join(", ")),
    }
}
fn parse_many(spec: &ChoiceSpec, text: &str, current: Option<&Value>) -> Result<Value, InputError> {
    let Pick::Many { store, .. } = spec.pick else {
        return Err(InputError {
            message: "This setting cannot be edited here.".into(),
        });
    };
    let held = current.map(picked_names).unwrap_or_default();
    let mut picks: Vec<String> = Vec::new();
    let mut unlisted: Vec<&str> = Vec::new();
    for name in text
        .split(',')
        .map(str::trim)
        .filter(|name| !name.is_empty())
    {
        if !spec.items.iter().any(|item| item.value == name) && !held.iter().any(|h| h == name) {
            unlisted.push(name);
        } else if !picks.iter().any(|pick| pick == name) {
            picks.push(name.to_owned());
        }
    }
    if unlisted.is_empty() {
        return Ok(many_value(store, &picks));
    }
    let mut names: Vec<&str> = spec.items.iter().take(8).map(|item| item.value).collect();
    if spec.items.len() > 8 {
        names.push("…");
    }
    Err(InputError {
        message: format!(
            "Choose from: {}; not listed: {}.",
            names.join(", "),
            unlisted.join(", ")
        ),
    })
}
fn parse_mixed(spec: &ChoiceSpec, text: &str) -> Result<Value, InputError> {
    let Pick::Mixed { own, .. } = spec.pick else {
        return Err(InputError {
            message: "This setting cannot be edited here.".into(),
        });
    };
    if spec.items.iter().any(|item| item.value == text) {
        return Ok(spec.saved(text));
    }
    let refused = || InputError {
        message: format!("Enter {}.", spec.ask),
    };
    match own {
        OwnKind::List => {
            let names = picked_names(&Value::String(text.into()));
            if names.is_empty() {
                Err(refused())
            } else {
                Ok(Value::Array(names.into_iter().map(Value::String).collect()))
            }
        }
        OwnKind::Number => text.parse::<u64>().map(Value::from).map_err(|_| refused()),
    }
}
/// The values a choice key lists, in display order: the built-in rows, then the schema options
/// they lack unless the built-in list replaces the schema's. Each comes with its built-in row.
pub(crate) fn listed_choices(field: &FieldView) -> Vec<(Value, Option<&'static ChoiceValue>)> {
    let spec = choice_spec(&field.key);
    let mut listed: Vec<(Value, Option<&'static ChoiceValue>)> = spec
        .map(|spec| {
            spec.items
                .iter()
                .map(|item| (spec.saved(item.value), Some(item)))
                .collect()
        })
        .unwrap_or_default();
    if spec.is_none() && field.clearable {
        listed.push((Value::String(String::new()), None));
    }
    if spec.is_none_or(|spec| spec.schema == SchemaList::Merge) {
        for option in field.options.iter().flatten() {
            if !listed
                .iter()
                .any(|(value, item)| is_choice(option, value, *item))
            {
                listed.push((option.clone(), None));
            }
        }
    }
    listed
}
/// Whether `value` selects the listed `choice`, directly or through one of its aliases.
pub(crate) fn is_choice(value: &Value, choice: &Value, item: Option<&ChoiceValue>) -> bool {
    value == choice
        || item.is_some_and(|item| value.as_str().is_some_and(|s| item.aliases.contains(&s)))
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
