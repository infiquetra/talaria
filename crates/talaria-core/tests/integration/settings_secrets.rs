use serde_json::{Value, json};
use talaria_core::settings::{
    FieldKind, FieldView, ReadOnlyReason, ValueKind, contains_secret_key, env_reference_names,
    is_secret_key, value_kind,
};
fn field(key: &str, kind: FieldKind) -> FieldView {
    FieldView {
        key: key.into(),
        kind,
        description: String::new(),
        category: String::new(),
        options: None,
        searchable: false,
        clearable: false,
    }
}
#[test]
fn secret_rule_matches_hermes() {
    for key in [
        "auxiliary.vision.api_key",
        "delegation.api_key",
        "x.password",
        "x.client_secret",
        "mcp_servers.s.headers.X-API-Key",
        "a.b_token",
        "x.jwt",
        "x.authorization",
        "x.private_key",
    ] {
        assert!(is_secret_key(key), "{key}");
    }
    for key in [
        "voice.record_key",
        "x.token_count",
        "mcp_servers.s.auth",
        "display.skin",
    ] {
        assert!(!is_secret_key(key), "{key}");
    }
    assert!(contains_secret_key(&json!([{"name":"a","api_key":"x"}])));
    assert!(!contains_secret_key(&json!(["a", "b"])));
}
#[test]
fn read_only_fields() {
    for key in [
        "model",
        "model_context_length",
        "fallback_providers",
        "agent.reasoning_effort",
        "delegation.base_url",
        "auxiliary.vision.api_key",
        "moa.enabled",
        "providers",
        "custom_providers",
    ] {
        assert_eq!(
            value_kind(
                &field(key, FieldKind::String),
                Some(&json!("a")),
                None,
                Some(&json!("${A}"))
            ),
            ValueKind::ReadOnly(ReadOnlyReason::OwnedElsewhere),
            "{key}"
        );
    }
    for (raw, names) in [
        (json!("${A}"), vec!["A"]),
        (json!("${env:A}"), vec!["A"]),
        (json!("prefix-${X}-${Y}"), vec!["X", "Y"]),
        (json!(["${X}", "${env:X}", "${Y}"]), vec!["X", "Y"]),
    ] {
        assert_eq!(env_reference_names(&raw), names);
        assert_eq!(
            value_kind(
                &field("display.example", FieldKind::String),
                Some(&json!("effective")),
                None,
                Some(&raw)
            ),
            ValueKind::ReadOnly(ReadOnlyReason::EnvReference(
                names.into_iter().map(str::to_owned).collect()
            ))
        );
    }
    assert_eq!(
        value_kind(
            &field("display.example", FieldKind::String),
            Some(&json!({"api_key":"x"})),
            None,
            None
        ),
        ValueKind::ReadOnly(ReadOnlyReason::HoldsSecret)
    );
    for (kind, name) in [
        (FieldKind::Text, "text"),
        (FieldKind::Object, "object"),
        (FieldKind::Other("future".into()), "future"),
    ] {
        assert_eq!(
            value_kind(
                &field("display.example", kind),
                Some(&Value::Null),
                None,
                None
            ),
            ValueKind::ReadOnly(ReadOnlyReason::Unsupported(name.into()))
        );
    }
}
