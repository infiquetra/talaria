//! Fallback chain patch and identity.

use serde_json::{Map, Value, json};

use super::FallbackEntry;
use super::text;

/// What makes two chain entries the same route. Hermes also collapses provider aliases;
/// Talaria does not, so the markers built on this are labels, never reasons to block a save.
pub fn route_identity(provider: &str, model: &str, base_url: &str) -> (String, String, String) {
    (
        provider.trim().to_lowercase(),
        model.trim().to_lowercase(),
        base_url.trim().trim_end_matches('/').to_lowercase(),
    )
}

/// The whole chain as Hermes stores it. Rows without a provider or model never reach the file.
pub fn fallback_patch(entries: &[FallbackEntry]) -> Value {
    json!({ "fallback_providers": entry_values(entries) })
}

pub(super) fn entry_values(entries: &[FallbackEntry]) -> Vec<Value> {
    entries
        .iter()
        .filter(|entry| is_complete(entry))
        .map(entry_value)
        .collect()
}

pub(super) fn entry_value(entry: &FallbackEntry) -> Value {
    let mut map = Map::new();
    map.insert("provider".to_owned(), Value::String(entry.provider.clone()));
    map.insert("model".to_owned(), Value::String(entry.model.clone()));
    for (key, value) in &entry.extra {
        if key != "provider" && key != "model" {
            map.insert(key.clone(), value.clone());
        }
    }
    Value::Object(map)
}

pub(super) fn is_complete(entry: &FallbackEntry) -> bool {
    !entry.provider.trim().is_empty() && !entry.model.trim().is_empty()
}

/// Reads one raw list item. A string splits at its first `/` (not at position zero).
pub(super) fn parse_entry(value: &Value) -> FallbackEntry {
    match value {
        Value::Object(map) => {
            let text_of = |key: &str| {
                map.get(key)
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_owned()
            };
            FallbackEntry {
                provider: text_of("provider"),
                model: text_of("model"),
                was_string: false,
                extra: map
                    .iter()
                    .filter(|(key, _)| key.as_str() != "provider" && key.as_str() != "model")
                    .map(|(key, value)| (key.clone(), value.clone()))
                    .collect(),
            }
        }
        Value::String(text) => {
            let (provider, model) = match text.find('/') {
                Some(at) if at >= 1 => (text[..at].to_owned(), text[at + 1..].to_owned()),
                _ => (String::new(), text.clone()),
            };
            FallbackEntry {
                provider,
                model,
                was_string: true,
                extra: Map::new(),
            }
        }
        _ => FallbackEntry {
            provider: String::new(),
            model: String::new(),
            was_string: false,
            extra: Map::new(),
        },
    }
}

pub(super) fn parse_entries(values: &[Value]) -> Vec<FallbackEntry> {
    values.iter().map(parse_entry).collect()
}

fn non_empty(entry: &FallbackEntry, key: &str) -> bool {
    entry.extra.get(key).is_some_and(|value| match value {
        Value::String(text) => !text.trim().is_empty(),
        Value::Null => false,
        _ => true,
    })
}

pub(super) fn base_url(entry: &FallbackEntry) -> &str {
    entry
        .extra
        .get("base_url")
        .and_then(Value::as_str)
        .unwrap_or_default()
}

pub(super) fn has_endpoint(entry: &FallbackEntry) -> bool {
    non_empty(entry, "base_url")
}

pub(super) fn has_key(entry: &FallbackEntry) -> bool {
    ["api_key", "key_env", "api_key_env"]
        .iter()
        .any(|key| non_empty(entry, key))
}

pub(super) fn identity(entry: &FallbackEntry) -> (String, String, String) {
    route_identity(&entry.provider, &entry.model, base_url(entry))
}

/// `<model> (via <provider>)` plus the endpoint and key presence markers.
pub(super) fn summary(entry: &FallbackEntry) -> String {
    let model = if entry.model.trim().is_empty() {
        "(no model)"
    } else {
        entry.model.as_str()
    };
    let provider = if entry.provider.trim().is_empty() {
        "(no provider)"
    } else {
        entry.provider.as_str()
    };
    let mut line = format!("{model} (via {provider})");
    if has_endpoint(entry) {
        line.push_str(" [endpoint]");
    }
    if has_key(entry) {
        line.push(' ');
        line.push_str(text::KEY_SET);
    }
    line
}

/// Which entries repeat an earlier one, over `chain` followed by `legacy`.
/// Returns one flag per `chain` entry and one per `legacy` entry.
pub(super) fn duplicates(
    chain: &[FallbackEntry],
    legacy: &[FallbackEntry],
) -> (Vec<bool>, Vec<bool>) {
    let mut seen: Vec<(String, String, String)> = Vec::new();
    let mut mark = |entry: &FallbackEntry| {
        if !is_complete(entry) {
            return false;
        }
        let id = identity(entry);
        if seen.contains(&id) {
            true
        } else {
            seen.push(id);
            false
        }
    };
    let first = chain.iter().map(&mut mark).collect();
    let second = legacy.iter().map(&mut mark).collect();
    (first, second)
}

/// The marker words after a summary.
pub(super) fn notes(
    entry: &FallbackEntry,
    duplicate: bool,
    main: Option<&(String, String)>,
) -> String {
    let mut out = String::new();
    if is_complete(entry)
        && !has_endpoint(entry)
        && main.is_some_and(|(provider, model)| {
            let id = identity(entry);
            id.0 == provider.trim().to_lowercase() && id.1 == model.trim().to_lowercase()
        })
    {
        out.push_str("  ");
        out.push_str(text::SAME_AS_MAIN);
    }
    if duplicate {
        out.push_str("  ");
        out.push_str(text::DUPLICATE);
    }
    if entry.was_string {
        out.push_str("  ");
        out.push_str(text::IGNORED_MARK);
    }
    out
}

/// Two chains hold the same routes and keys. A string entry and the mapping it becomes are equal.
pub(super) fn same_chain(left: &[FallbackEntry], right: &[FallbackEntry]) -> bool {
    left.len() == right.len()
        && left
            .iter()
            .zip(right)
            .all(|(a, b)| a.provider == b.provider && a.model == b.model && a.extra == b.extra)
}
