use serde_json::json;
use std::collections::BTreeMap;
use talaria_core::settings::{
    ConfigSchemaView, FieldKind, FieldView, SettingsSnapshot, build_patch, review_rows,
};
fn field(key: &str, kind: FieldKind) -> FieldView {
    FieldView {
        key: key.into(),
        kind,
        description: String::new(),
        category: "general".into(),
        options: None,
        searchable: false,
        clearable: false,
    }
}
#[test]
fn patch_is_nested_and_only_changed_keys() {
    let changes = BTreeMap::from([
        ("display.compact".to_owned(), json!(true)),
        ("display.skin".to_owned(), json!("mono")),
        ("agent.max_turns".to_owned(), json!(12)),
    ]);
    let patch = build_patch(&changes);
    assert_eq!(
        patch,
        json!({"display":{"compact":true,"skin":"mono"},"agent":{"max_turns":12}})
    );
    assert!(patch["display"].get("theme").is_none());
    assert_eq!(build_patch(&BTreeMap::new()), json!({}));
}
#[test]
fn review_rows_mask_secrets_and_warn_on_emptied_lists() {
    let snapshot = SettingsSnapshot {
        profile: "fixture".into(),
        schema: ConfigSchemaView {
            fields: vec![
                field("service.api_key", FieldKind::String),
                field("toolsets", FieldKind::List),
                field("other_list", FieldKind::List),
            ],
            category_order: vec!["general".into()],
        },
        effective: json!({"service":{"api_key":"old-secret"},"toolsets":["web"],"other_list":["a"]}),
        raw: json!({}),
        defaults: json!({}),
    };
    let changes = BTreeMap::from([
        ("service.api_key".into(), json!("new-secret")),
        ("toolsets".into(), json!([])),
        ("other_list".into(), json!([])),
    ]);
    let rows = review_rows(&snapshot, &changes);
    let secret = rows
        .iter()
        .find(|r| r.key == "service.api_key")
        .expect("secret row");
    assert_eq!(secret.old, "••••");
    assert_eq!(secret.new, "•••• (changed)");
    assert!(!format!("{} {}", secret.old, secret.new).contains("secret"));
    let toolsets = rows
        .iter()
        .find(|r| r.key == "toolsets")
        .expect("toolsets row");
    assert_eq!(
        toolsets.warnings,
        ["Empties this list.", "The agent loses every toolset."]
    );
    let other = rows
        .iter()
        .find(|r| r.key == "other_list")
        .expect("list row");
    assert_eq!(other.warnings, ["Empties this list."]);
}
