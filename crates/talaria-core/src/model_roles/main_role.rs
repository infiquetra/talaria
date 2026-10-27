//! The context and effort patch that follows a main-model write.

use serde_json::{Map, Value, json};

use super::MainRoles;
use crate::models::Effort;

/// Only the keys that differ. `model_context_length` of 0 means auto-detect, and an effort of
/// `None` is sent as the empty string, which Hermes reads as the provider default.
pub fn main_config_patch(before: &MainRoles, after: &MainRoles) -> Option<Value> {
    let context =
        (before.context_override != after.context_override).then_some(after.context_override);
    let effort = (!after.effort_read_only && before.effort != after.effort).then_some(after.effort);
    config_patch(context, effort)
}

/// Builds the patch from explicit choices, so a value the user entered is sent even when it
/// matches what was read.
pub(super) fn config_patch(context: Option<u64>, effort: Option<Option<Effort>>) -> Option<Value> {
    let mut patch = Map::new();
    if let Some(context) = context {
        patch.insert("model_context_length".to_owned(), json!(context));
    }
    if let Some(effort) = effort {
        let wire = effort.map_or("", Effort::as_wire);
        patch.insert("agent".to_owned(), json!({ "reasoning_effort": wire }));
    }
    (!patch.is_empty()).then_some(Value::Object(patch))
}

/// Reads `agent.reasoning_effort`: a word, or a mapping that Talaria shows but never rewrites.
pub(super) fn effort_from_raw(value: &Value) -> (Option<Effort>, bool) {
    match value {
        Value::String(text) => (Effort::parse(text), false),
        Value::Object(map) => (
            map.get("effort")
                .and_then(Value::as_str)
                .and_then(Effort::parse),
            true,
        ),
        _ => (None, false),
    }
}

/// `Default effort:` text for a value read from the file.
pub(super) fn effort_words(value: &Value) -> String {
    let (effort, read_only) = effort_from_raw(value);
    match (effort, read_only) {
        (Some(level), true) => format!("{} (set in config.yaml)", level.as_wire()),
        (None, true) => "set in config.yaml".to_owned(),
        (Some(level), false) => level.as_wire().to_owned(),
        (None, false) => super::text::INHERITED.to_owned(),
    }
}
