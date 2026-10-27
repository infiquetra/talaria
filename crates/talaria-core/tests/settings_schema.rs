use serde_json::json;
use talaria_core::settings::{ConfigSchemaView, SettingsInput, SettingsSnapshot, SettingsState};

#[test]
fn categories_in_hermes_order_then_alphabetical() {
    let text = include_str!("../../../fixtures/rest/config-schema.json");
    let schema = ConfigSchemaView::from_json_str(text).expect("schema");
    let keys: Vec<_> = schema
        .fields
        .iter()
        .take(5)
        .map(|f| f.key.as_str())
        .collect();
    assert_eq!(
        keys,
        [
            "model",
            "model_context_length",
            "fallback_providers",
            "fallback.min_switch_reset_seconds",
            "toolsets"
        ]
    );
    let text = r#"{"fields":{"z.item":{"category":"zeta"},"a.item":{"category":"alpha"},"d.item":{"category":"display"},"g.item":{"category":"general"}},"category_order":["general","display","missing"]}"#;
    let schema = ConfigSchemaView::from_json_str(text).expect("schema");
    let mut state = SettingsState::new(&[]);
    state.update(SettingsInput::Open {
        profile: "fixture".into(),
        filter: None,
    });
    state.update(SettingsInput::Loaded(SettingsSnapshot {
        profile: "fixture".into(),
        schema,
        effective: json!({}),
        raw: json!({}),
        defaults: json!({}),
    }));
    assert_eq!(
        state.view().categories,
        ["general", "display", "alpha", "zeta"]
    );
}
