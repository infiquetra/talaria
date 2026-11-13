//! Subagent route, effort, and chain rules.

#![allow(clippy::too_many_lines)]

use serde_json::{Map, Value};
use talaria_core::model_roles::{
    ChainMode, FallbackEntry, RoleRoute, SubagentRoles, subagent_patch,
};
use talaria_core::models::Effort;

fn roles(
    route: RoleRoute,
    effort: Option<Effort>,
    chain: ChainMode,
    endpoint: Option<&str>,
) -> SubagentRoles {
    let mut endpoint_keys = Map::new();
    if let Some(url) = endpoint {
        endpoint_keys.insert("base_url".to_owned(), Value::String(url.to_owned()));
        endpoint_keys.insert(
            "api_key".to_owned(),
            Value::String("${EXAMPLE_API_KEY}".to_owned()),
        );
        endpoint_keys.insert("api_mode".to_owned(), Value::String("chat".to_owned()));
    }
    SubagentRoles {
        route,
        effort,
        chain,
        direct_endpoint: endpoint.map(str::to_owned),
        endpoint_keys,
    }
}

fn keys(patch: &Value) -> Vec<String> {
    patch["delegation"]
        .as_object()
        .expect("delegation object")
        .keys()
        .cloned()
        .collect()
}

#[test]
fn route_and_endpoint_reset() {
    let before = roles(
        RoleRoute::Inherit,
        None,
        ChainMode::Inherit,
        Some("https://endpoint.invalid"),
    );
    let pinned = roles(
        RoleRoute::Pinned {
            provider: "ollama-cloud".to_owned(),
            model: "minimax-m3.1-flash".to_owned(),
        },
        None,
        ChainMode::Inherit,
        None,
    );
    let patch = subagent_patch(&before, &pinned);
    let mut names = keys(&patch);
    names.sort();
    assert_eq!(
        names,
        vec!["api_key", "api_mode", "base_url", "model", "provider"]
    );
    assert_eq!(patch["delegation"]["provider"], "ollama-cloud");
    assert_eq!(patch["delegation"]["model"], "minimax-m3.1-flash");
    assert_eq!(patch["delegation"]["base_url"], "");
    assert_eq!(patch["delegation"]["api_key"], "");
    assert_eq!(patch["delegation"]["api_mode"], "");

    let inherit = roles(RoleRoute::Inherit, None, ChainMode::Inherit, None);
    let cleared = subagent_patch(&before, &inherit);
    assert_eq!(cleared["delegation"]["provider"], "");
    assert_eq!(cleared["delegation"]["model"], "");
    assert_eq!(cleared["delegation"]["base_url"], "");
    assert_eq!(cleared["delegation"]["api_key"], "");
    assert_eq!(cleared["delegation"]["api_mode"], "");
    assert_eq!(keys(&cleared).len(), 5);
}

#[test]
fn chain_modes_and_effort() {
    let before = roles(RoleRoute::Inherit, None, ChainMode::Inherit, None);
    let none = roles(RoleRoute::Inherit, None, ChainMode::Disabled, None);
    let patch = subagent_patch(&before, &none);
    assert!(patch["delegation"]["fallback_providers"].is_array());
    assert!(
        patch["delegation"]["fallback_providers"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    assert!(
        patch["delegation"].get("model").is_none(),
        "an unchanged route is absent"
    );

    let own_entry = FallbackEntry {
        provider: "ollama-cloud".to_owned(),
        model: "minimax-m3.1-flash".to_owned(),
        was_string: false,
        extra: Map::new(),
    };
    let own = roles(
        RoleRoute::Inherit,
        None,
        ChainMode::Own(vec![own_entry]),
        None,
    );
    let listed = subagent_patch(&before, &own);
    assert_eq!(
        listed["delegation"]["fallback_providers"][0]["provider"],
        "ollama-cloud"
    );

    let back = subagent_patch(&own, &before);
    assert!(back["delegation"]["fallback_providers"].is_null());

    let effort = roles(
        RoleRoute::Inherit,
        Some(Effort::Low),
        ChainMode::Inherit,
        None,
    );
    let with_effort = subagent_patch(&before, &effort);
    assert_eq!(with_effort["delegation"]["reasoning_effort"], "low");
    let cleared = roles(RoleRoute::Inherit, None, ChainMode::Inherit, None);
    let inherited = subagent_patch(&effort, &cleared);
    assert_eq!(inherited["delegation"]["reasoning_effort"], "");

    let same = subagent_patch(&before, &before);
    assert!(same["delegation"].as_object().expect("object").is_empty());
}

#[test]
fn own_chain_escape_sets_empty_list() {
    use talaria_core::model_roles::{
        Loaded, ModelRolesState, RawRoles, RolesInput, RolesKey, Section,
    };
    let (mut state, _) = ModelRolesState::new("fixture".to_owned());
    state.update(RolesInput::Loaded(Loaded::Raw(RawRoles {
        fallback_providers: Vec::new(),
        fallback_model: Vec::new(),
        delegation: serde_json::Map::new(),
        model_context_length: 0,
        agent_reasoning_effort: serde_json::Value::String(String::new()),
        has_moa_presets: false,
    })));
    state.update(RolesInput::Key(RolesKey::Section(Section::Subagents)));
    state.update(RolesInput::Key(RolesKey::ChainMode));
    state.update(RolesInput::Key(RolesKey::Escape));
    state.update(RolesInput::Key(RolesKey::Review));
    state.update(RolesInput::Fresh {
        section: Section::Subagents,
        reads: Vec::new(),
    });
    let effects = state.update(RolesInput::Key(RolesKey::Enter));
    let patch = effects.iter().find_map(|effect| match effect {
        talaria_core::model_roles::RolesEffect::PutConfig { patch, .. } => Some(patch),
        _ => None,
    });
    let patch = patch.expect("leaving the first own-chain pick stores an empty chain");
    assert!(
        patch["delegation"]["fallback_providers"]
            .as_array()
            .expect("list")
            .is_empty()
    );
}
