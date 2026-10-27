use serde_json::json;
use std::collections::BTreeMap;
use talaria_core::memory::*;
use talaria_core::settings::SecretField;

#[test]
fn submitted_secrets_are_never_debugged() {
    let sentinel = "a-private-test-value";
    let values = SubmittedValues {
        plain: BTreeMap::new(),
        secrets: BTreeMap::from([("token".into(), SecretField::new(sentinel.into()))]),
    };
    assert!(!format!("{values:?}").contains(sentinel));
    let effect = MemoryEffect::SaveConfig {
        name: "provider".into(),
        surface: ConfigSurface::Plain,
        values,
    };
    assert!(!format!("{effect:?}").contains(sentinel));
}

fn field(kind: FieldKind, value: serde_json::Value) -> FieldView {
    FieldView {
        key: "key".into(),
        label: "Key".into(),
        kind,
        description: String::new(),
        placeholder: String::new(),
        required: false,
        value,
        is_set: true,
        options: vec![],
        when: None,
        minimum: None,
        maximum: None,
    }
}

#[test]
fn save_payload_holds_only_changes() {
    use std::time::Instant;
    let now = Instant::now();
    let mut s = MemoryState::new();
    s.update(
        MemoryInput::Open {
            profile: "fixture".into(),
        },
        now,
    );
    let provider = ProviderRowView {
        name: "provider".into(),
        description: String::new(),
        available: true,
        configured: true,
        status: "ready".into(),
        setup: ProviderSetup {
            pip_dependencies: vec![],
            python_dependencies_declared: false,
            external_dependencies: vec![],
            required_env: vec![],
            dependencies_installed: true,
        },
    };
    s.update(
        MemoryInput::StatusLoaded(Ok(StatusView {
            active: "provider".into(),
            providers: vec![provider],
            builtin_files: BuiltinFiles { memory: 0, user: 0 },
        })),
        now,
    );
    let mut unchanged = field(FieldKind::Text, json!("same"));
    unchanged.key = "unchanged".into();
    let mut secret = field(FieldKind::Secret, json!(null));
    secret.key = "secret".into();
    let mut flag = field(FieldKind::Bool, json!(false));
    flag.key = "flag".into();
    let key = |s: &mut MemoryState, k| s.update(MemoryInput::Key(k), now);
    key(&mut s, MemoryKey::Down);
    key(&mut s, MemoryKey::Open);
    s.update(
        MemoryInput::ConfigLoaded {
            name: "provider".into(),
            result: Ok(ConfigView {
                name: "provider".into(),
                label: "provider".into(),
                surface: ConfigSurface::Plain,
                fields: vec![unchanged, secret, flag],
            }),
        },
        now,
    );
    key(&mut s, MemoryKey::Open);
    key(&mut s, MemoryKey::Open);
    key(&mut s, MemoryKey::Down);
    key(&mut s, MemoryKey::Open);
    key(&mut s, MemoryKey::Open);
    key(&mut s, MemoryKey::Down);
    key(&mut s, MemoryKey::Open);
    key(&mut s, MemoryKey::Space);
    key(&mut s, MemoryKey::Open);
    let effects = key(&mut s, MemoryKey::Save);
    let [MemoryEffect::SaveConfig { values, .. }] = effects.as_slice() else {
        panic!("expected save")
    };
    assert_eq!(values.plain.len(), 1);
    assert_eq!(values.plain["flag"], json!(true));
    assert!(values.secrets.is_empty());
}

#[test]
fn byte_sizes_follow_desktop_rounding() {
    for (bytes, expected) in [
        (0, "empty"),
        (1, "1 B"),
        (1023, "1023 B"),
        (1024, "1.0 KB"),
        (1536, "1.5 KB"),
        (2304, "2.3 KB"),
        (1_048_575, "1024.0 KB"),
        (1_048_576, "1.0 MB"),
        (5_242_880, "5.0 MB"),
    ] {
        assert_eq!(format_bytes(bytes), expected);
    }
}

#[test]
fn validate_input_per_kind() {
    assert_eq!(
        validate_input(&field(FieldKind::Text, json!("")), "  text  "),
        Ok(json!("text"))
    );
    let mut integer = field(FieldKind::Integer, json!(0));
    integer.minimum = Some(1.0);
    integer.maximum = Some(5.0);
    assert_eq!(
        validate_input(&integer, "2.5"),
        Err(FieldError::NotWholeNumber)
    );
    assert_eq!(
        validate_input(&integer, "0"),
        Err(FieldError::BelowMin(1.0))
    );
    assert_eq!(
        validate_input(&integer, "6"),
        Err(FieldError::AboveMax(5.0))
    );
    let number = field(FieldKind::Number, json!(0));
    assert_eq!(validate_input(&number, "NaN"), Err(FieldError::NotNumber));
    let json_field = field(FieldKind::Json, json!({}));
    assert_eq!(
        validate_input(&json_field, "1"),
        Err(FieldError::NotJsonContainer)
    );
    assert!(validate_input(&json_field, "[1]").is_ok());
    let mut select = field(FieldKind::Select, json!("a"));
    select.options.push(FieldOption {
        value: "a".into(),
        label: "A".into(),
        description: String::new(),
    });
    assert_eq!(validate_input(&select, "b"), Err(FieldError::NotAnOption));
    assert!(validate_input(&field(FieldKind::Bool, json!(false)), "true").is_ok());
    assert_eq!(
        validate_input(&field(FieldKind::Other("custom".into()), json!(null)), "x"),
        Err(FieldError::ReadOnly("custom".into()))
    );
}

#[test]
fn when_controls_visibility() {
    let mut conditional = field(FieldKind::Text, json!(""));
    conditional.when = Some(json!({"mode":"cloud"}).as_object().unwrap().clone());
    assert!(!field_visible(
        &conditional,
        &BTreeMap::from([("mode".into(), "local".into())])
    ));
    assert!(field_visible(
        &conditional,
        &BTreeMap::from([("mode".into(), "cloud".into())])
    ));
}

#[test]
fn classify_saved_outcomes() {
    let before = field(FieldKind::Text, json!("old"));
    for (sent, after, expected) in [
        ("new", "new", SavedOutcome::Saved),
        ("new", "old", SavedOutcome::NotSaved),
        ("new", "adjusted", SavedOutcome::Adjusted("adjusted".into())),
        ("", "default", SavedOutcome::Cleared("default".into())),
        (" new ", "new", SavedOutcome::Saved),
    ] {
        assert_eq!(
            classify_saved(
                &before,
                &json!(sent),
                &before,
                &field(FieldKind::Text, json!(after))
            ),
            expected
        );
    }
    let mut secret_before = field(FieldKind::Secret, json!(null));
    secret_before.is_set = false;
    let mut secret_after = secret_before.clone();
    secret_after.is_set = true;
    assert_eq!(
        classify_saved(
            &secret_before,
            &json!("secret"),
            &secret_before,
            &secret_after
        ),
        SavedOutcome::SecretStored
    );
    assert_eq!(
        classify_saved(
            &secret_after,
            &json!("secret"),
            &secret_after,
            &secret_after
        ),
        SavedOutcome::SecretSetUnconfirmed
    );
    assert_eq!(
        classify_saved(
            &secret_before,
            &json!("secret"),
            &secret_before,
            &secret_before
        ),
        SavedOutcome::SecretNotStored
    );
}
