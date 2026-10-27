use serde_json::json;
use talaria_core::settings::{
    ConfigSchemaView, EditKey, FieldKind, FieldView, SettingsAction, SettingsEffect, SettingsInput,
    SettingsKey, SettingsSnapshot, SettingsState,
};

fn field(key: &str, description: &str, category: &str) -> FieldView {
    FieldView {
        key: key.into(),
        kind: FieldKind::Boolean,
        description: description.into(),
        category: category.into(),
        options: None,
        searchable: false,
        clearable: false,
    }
}
fn loaded() -> SettingsSnapshot {
    SettingsSnapshot {
        profile: "fixture".into(),
        schema: ConfigSchemaView {
            fields: vec![
                field(
                    "compression.micro_compact",
                    "Compress old entries",
                    "compression",
                ),
                field("display.compact", "Compact output", "display"),
                field("display.secondary", "Compact alternate view", "display"),
                field("general.other", "Unrelated setting", "general"),
            ],
            category_order: vec!["display".into(), "compression".into(), "general".into()],
        },
        effective: json!({"display":{"compact":false,"secondary":false},"compression":{"micro_compact":false},"general":{"other":false}}),
        raw: json!({}),
        defaults: json!({}),
    }
}
fn open(filter: Option<&str>) -> SettingsState {
    let mut state = SettingsState::new(&[]);
    assert_eq!(
        state.update(SettingsInput::Open {
            profile: "fixture".into(),
            filter: filter.map(str::to_owned)
        }),
        vec![SettingsEffect::Load {
            profile: "fixture".into()
        }]
    );
    assert!(state.update(SettingsInput::Loaded(loaded())).is_empty());
    state
}
fn keys(state: &SettingsState) -> Vec<String> {
    state.view().rows.iter().map(|r| r.key.clone()).collect()
}

#[test]
fn terms_match_key_description_category() {
    let state = open(Some("CoMpAcT"));
    assert_eq!(
        keys(&state),
        [
            "display.compact",
            "display.secondary",
            "compression.micro_compact"
        ]
    );
    assert!(!state.view().text_focus);
    assert_eq!(state.view().cursor, 0);

    let state = open(Some("display alternate"));
    assert_eq!(keys(&state), ["display.secondary"]);
    let state = open(Some("old COMPRESS"));
    assert_eq!(keys(&state), ["compression.micro_compact"]);
    let state = open(Some("display old"));
    assert!(keys(&state).is_empty());
    assert!(
        state
            .view()
            .banner
            .join("\n")
            .contains("No setting matches \"display old\".")
    );

    let mut state = open(None);
    assert!(
        state
            .update(SettingsInput::Key(SettingsKey::Action(
                SettingsAction::Filter
            )))
            .is_empty()
    );
    for c in "compact".chars() {
        assert!(
            state
                .update(SettingsInput::Key(SettingsKey::Char(c)))
                .is_empty()
        );
    }
    assert!(
        state
            .update(SettingsInput::Key(SettingsKey::Edit(EditKey::Enter)))
            .is_empty()
    );
    assert!(!state.view().text_focus);
    assert_eq!(state.view().cursor, 0);
    assert_eq!(keys(&state)[0], "display.compact");
    assert!(
        state
            .update(SettingsInput::Key(SettingsKey::Action(
                SettingsAction::Edit
            )))
            .is_empty()
    );
    assert_eq!(state.view().rows[0].value, "true");
}
