//! Allowlist scrub for recorded model-role replies.
//!
//! Every key that is not named for that file is dropped, at every depth. The
//! values that remain are then reduced: secret-shaped fields, endpoint URLs,
//! user-defined providers, and plugin rows.

use serde_json::{Map, Value};

const MODEL_INFO: &[&str] = &[
    "model",
    "provider",
    "auto_context_length",
    "config_context_length",
    "effective_context_length",
    "capabilities",
];
const CAPABILITIES: &[&str] = &[
    "supports_tools",
    "supports_vision",
    "supports_reasoning",
    "context_window",
    "max_output_tokens",
    "model_family",
];
const TASK_KEYS: &[&str] = &[
    "task",
    "provider",
    "model",
    "base_url",
    "reasoning_effort",
    "local_endpoint",
    "label",
    "hint",
    "plugin",
    "inherit_from",
    "effective",
];
const ROUTE_KEYS: &[&str] = &["provider", "model", "base_url"];
const MAIN_KEYS: &[&str] = &["provider", "model"];
const SLOT_KEYS: &[&str] = &["provider", "model", "reasoning_effort", "enabled"];
const PRESET_KEYS: &[&str] = &[
    "enabled",
    "reference_models",
    "aggregator",
    "reference_temperature",
    "aggregator_temperature",
    "reference_timeout",
    "degraded_reference_policy",
    "fanout",
];
const MOA_TOP: &[&str] = &[
    "ok",
    "default_preset",
    "active_preset",
    "privacy_filter",
    "presets",
    "enabled",
    "reference_models",
    "aggregator",
    "reference_temperature",
    "aggregator_temperature",
    "reference_timeout",
    "degraded_reference_policy",
    "fanout",
];
const ENTRY_KEYS: &[&str] = &[
    "provider",
    "model",
    "base_url",
    "api_key",
    "key_env",
    "api_key_env",
    "api_mode",
];
const DELEGATION_KEYS: &[&str] = &[
    "model",
    "provider",
    "base_url",
    "api_key",
    "api_mode",
    "reasoning_effort",
    "fallback_providers",
];
const PROVIDER_KEYS: &[&str] = &[
    "slug",
    "name",
    "models",
    "authenticated",
    "is_user_defined",
    "api_url",
    "source",
];

#[derive(Clone, Debug)]
struct CustomProvider {
    from: String,
    to: String,
    models: Vec<(String, String)>,
}

/// Rename table built from an unscrubbed `model.options` body.
#[derive(Clone, Debug, Default)]
pub struct ProviderMap {
    custom: Vec<CustomProvider>,
    public_slugs: Vec<String>,
}

impl ProviderMap {
    fn custom(&self, slug: &str) -> Option<&CustomProvider> {
        self.custom.iter().find(|row| row.from == slug)
    }

    fn model_name(row: &CustomProvider, model: &str) -> String {
        row.models
            .iter()
            .find(|(from, _)| from == model)
            .map(|(_, to)| to.clone())
            .unwrap_or_else(|| "model-1".to_owned())
    }
}

/// Build the rename table from an unscrubbed catalogue body.
pub fn provider_map(body: &Value) -> ProviderMap {
    let mut map = ProviderMap::default();
    let Some(rows) = body.get("providers").and_then(Value::as_array) else {
        return map;
    };
    let mut next = 1_u32;
    for row in rows {
        let Some(slug) = row.get("slug").and_then(Value::as_str) else {
            continue;
        };
        if slug == "moa" {
            continue;
        }
        let user_defined = row.get("is_user_defined").and_then(Value::as_bool) == Some(true);
        if !user_defined {
            map.public_slugs.push(slug.to_owned());
            continue;
        }
        let models = row
            .get("models")
            .and_then(Value::as_array)
            .map(|items| {
                items
                    .iter()
                    .enumerate()
                    .filter_map(|(index, item)| {
                        item.as_str()
                            .map(|model| (model.to_owned(), format!("model-{}", index + 1)))
                    })
                    .collect()
            })
            .unwrap_or_default();
        map.custom.push(CustomProvider {
            from: slug.to_owned(),
            to: format!("custom-{next}"),
            models,
        });
        next += 1;
    }
    map
}

/// Scrub one recorded call. Envelope keys stay; `body` is reduced for `file_name`.
pub fn scrub_fixture(file_name: &str, document: &Value, providers: &ProviderMap) -> Value {
    let Some(object) = document.as_object() else {
        return Value::Null;
    };
    let mut out = Map::new();
    for key in ["method", "path_without_query", "status"] {
        if let Some(value) = object.get(key) {
            out.insert(key.to_owned(), value.clone());
        }
    }
    let body = object.get("body").cloned().unwrap_or(Value::Null);
    out.insert("body".to_owned(), scrub_body(file_name, &body, providers));
    Value::Object(out)
}

fn scrub_body(file_name: &str, body: &Value, providers: &ProviderMap) -> Value {
    match file_name {
        "model-info.json" => scrub_model_info(body, providers),
        "auxiliary.json" => scrub_auxiliary(body, providers),
        "moa.json" => scrub_moa(body, providers),
        "config-raw.json" => scrub_config(body, providers),
        "model-options.json" => scrub_options(body, providers),
        _ => Value::Object(Map::new()),
    }
}

fn keep(source: &Map<String, Value>, keys: &[&str]) -> Map<String, Value> {
    let mut out = Map::new();
    for key in keys {
        if let Some(value) = source.get(*key) {
            out.insert((*key).to_owned(), value.clone());
        }
    }
    out
}

fn reduce_url(raw: &str) -> String {
    if raw.is_empty() {
        return String::new();
    }
    let Some((scheme, _)) = raw.split_once("://") else {
        return "http://endpoint.invalid".to_owned();
    };
    if scheme.is_empty()
        || !scheme
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || ch == '+' || ch == '.' || ch == '-')
    {
        return "http://endpoint.invalid".to_owned();
    }
    format!("{scheme}://endpoint.invalid")
}

fn reduce_secret(value: &Value) -> Value {
    match value {
        Value::String(text) if text.is_empty() => Value::String(String::new()),
        Value::String(_) => Value::String("<redacted>".to_owned()),
        other => other.clone(),
    }
}

fn rename_provider_field(value: &Value, providers: &ProviderMap) -> Value {
    let Some(text) = value.as_str() else {
        return value.clone();
    };
    if let Some(row) = providers.custom(text) {
        return Value::String(row.to.clone());
    }
    value.clone()
}

fn rename_model_field(provider_raw: &str, value: &Value, providers: &ProviderMap) -> Value {
    let Some(text) = value.as_str() else {
        return value.clone();
    };
    if let Some(row) = providers.custom(provider_raw) {
        return Value::String(ProviderMap::model_name(row, text));
    }
    value.clone()
}

fn reduce_entry(value: &Value, providers: &ProviderMap) -> Option<Value> {
    match value {
        Value::String(text) => Some(Value::String(reduce_fallback_string(text, providers))),
        Value::Object(map) => {
            let provider_raw = map.get("provider").and_then(Value::as_str).unwrap_or("");
            let mut kept = keep(map, ENTRY_KEYS);
            if let Some(provider) = kept.get("provider").cloned() {
                kept.insert(
                    "provider".to_owned(),
                    rename_provider_field(&provider, providers),
                );
            }
            if let Some(model) = kept.get("model").cloned() {
                kept.insert(
                    "model".to_owned(),
                    rename_model_field(provider_raw, &model, providers),
                );
            }
            if let Some(url) = kept.get("base_url").and_then(Value::as_str) {
                kept.insert("base_url".to_owned(), Value::String(reduce_url(url)));
            }
            if let Some(key) = kept.get("api_key").cloned() {
                kept.insert("api_key".to_owned(), reduce_secret(&key));
            }
            for alias in ["key_env", "api_key_env"] {
                if kept.contains_key(alias) {
                    kept.insert(
                        alias.to_owned(),
                        Value::String("EXAMPLE_KEY_ENV".to_owned()),
                    );
                }
            }
            Some(Value::Object(kept))
        }
        _ => None,
    }
}

fn reduce_fallback_string(text: &str, providers: &ProviderMap) -> String {
    let Some((slug, model)) = text.split_once('/') else {
        return "custom-1/model-1".to_owned();
    };
    if slug.is_empty() {
        return "custom-1/model-1".to_owned();
    }
    if providers.public_slugs.iter().any(|public| public == slug) {
        return text.to_owned();
    }
    if let Some(row) = providers.custom(slug) {
        return format!("{}/{}", row.to, ProviderMap::model_name(row, model));
    }
    let next = providers.custom.len() + 1;
    format!("custom-{next}/model-1")
}

fn reduce_entries(value: &Value, providers: &ProviderMap) -> Value {
    let Some(items) = value.as_array() else {
        return Value::Array(Vec::new());
    };
    Value::Array(
        items
            .iter()
            .filter_map(|item| reduce_entry(item, providers))
            .collect(),
    )
}

fn scrub_model_info(body: &Value, providers: &ProviderMap) -> Value {
    let Some(map) = body.as_object() else {
        return Value::Object(Map::new());
    };
    let mut kept = keep(map, MODEL_INFO);
    if let Some(provider) = kept.get("provider").cloned() {
        kept.insert(
            "provider".to_owned(),
            rename_provider_field(&provider, providers),
        );
    }
    if let Some(caps) = kept.get("capabilities").and_then(Value::as_object) {
        kept.insert(
            "capabilities".to_owned(),
            Value::Object(keep(caps, CAPABILITIES)),
        );
    }
    Value::Object(kept)
}

fn scrub_slot(map: &Map<String, Value>, providers: &ProviderMap) -> Value {
    let provider_raw = map.get("provider").and_then(Value::as_str).unwrap_or("");
    let mut kept = keep(map, SLOT_KEYS);
    if let Some(provider) = kept.get("provider").cloned() {
        kept.insert(
            "provider".to_owned(),
            rename_provider_field(&provider, providers),
        );
    }
    if let Some(model) = kept.get("model").cloned() {
        kept.insert(
            "model".to_owned(),
            rename_model_field(provider_raw, &model, providers),
        );
    }
    Value::Object(kept)
}

fn scrub_preset(map: &Map<String, Value>, providers: &ProviderMap) -> Value {
    let mut kept = keep(map, PRESET_KEYS);
    if let Some(rows) = kept.get("reference_models").and_then(Value::as_array) {
        let reduced = rows
            .iter()
            .filter_map(|row| row.as_object().map(|map| scrub_slot(map, providers)))
            .collect();
        kept.insert("reference_models".to_owned(), Value::Array(reduced));
    }
    if let Some(aggregator) = kept.get("aggregator").and_then(Value::as_object) {
        kept.insert("aggregator".to_owned(), scrub_slot(aggregator, providers));
    }
    Value::Object(kept)
}

fn scrub_moa(body: &Value, providers: &ProviderMap) -> Value {
    let Some(map) = body.as_object() else {
        return Value::Object(Map::new());
    };
    let mut kept = keep(map, MOA_TOP);
    if let Some(presets) = kept.get("presets").and_then(Value::as_object) {
        let mut reduced = Map::new();
        for (name, preset) in presets {
            let value = preset
                .as_object()
                .map(|row| scrub_preset(row, providers))
                .unwrap_or_else(|| Value::Object(Map::new()));
            reduced.insert(name.clone(), value);
        }
        kept.insert("presets".to_owned(), Value::Object(reduced));
    }
    if let Some(rows) = kept.get("reference_models").and_then(Value::as_array) {
        let reduced = rows
            .iter()
            .filter_map(|row| row.as_object().map(|map| scrub_slot(map, providers)))
            .collect();
        kept.insert("reference_models".to_owned(), Value::Array(reduced));
    }
    if let Some(aggregator) = kept.get("aggregator").and_then(Value::as_object) {
        kept.insert("aggregator".to_owned(), scrub_slot(aggregator, providers));
    }
    Value::Object(kept)
}

fn scrub_task(
    map: &Map<String, Value>,
    providers: &ProviderMap,
    plugin_index: Option<u32>,
) -> Value {
    let provider_raw = map.get("provider").and_then(Value::as_str).unwrap_or("");
    let mut kept = keep(map, TASK_KEYS);
    if let Some(index) = plugin_index {
        kept.insert(
            "plugin".to_owned(),
            Value::String(format!("plugin-{index}")),
        );
        kept.insert(
            "label".to_owned(),
            Value::String(format!("Plugin task {index}")),
        );
        kept.insert(
            "hint".to_owned(),
            Value::String(format!("Plugin hint {index}")),
        );
        kept.insert(
            "task".to_owned(),
            Value::String(format!("plugin_task_{index}")),
        );
    }
    if let Some(provider) = kept.get("provider").cloned() {
        kept.insert(
            "provider".to_owned(),
            rename_provider_field(&provider, providers),
        );
    }
    if let Some(model) = kept.get("model").cloned() {
        kept.insert(
            "model".to_owned(),
            rename_model_field(provider_raw, &model, providers),
        );
    }
    if let Some(url) = kept.get("base_url").and_then(Value::as_str) {
        kept.insert("base_url".to_owned(), Value::String(reduce_url(url)));
    }
    if let Some(effective) = kept.get("effective").and_then(Value::as_object) {
        let raw = effective
            .get("provider")
            .and_then(Value::as_str)
            .unwrap_or("");
        let mut route = keep(effective, ROUTE_KEYS);
        if let Some(provider) = route.get("provider").cloned() {
            route.insert(
                "provider".to_owned(),
                rename_provider_field(&provider, providers),
            );
        }
        if let Some(model) = route.get("model").cloned() {
            route.insert(
                "model".to_owned(),
                rename_model_field(raw, &model, providers),
            );
        }
        if let Some(url) = route.get("base_url").and_then(Value::as_str) {
            route.insert("base_url".to_owned(), Value::String(reduce_url(url)));
        }
        kept.insert("effective".to_owned(), Value::Object(route));
    }
    Value::Object(kept)
}

fn scrub_auxiliary(body: &Value, providers: &ProviderMap) -> Value {
    let Some(map) = body.as_object() else {
        return Value::Object(Map::new());
    };
    let mut out = Map::new();
    let mut plugin_index = 0_u32;
    if let Some(tasks) = map.get("tasks").and_then(Value::as_array) {
        let mut reduced = Vec::new();
        for task in tasks {
            let Some(row) = task.as_object() else {
                continue;
            };
            let plugin = row
                .get("plugin")
                .and_then(Value::as_str)
                .filter(|text| !text.is_empty());
            let index = plugin.map(|_| {
                plugin_index += 1;
                plugin_index
            });
            reduced.push(scrub_task(row, providers, index));
        }
        out.insert("tasks".to_owned(), Value::Array(reduced));
    }
    if let Some(main) = map.get("main").and_then(Value::as_object) {
        let mut kept = keep(main, MAIN_KEYS);
        if let Some(provider) = kept.get("provider").cloned() {
            kept.insert(
                "provider".to_owned(),
                rename_provider_field(&provider, providers),
            );
        }
        out.insert("main".to_owned(), Value::Object(kept));
    }
    Value::Object(out)
}

fn scrub_config(body: &Value, providers: &ProviderMap) -> Value {
    let Some(map) = body.as_object() else {
        return Value::Object(Map::new());
    };
    let mut out = Map::new();
    if let Some(entries) = map.get("fallback_providers") {
        out.insert(
            "fallback_providers".to_owned(),
            reduce_entries(entries, providers),
        );
    }
    if let Some(legacy) = map.get("fallback_model") {
        let entries = if legacy.is_array() {
            reduce_entries(legacy, providers)
        } else {
            Value::Array(reduce_entry(legacy, providers).into_iter().collect())
        };
        out.insert("fallback_model".to_owned(), entries);
    }
    if let Some(delegation) = map.get("delegation").and_then(Value::as_object) {
        let mut kept = keep(delegation, DELEGATION_KEYS);
        if let Some(provider) = kept.get("provider").cloned() {
            let raw = provider.as_str().unwrap_or("");
            kept.insert(
                "provider".to_owned(),
                rename_provider_field(&provider, providers),
            );
            if let Some(model) = kept.get("model").cloned() {
                kept.insert(
                    "model".to_owned(),
                    rename_model_field(raw, &model, providers),
                );
            }
        }
        if let Some(url) = kept.get("base_url").and_then(Value::as_str) {
            kept.insert("base_url".to_owned(), Value::String(reduce_url(url)));
        }
        if let Some(key) = kept.get("api_key").cloned() {
            kept.insert("api_key".to_owned(), reduce_secret(&key));
        }
        if let Some(entries) = kept.get("fallback_providers").cloned() {
            kept.insert(
                "fallback_providers".to_owned(),
                reduce_entries(&entries, providers),
            );
        }
        out.insert("delegation".to_owned(), Value::Object(kept));
    }
    if let Some(length) = map.get("model_context_length") {
        out.insert("model_context_length".to_owned(), length.clone());
    }
    if let Some(effort) = map
        .get("agent")
        .and_then(Value::as_object)
        .and_then(|agent| agent.get("reasoning_effort"))
    {
        let mut agent = Map::new();
        agent.insert("reasoning_effort".to_owned(), effort.clone());
        out.insert("agent".to_owned(), Value::Object(agent));
    }
    if let Some(presets) = map
        .get("moa")
        .and_then(|moa| moa.get("presets"))
        .and_then(Value::as_object)
    {
        let mut names = Map::new();
        for name in presets.keys() {
            names.insert(name.clone(), Value::Object(Map::new()));
        }
        let mut moa = Map::new();
        moa.insert("presets".to_owned(), Value::Object(names));
        out.insert("moa".to_owned(), Value::Object(moa));
    }
    Value::Object(out)
}

fn scrub_options(body: &Value, providers: &ProviderMap) -> Value {
    let Some(map) = body.as_object() else {
        return Value::Object(Map::new());
    };
    let mut out = Map::new();
    if let Some(model) = map.get("model") {
        out.insert("model".to_owned(), model.clone());
    }
    if let Some(provider) = map.get("provider") {
        out.insert(
            "provider".to_owned(),
            rename_provider_field(provider, providers),
        );
    }
    if let Some(rows) = map.get("providers").and_then(Value::as_array) {
        let reduced = rows
            .iter()
            .filter_map(|row| {
                row.as_object()
                    .and_then(|map| scrub_provider_row(map, providers))
            })
            .collect();
        out.insert("providers".to_owned(), Value::Array(reduced));
    }
    Value::Object(out)
}

fn scrub_provider_row(map: &Map<String, Value>, providers: &ProviderMap) -> Option<Value> {
    let slug = map.get("slug").and_then(Value::as_str).unwrap_or("");
    if slug == "moa" {
        return None;
    }
    let mut kept = keep(map, PROVIDER_KEYS);
    if let Some(row) = providers.custom(slug) {
        kept.insert("slug".to_owned(), Value::String(row.to.clone()));
        kept.insert("name".to_owned(), Value::String(row.to.clone()));
        let models: Vec<Value> = row
            .models
            .iter()
            .take(5)
            .map(|(_, to)| Value::String(to.clone()))
            .collect();
        kept.insert("models".to_owned(), Value::Array(models));
    } else if let Some(models) = kept.get("models").and_then(Value::as_array) {
        let trimmed: Vec<Value> = models.iter().take(5).cloned().collect();
        kept.insert("models".to_owned(), Value::Array(trimmed));
    }
    if let Some(url) = kept.get("api_url").and_then(Value::as_str) {
        kept.insert("api_url".to_owned(), Value::String(reduce_url(url)));
    }
    Some(Value::Object(kept))
}

fn unexpected_keys(map: &Map<String, Value>, allowed: &[&str], path: &str, out: &mut Vec<String>) {
    for key in map.keys() {
        if !allowed.contains(&key.as_str()) {
            out.push(format!("{path}{key}"));
        }
    }
}

fn check_url(value: &Value, path: &str, out: &mut Vec<String>) {
    let Some(text) = value.as_str() else {
        out.push(format!("{path} is not a string"));
        return;
    };
    if text.is_empty() {
        return;
    }
    let ok = text
        .split_once("://")
        .is_some_and(|(scheme, rest)| rest == "endpoint.invalid" && !scheme.is_empty());
    if !ok {
        out.push(format!("{path} is not a reduced endpoint"));
    }
}

fn walk_value(file_name: &str, value: &Value, path: &str, out: &mut Vec<String>) {
    match value {
        Value::Object(map) => {
            let allowed: &[&str] = if path.is_empty() {
                match file_name {
                    "model-info.json" => MODEL_INFO,
                    "auxiliary.json" => &["tasks", "main"],
                    "moa.json" => MOA_TOP,
                    "config-raw.json" => &[
                        "fallback_providers",
                        "fallback_model",
                        "delegation",
                        "model_context_length",
                        "agent",
                        "moa",
                    ],
                    "model-options.json" => &["model", "provider", "providers"],
                    _ => &[],
                }
            } else {
                &[]
            };
            if !path.is_empty() {
                // Nested objects are checked by their parent. A bare walk still
                // rejects a home path or a token shape in every string below.
            } else {
                unexpected_keys(map, allowed, "", out);
            }
            for (key, child) in map {
                let child_path = if path.is_empty() {
                    key.clone()
                } else {
                    format!("{path}.{key}")
                };
                match (file_name, key.as_str()) {
                    (_, "base_url" | "api_url") => check_url(child, &child_path, out),
                    (_, "api_key") => {
                        let text = child.as_str().unwrap_or("missing");
                        if text != "<redacted>" && !text.is_empty() {
                            out.push(format!("{child_path} was not reduced"));
                        }
                    }
                    (_, "key_env" | "api_key_env") => {
                        if child.as_str() != Some("EXAMPLE_KEY_ENV") {
                            out.push(format!("{child_path} was not reduced"));
                        }
                    }
                    ("model-info.json", "capabilities") => {
                        if let Some(caps) = child.as_object() {
                            unexpected_keys(caps, CAPABILITIES, "capabilities.", out);
                        }
                    }
                    ("auxiliary.json", "tasks") => check_tasks(child, out),
                    ("auxiliary.json", "main") => {
                        if let Some(main) = child.as_object() {
                            unexpected_keys(main, MAIN_KEYS, "main.", out);
                        }
                    }
                    ("moa.json", "presets") => check_presets(child, out),
                    ("moa.json", "reference_models") => check_slots(child, "reference_models", out),
                    ("moa.json", "aggregator") => {
                        if let Some(slot) = child.as_object() {
                            unexpected_keys(slot, SLOT_KEYS, "aggregator.", out);
                        }
                    }
                    ("config-raw.json", "fallback_providers" | "fallback_model") => {
                        check_entries(child, &child_path, out);
                    }
                    ("config-raw.json", "delegation") => check_delegation(child, out),
                    ("config-raw.json", "agent") => {
                        if let Some(agent) = child.as_object() {
                            unexpected_keys(agent, &["reasoning_effort"], "agent.", out);
                        }
                    }
                    ("config-raw.json", "moa") => check_config_moa(child, out),
                    ("model-options.json", "providers") => check_provider_rows(child, out),
                    _ => {}
                }
                remember_strings(child, &child_path, out);
            }
        }
        Value::Array(items) => {
            for (index, item) in items.iter().enumerate() {
                remember_strings(item, &format!("{path}[{index}]"), out);
            }
        }
        Value::String(text) => remember_text(text, path, out),
        _ => {}
    }
}

fn remember_strings(value: &Value, path: &str, out: &mut Vec<String>) {
    match value {
        Value::String(text) => remember_text(text, path, out),
        Value::Array(items) => {
            for (index, item) in items.iter().enumerate() {
                remember_strings(item, &format!("{path}[{index}]"), out);
            }
        }
        Value::Object(map) => {
            for (key, child) in map {
                remember_strings(child, &format!("{path}.{key}"), out);
            }
        }
        _ => {}
    }
}

fn remember_text(text: &str, path: &str, out: &mut Vec<String>) {
    let secret = text.chars().collect::<Vec<_>>();
    let raw = text;
    if raw.contains("Bearer ") || raw.contains("/Users/") || raw.contains("/home/") {
        out.push(format!("{path} holds a private path or token"));
        return;
    }
    let folded: String = secret.iter().collect();
    if folded
        .split("sk-")
        .skip(1)
        .any(|tail| tail.chars().take(8).all(|ch| ch.is_ascii_alphanumeric()) && tail.len() >= 8)
    {
        out.push(format!("{path} holds a secret shape"));
    }
}

fn check_tasks(value: &Value, out: &mut Vec<String>) {
    let Some(tasks) = value.as_array() else {
        return;
    };
    for (index, task) in tasks.iter().enumerate() {
        let Some(map) = task.as_object() else {
            continue;
        };
        unexpected_keys(map, TASK_KEYS, &format!("tasks[{index}]."), out);
        if let Some(plugin) = map.get("plugin").and_then(Value::as_str) {
            if !plugin.starts_with("plugin-") {
                out.push(format!("tasks[{index}].plugin was not reduced"));
            }
        }
        if let Some(effective) = map.get("effective").and_then(Value::as_object) {
            unexpected_keys(
                effective,
                ROUTE_KEYS,
                &format!("tasks[{index}].effective."),
                out,
            );
        }
    }
}

fn check_slots(value: &Value, path: &str, out: &mut Vec<String>) {
    let Some(rows) = value.as_array() else { return };
    for (index, row) in rows.iter().enumerate() {
        if let Some(map) = row.as_object() {
            unexpected_keys(map, SLOT_KEYS, &format!("{path}[{index}]."), out);
        }
    }
}

fn check_presets(value: &Value, out: &mut Vec<String>) {
    let Some(presets) = value.as_object() else {
        return;
    };
    for (name, preset) in presets {
        let Some(map) = preset.as_object() else {
            continue;
        };
        unexpected_keys(map, PRESET_KEYS, &format!("presets.{name}."), out);
        if let Some(rows) = map.get("reference_models") {
            check_slots(rows, &format!("presets.{name}.reference_models"), out);
        }
        if let Some(slot) = map.get("aggregator").and_then(Value::as_object) {
            unexpected_keys(slot, SLOT_KEYS, &format!("presets.{name}.aggregator."), out);
        }
    }
}

fn check_entries(value: &Value, path: &str, out: &mut Vec<String>) {
    let Some(rows) = value.as_array() else {
        out.push(format!("{path} is not a list"));
        return;
    };
    for (index, row) in rows.iter().enumerate() {
        match row {
            Value::String(_) => {}
            Value::Object(map) => {
                unexpected_keys(map, ENTRY_KEYS, &format!("{path}[{index}]."), out)
            }
            _ => out.push(format!("{path}[{index}] is not an entry")),
        }
    }
}

fn check_delegation(value: &Value, out: &mut Vec<String>) {
    let Some(map) = value.as_object() else { return };
    unexpected_keys(map, DELEGATION_KEYS, "delegation.", out);
    if let Some(entries) = map.get("fallback_providers") {
        check_entries(entries, "delegation.fallback_providers", out);
    }
}

fn check_config_moa(value: &Value, out: &mut Vec<String>) {
    let Some(map) = value.as_object() else { return };
    unexpected_keys(map, &["presets"], "moa.", out);
    if let Some(presets) = map.get("presets").and_then(Value::as_object) {
        for (name, preset) in presets {
            if preset.as_object().is_none_or(|row| !row.is_empty()) {
                out.push(format!("moa.presets.{name} was not emptied"));
            }
        }
    }
}

fn check_provider_rows(value: &Value, out: &mut Vec<String>) {
    let Some(rows) = value.as_array() else { return };
    for (index, row) in rows.iter().enumerate() {
        let Some(map) = row.as_object() else { continue };
        if map.is_empty() {
            continue;
        }
        unexpected_keys(map, PROVIDER_KEYS, &format!("providers[{index}]."), out);
        if map.get("slug").and_then(Value::as_str) == Some("moa") {
            out.push(format!("providers[{index}] still lists moa"));
        }
        if let Some(models) = map.get("models").and_then(Value::as_array) {
            if models.len() > 5 {
                out.push(format!("providers[{index}].models kept more than five"));
            }
        }
    }
}

/// Names of allowlist breaks in one scrubbed document. Empty means the file is clean.
pub fn allowlist_violations(file_name: &str, document: &Value) -> Vec<String> {
    let mut out = Vec::new();
    let Some(map) = document.as_object() else {
        out.push("document is not an object".to_owned());
        return out;
    };
    unexpected_keys(
        map,
        &["method", "path_without_query", "status", "body"],
        "",
        &mut out,
    );
    if let Some(body) = map.get("body") {
        walk_value(file_name, body, "", &mut out);
    } else {
        out.push("body is missing".to_owned());
    }
    out
}
