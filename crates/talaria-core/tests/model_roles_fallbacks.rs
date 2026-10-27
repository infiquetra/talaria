//! Fallback chain rules.

#![allow(clippy::too_many_lines)]

use serde_json::{Map, Value, json};
use talaria_core::model_roles::{
    FallbackEntry, Loaded, ModelRolesState, RawRoles, RolesInput, RolesKey, Section,
    fallback_patch, review_lines, route_identity,
};

fn entry(provider: &str, model: &str, extra: Value, was_string: bool) -> FallbackEntry {
    FallbackEntry {
        provider: provider.to_owned(),
        model: model.to_owned(),
        was_string,
        extra: extra.as_object().cloned().unwrap_or_default(),
    }
}

#[test]
fn extras_survive_edit_and_move() {
    let first = entry(
        "ollama-cloud",
        "one",
        json!({"base_url": "https://endpoint.invalid", "api_key": "${EXAMPLE_API_KEY}", "key_env": "EXAMPLE_KEY_ENV", "api_mode": "chat"}),
        false,
    );
    let second = entry("openrouter", "two", json!({}), false);
    let moved = fallback_patch(&[second.clone(), first.clone()]);
    let list = moved["fallback_providers"].as_array().expect("list");
    assert_eq!(list.len(), 2);
    assert_eq!(list[0]["provider"], "openrouter");
    assert_eq!(list[1]["provider"], "ollama-cloud");
    assert_eq!(list[1]["base_url"], "https://endpoint.invalid");
    assert_eq!(list[1]["api_key"], "${EXAMPLE_API_KEY}");
    assert_eq!(list[1]["key_env"], "EXAMPLE_KEY_ENV");
    assert_eq!(list[1]["api_mode"], "chat");
    assert!(moved.get("fallback_model").is_none());
}

#[test]
fn legacy_duplicates_and_main() {
    assert_eq!(
        route_identity("OpenRouter", "Model", "https://x/v1/"),
        route_identity("openrouter", "model", "https://x/v1")
    );
    assert_eq!(
        route_identity("  Ollama-Cloud ", " Mini ", " https://Endpoint.invalid/ "),
        (
            "ollama-cloud".to_owned(),
            "mini".to_owned(),
            "https://endpoint.invalid".to_owned()
        )
    );
    let (mut state, _) = ModelRolesState::new("fixture".to_owned());
    state.update(RolesInput::Loaded(Loaded::Raw(RawRoles {
        fallback_providers: vec![
            json!({"provider": "OpenRouter", "model": "A"}),
            json!({"provider": "openrouter", "model": "a", "base_url": "https://x/v1/"}),
            json!({"provider": "openrouter", "model": "a", "base_url": "https://x/v1"}),
        ],
        fallback_model: vec![json!({"provider": "openrouter", "model": "legacy"})],
        delegation: Map::new(),
        model_context_length: 0,
        agent_reasoning_effort: Value::String(String::new()),
        has_moa_presets: false,
    })));
    state.update(RolesInput::Loaded(Loaded::Info(
        talaria_core::model_roles::RolesInfo {
            model: "a".to_owned(),
            provider: "openrouter".to_owned(),
            auto_context_length: 0,
            config_context_length: 0,
            capabilities: Map::new(),
        },
    )));
    state.update(RolesInput::Key(RolesKey::Section(Section::Fallbacks)));
    let text = rows(&state);
    assert!(text.contains("duplicate: Hermes skips it"));
    assert!(text.contains("same as main model"));
    assert!(text.contains("Legacy entry (fallback_model): edit with the hermes fallback command"));
    state.update(RolesInput::Key(RolesKey::Remove));
    let text = rows(&state);
    assert!(
        text.contains("legacy"),
        "a legacy row cannot be removed from this page"
    );
}

fn rows(state: &ModelRolesState) -> String {
    state
        .view()
        .rows
        .into_iter()
        .map(|row| row.text)
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn string_entries_and_key_aliases() {
    let parsed = entry("openrouter", "a/b", json!({}), true);
    assert_eq!(
        (parsed.provider.as_str(), parsed.model.as_str()),
        ("openrouter", "a/b")
    );
    let patch = fallback_patch(&[
        entry("openrouter", "a/b", json!({}), true),
        entry(
            "custom",
            "m",
            json!({"api_key_env": "EXAMPLE_KEY_ENV"}),
            false,
        ),
    ]);
    let list = patch["fallback_providers"].as_array().expect("list");
    assert!(list[0].is_object(), "a string entry is saved as a mapping");
    assert_eq!(list[0]["provider"], "openrouter");
    assert_eq!(list[0]["model"], "a/b");
    assert_eq!(list[1]["api_key_env"], "EXAMPLE_KEY_ENV");
    let (mut state, _) = ModelRolesState::new("fixture".to_owned());
    state.update(RolesInput::Loaded(Loaded::Raw(RawRoles {
        fallback_providers: vec![json!("openrouter/a/b"), json!("nosh slash")],
        fallback_model: Vec::new(),
        delegation: Map::new(),
        model_context_length: 0,
        agent_reasoning_effort: Value::String(String::new()),
        has_moa_presets: false,
    })));
    state.update(RolesInput::Key(RolesKey::Section(Section::Fallbacks)));
    let text = rows(&state);
    assert!(text.contains("(ignored by Hermes until saved)"));
    state.update(RolesInput::Key(RolesKey::Review));
    let lines = review_lines(&state);
    let rendered = lines
        .iter()
        .map(|line| format!("{}  {} -> {}", line.key, line.before, line.after))
        .collect::<Vec<_>>()
        .join("\n");
    assert!(
        rendered.contains("Entry 1 was ignored by Hermes; saving makes it active.")
            || rows(&state).contains("Entry 1 was ignored by Hermes; saving makes it active.")
    );
}

#[test]
fn patch_is_whole_list() {
    let complete = [
        entry("one", "a", json!({}), false),
        entry(
            "two",
            "b",
            json!({"base_url": "https://endpoint.invalid"}),
            false,
        ),
        entry("three", "c", json!({}), false),
    ];
    let patch = fallback_patch(&complete);
    let list = patch["fallback_providers"].as_array().expect("whole list");
    assert_eq!(list.len(), 3);
    assert_eq!(list[0]["model"], "a");
    assert_eq!(list[1]["model"], "b");
    assert_eq!(list[2]["model"], "c");
    assert_eq!(list[1]["base_url"], "https://endpoint.invalid");

    let with_gap = [
        entry("one", "a", json!({}), false),
        entry("two", "b", json!({}), false),
        entry("", "", json!({}), false),
    ];
    let blocked = fallback_patch(&with_gap);
    let kept = blocked["fallback_providers"]
        .as_array()
        .expect("complete rows only");
    assert!(kept.iter().all(|row| {
        row["provider"]
            .as_str()
            .is_some_and(|text| !text.is_empty())
    }));
    assert!(
        kept.iter()
            .all(|row| row["model"].as_str().is_some_and(|text| !text.is_empty()))
    );

    let (mut state, _) = ModelRolesState::new("fixture".to_owned());
    state.update(RolesInput::Loaded(Loaded::Raw(RawRoles {
        fallback_providers: vec![
            json!({"provider": "one", "model": "a"}),
            json!({"provider": "two", "model": "b"}),
            json!({"provider": "three", "model": ""}),
        ],
        fallback_model: Vec::new(),
        delegation: Map::new(),
        model_context_length: 0,
        agent_reasoning_effort: Value::String(String::new()),
        has_moa_presets: false,
    })));
    state.update(RolesInput::Key(RolesKey::Section(Section::Fallbacks)));
    let effects = state.update(RolesInput::Key(RolesKey::Review));
    assert!(effects.is_empty(), "an incomplete row never opens a write");
    let text = rows(&state) + &state.view().notice.join("\n");
    assert!(text.contains("Entry 3 needs a model"));
}
