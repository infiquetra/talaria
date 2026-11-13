//! Mixture of Agents rules.

#![allow(clippy::too_many_lines)]

use serde_json::{Map, Value, json};
use talaria_core::model_roles::{
    Loaded, MoaConfigView, MoaPresetView, ModelRolesState, RolesInput, RolesKey, Section, SlotView,
    fanout_words as describe_fanout, validate_moa,
};

fn slot(provider: &str, model: &str) -> SlotView {
    SlotView {
        provider: provider.to_owned(),
        model: model.to_owned(),
        reasoning_effort: None,
        enabled: Some(true),
        extra: Map::new(),
    }
}

fn preset(name: &str, references: Vec<SlotView>, aggregator: SlotView) -> MoaPresetView {
    MoaPresetView {
        name: name.to_owned(),
        enabled: true,
        references,
        aggregator,
        reference_temperature: None,
        aggregator_temperature: None,
        reference_timeout: None,
        degraded_reference_policy: "loud".to_owned(),
        fanout: "user_turn".to_owned(),
        extra: Map::new(),
    }
}

fn config(presets: Vec<MoaPresetView>) -> MoaConfigView {
    let default_preset = presets
        .first()
        .map(|item| item.name.clone())
        .unwrap_or_default();
    MoaConfigView {
        default_preset,
        active_preset: String::new(),
        presets,
        privacy_filter: None,
    }
}

#[test]
fn validate_mirrors_hermes() {
    let empty = config(vec![preset("x", vec![slot("", "")], slot("", ""))]);
    assert_eq!(
        validate_moa(&empty),
        vec![
            "preset 'x' reference 1: provider and model are required".to_owned(),
            "preset 'x': needs at least one complete reference model".to_owned(),
            "preset 'x' aggregator: provider and model are required".to_owned(),
        ]
    );

    let missing_model = config(vec![preset(
        "x",
        vec![slot("ollama-cloud", "")],
        slot("public", "a"),
    )]);
    assert_eq!(
        validate_moa(&missing_model),
        vec![
            "preset 'x' reference 1: model is required (provider 'ollama-cloud' has no model selected)".to_owned(),
            "preset 'x': needs at least one complete reference model".to_owned(),
        ]
    );

    let missing_provider = config(vec![preset("x", vec![slot("", "a")], slot("public", "a"))]);
    assert_eq!(
        validate_moa(&missing_provider),
        vec![
            "preset 'x' reference 1: provider is required".to_owned(),
            "preset 'x': needs at least one complete reference model".to_owned(),
        ]
    );

    let recursive = config(vec![preset(
        "x",
        vec![slot("moa", "preset")],
        slot("public", "a"),
    )]);
    assert_eq!(
        validate_moa(&recursive),
        vec![
            "preset 'x' reference 1: the Mixture of Agents provider cannot be used inside a preset (recursive MoA)".to_owned(),
            "preset 'x': needs at least one complete reference model".to_owned(),
        ]
    );

    let no_references = config(vec![preset("x", Vec::new(), slot("", ""))]);
    assert_eq!(
        validate_moa(&no_references),
        vec![
            "preset 'x': needs at least one complete reference model".to_owned(),
            "preset 'x' aggregator: provider and model are required".to_owned(),
        ]
    );

    let ok = config(vec![preset(
        "x",
        vec![slot("public", "a")],
        slot("public", "b"),
    )]);
    assert!(validate_moa(&ok).is_empty());
}

fn loaded_moa(presets: Vec<MoaPresetView>, default_name: &str, active: &str) -> ModelRolesState {
    let (mut state, _) = ModelRolesState::new("fixture".to_owned());
    state.update(RolesInput::Loaded(Loaded::Moa(MoaConfigView {
        default_preset: default_name.to_owned(),
        active_preset: active.to_owned(),
        presets,
        privacy_filter: Some("off".to_owned()),
    })));
    state.update(RolesInput::Key(RolesKey::Section(Section::Moa)));
    state
}

#[test]
fn delete_rules() {
    let only = preset("only", vec![slot("public", "a")], slot("public", "b"));
    let mut state = loaded_moa(vec![only], "only", "");
    let effects = state.update(RolesInput::Key(RolesKey::Remove));
    assert!(effects.is_empty(), "refusing the last preset sends nothing");
    let text = state.view().notice.join("\n") + &modal_text(&state);
    assert!(text.contains("A profile keeps at least one preset."));
    assert!(view_text(&state).contains("only"));

    let mut state = loaded_moa(
        vec![
            preset("first", vec![slot("public", "a")], slot("public", "b")),
            preset("second", vec![slot("public", "a")], slot("public", "b")),
            preset("third", vec![slot("public", "a")], slot("public", "b")),
        ],
        "second",
        "second",
    );
    state.update(RolesInput::Key(RolesKey::Down));
    state.update(RolesInput::Key(RolesKey::Remove));
    let text = view_text(&state);
    assert!(text.contains("Delete preset second? y/n"));
    state.update(RolesInput::Key(RolesKey::Char('y')));
    let text = view_text(&state);
    assert!(
        text.contains("* ") && text.contains("first"),
        "the default moves to the first remaining preset"
    );
    assert!(
        text.contains("Active preset: none"),
        "deleting the active preset clears it"
    );
    assert!(!text.contains("second"));
}

fn view_text(state: &ModelRolesState) -> String {
    let view = state.view();
    let mut parts = vec![view.title];
    parts.extend(view.notice);
    parts.extend(view.footer);
    parts.extend(view.rows.into_iter().map(|row| row.text));
    if let Some(detail) = view.detail {
        parts.extend(detail.into_iter().map(|row| row.text));
    }
    if let Some(modal) = view.modal {
        parts.push(format!("{modal:?}"));
    }
    parts.join("\n")
}

fn modal_text(state: &ModelRolesState) -> String {
    view_text(state)
}

#[test]
fn fanout_words() {
    assert_eq!(describe_fanout("user_turn"), "once per user turn");
    assert_eq!(describe_fanout("junk"), "once per user turn");
    assert_eq!(describe_fanout("per_iteration"), "every tool step");
    assert_eq!(describe_fanout("every_n:1"), "every tool step");
    assert_eq!(describe_fanout("every_n:3"), "every 3 tool steps");
    assert_eq!(describe_fanout("every_n:12"), "every 12 tool steps");
}

#[test]
fn first_save_includes_example_and_warns() {
    let (mut state, _) = ModelRolesState::new("fixture".to_owned());
    state.update(RolesInput::Loaded(Loaded::Raw(
        talaria_core::model_roles::RawRoles {
            fallback_providers: Vec::new(),
            fallback_model: Vec::new(),
            delegation: Map::new(),
            model_context_length: 0,
            agent_reasoning_effort: Value::String(String::new()),
            has_moa_presets: false,
        },
    )));
    state.update(RolesInput::Loaded(Loaded::Moa(config(vec![preset(
        "default",
        vec![
            slot("openai-codex", "gpt-5.5"),
            slot("openrouter", "deepseek/deepseek-v4-pro"),
        ],
        slot("openrouter", "anthropic/claude-opus-4.8"),
    )]))));
    state.update(RolesInput::Key(RolesKey::Section(Section::Moa)));
    let text = view_text(&state);
    assert!(text.contains("No presets of your own."));
    assert!(text.contains("default"));
    state.update(RolesInput::Key(RolesKey::Review));
    let lines = talaria_core::model_roles::review_lines(&state);
    assert!(lines.iter().any(|line| line.before.contains("first time")
        || line.after.contains("first time")
        || format!("{line:?}").contains(
            "This writes Mixture of Agents presets into this profile for the first time."
        )));
    let text = view_text(&state);
    assert!(
        text.contains(
            "This writes Mixture of Agents presets into this profile for the first time."
        )
    );
    let _ = json!({"presets": {"default": {}}});
}

#[test]
fn new_preset_and_field_edits() {
    let mut state = loaded_moa(
        vec![preset(
            "alpha",
            vec![slot("public", "a")],
            slot("public", "b"),
        )],
        "alpha",
        "",
    );
    state.update(RolesInput::Key(RolesKey::Add));
    let text = view_text(&state);
    assert!(text.contains("Name:"), "adding a preset asks for a name");
    for ch in ['n', 'e', 'w'] {
        state.update(RolesInput::Key(RolesKey::Char(ch)));
    }
    state.update(RolesInput::Key(RolesKey::Enter));
    let text = view_text(&state);
    assert!(text.contains("new"));
    assert!(text.contains("Enabled: yes"));
    assert!(text.contains("once per user turn"));
    assert!(text.contains("say so (loud)") || text.contains("loud"));
    state.update(RolesInput::Key(RolesKey::Right));
    state.update(RolesInput::Key(RolesKey::Add));
    state.update(RolesInput::Key(RolesKey::Escape));
    let text = view_text(&state);
    assert!(
        text.contains("provider and model are required") || text.contains("References:"),
        "leaving the picker keeps the empty reference row"
    );
}
