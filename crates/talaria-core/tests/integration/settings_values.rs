use serde_json::{Value, json};
use talaria_core::settings::{FieldKind, FieldView, ValueKind, parse_input, value_kind};

fn field(kind: FieldKind) -> FieldView {
    FieldView {
        key: "display.example".into(),
        kind,
        description: String::new(),
        category: "display".into(),
        options: None,
        searchable: false,
        clearable: false,
    }
}

#[test]
fn value_kind_follows_current_then_default_then_schema() {
    let number = field(FieldKind::Number);
    assert_eq!(
        value_kind(&number, Some(&json!(1)), Some(&json!(1.5)), None),
        ValueKind::Integer
    );
    assert_eq!(
        value_kind(&number, Some(&Value::Null), Some(&json!(1.5)), None),
        ValueKind::Float
    );
    assert_eq!(
        value_kind(&field(FieldKind::Boolean), None, None, None),
        ValueKind::Bool
    );
    assert_eq!(
        value_kind(&field(FieldKind::Select), None, None, None),
        ValueKind::Choice
    );
    assert_eq!(
        value_kind(
            &field(FieldKind::String),
            Some(&Value::Null),
            Some(&Value::Null),
            None
        ),
        ValueKind::Nullable
    );
    let list = field(FieldKind::List);
    assert_eq!(
        value_kind(&list, Some(&json!(["a", "b"])), None, None),
        ValueKind::StringList
    );
    assert_eq!(
        value_kind(&list, Some(&json!(["a,b"])), None, None),
        ValueKind::JsonList
    );
    assert_eq!(
        value_kind(&list, Some(&json!([1, 2])), None, None),
        ValueKind::JsonList
    );
}

#[test]
fn parse_input_by_kind() {
    let f = field(FieldKind::String);
    for (s, v) in [
        ("true", true),
        ("ON", true),
        ("yes", true),
        ("1", true),
        ("false", false),
        ("off", false),
        ("NO", false),
        ("0", false),
    ] {
        assert_eq!(
            parse_input(&ValueKind::Bool, &f, s, None).unwrap(),
            json!(v)
        );
    }
    assert!(parse_input(&ValueKind::Bool, &f, "maybe", None).is_err());
    for (s, v) in [("-0", 0), ("+5", 5), ("-42", -42)] {
        assert_eq!(
            parse_input(&ValueKind::Integer, &f, s, None).unwrap(),
            json!(v)
        );
    }
    for s in ["1.5", "9223372036854775808", "", "1e3"] {
        assert!(
            parse_input(&ValueKind::Integer, &f, s, None).is_err(),
            "{s}"
        );
    }
    assert_eq!(
        parse_input(&ValueKind::Float, &f, "1e3", None).unwrap(),
        json!(1000.0)
    );
    for s in ["NaN", "inf", "-inf"] {
        assert!(parse_input(&ValueKind::Float, &f, s, None).is_err());
    }
    assert_eq!(
        parse_input(&ValueKind::Text, &f, "single line", None).unwrap(),
        json!("single line")
    );
    assert!(parse_input(&ValueKind::Text, &f, "a\nb", None).is_err());
    assert_eq!(
        parse_input(&ValueKind::StringList, &f, " a, ,b ", None).unwrap(),
        json!(["a", "b"])
    );
    assert_eq!(
        parse_input(&ValueKind::JsonList, &f, "[1,2]", None).unwrap(),
        json!([1, 2])
    );
    assert!(parse_input(&ValueKind::JsonList, &f, "not an array", None).is_err());
    assert_eq!(
        parse_input(&ValueKind::JsonList, &f, "[3,4]", Some(&json!([1, 2]))).unwrap(),
        json!([3, 4])
    );
    assert!(
        parse_input(
            &ValueKind::JsonList,
            &f,
            "[3,\"four\"]",
            Some(&json!([1, 2]))
        )
        .is_err()
    );
    assert_eq!(
        parse_input(&ValueKind::JsonList, &f, "[]", Some(&json!([1, 2]))).unwrap(),
        json!([])
    );
    assert!(parse_input(&ValueKind::JsonList, &f, "[5]", Some(&json!(["a,b"]))).is_err());

    assert_eq!(
        parse_input(&ValueKind::Nullable, &f, "", None).unwrap(),
        Value::Null
    );
    assert_eq!(
        parse_input(&ValueKind::Nullable, &f, "true", None).unwrap(),
        json!(true)
    );
    assert_eq!(
        parse_input(&ValueKind::Nullable, &f, "hello", None).unwrap(),
        json!("hello")
    );
    let mut choice = field(FieldKind::Select);
    choice.options = Some(vec![json!("one"), json!(3)]);
    choice.clearable = true;
    assert_eq!(
        parse_input(&ValueKind::Choice, &choice, "3", None).unwrap(),
        json!(3)
    );
    assert_eq!(
        parse_input(&ValueKind::Choice, &choice, "", None).unwrap(),
        json!("")
    );
    assert!(parse_input(&ValueKind::Choice, &choice, "unknown", None).is_err());
}

fn keyed(key: &str, kind: FieldKind, options: Option<Vec<Value>>) -> FieldView {
    FieldView {
        key: key.into(),
        options,
        ..field(kind)
    }
}

#[test]
fn a_string_key_with_a_built_in_list_becomes_a_choice() {
    let text = keyed("browser.dialog_policy", FieldKind::String, None);
    assert_eq!(
        value_kind(&text, Some(&json!("must_respond")), None, None),
        ValueKind::Choice
    );
    let nullable = keyed("computer_use.no_overlay", FieldKind::String, None);
    assert_eq!(
        value_kind(&nullable, Some(&Value::Null), Some(&Value::Null), None),
        ValueKind::Choice
    );
}

#[test]
fn plain_string_key_without_spec_stays_text() {
    let region = keyed("bedrock.region", FieldKind::String, None);
    assert_eq!(
        value_kind(&region, Some(&json!("example-region")), None, None),
        ValueKind::Text
    );
}

#[test]
fn modal_mode_command_accepts_direct_and_refuses_sandbox() {
    let modal = keyed(
        "terminal.modal_mode",
        FieldKind::Select,
        Some(vec![json!("sandbox"), json!("function")]),
    );
    assert_eq!(
        parse_input(&ValueKind::Choice, &modal, "direct", Some(&json!("auto"))).unwrap(),
        json!("direct")
    );
    let refused = parse_input(&ValueKind::Choice, &modal, "sandbox", Some(&json!("auto")));
    assert_eq!(
        refused.unwrap_err().message,
        "Choose one of: auto, managed, direct."
    );
}

#[test]
fn the_current_value_is_accepted_even_when_unlisted() {
    let backup = keyed("updates.pre_update_backup", FieldKind::String, None);
    assert_eq!(
        parse_input(
            &ValueKind::Choice,
            &backup,
            "legacy",
            Some(&json!("legacy"))
        )
        .unwrap(),
        json!("legacy")
    );
    assert!(parse_input(&ValueKind::Choice, &backup, "legacy", Some(&json!("off"))).is_err());
}

#[test]
fn context_engine_accepts_compressor_and_a_plugin_name() {
    let engine = keyed(
        "context.engine",
        FieldKind::Select,
        Some(vec![json!("default"), json!("custom")]),
    );
    assert_eq!(
        parse_input(&ValueKind::Choice, &engine, "compressor", None).unwrap(),
        json!("compressor")
    );
    assert_eq!(
        parse_input(&ValueKind::Choice, &engine, "example-engine", None).unwrap(),
        json!("example-engine")
    );
}

#[test]
fn null_default_choice_saves_json() {
    let overlay = keyed("computer_use.no_overlay", FieldKind::String, None);
    assert_eq!(
        parse_input(&ValueKind::Choice, &overlay, "true", None).unwrap(),
        Value::Bool(true)
    );
    assert_eq!(
        parse_input(&ValueKind::Choice, &overlay, "false", None).unwrap(),
        Value::Bool(false)
    );
    assert_eq!(
        parse_input(&ValueKind::Choice, &overlay, "", None).unwrap(),
        Value::Null
    );
    let effort = keyed("x_search.reasoning_effort", FieldKind::String, None);
    assert_eq!(
        parse_input(&ValueKind::Choice, &effort, "high", None).unwrap(),
        json!("high")
    );
}

#[test]
fn x_search_effort_refuses_values_its_reader_rejects() {
    // Hermes's x_search tool accepts only its own four efforts, not the general effort list.
    let effort = keyed("x_search.reasoning_effort", FieldKind::String, None);
    for refused in ["minimal", "max", "ultra"] {
        assert_eq!(
            parse_input(&ValueKind::Choice, &effort, refused, None)
                .unwrap_err()
                .message,
            "Choose one of: \"\", low, medium, high, xhigh.",
            "{refused}"
        );
    }
}

#[test]
fn schema_options_merge_with_the_built_in_list() {
    let provider = keyed(
        "tts.provider",
        FieldKind::Select,
        Some(vec![json!("edge"), json!("example-command")]),
    );
    for name in ["deepinfra", "example-command", "edge"] {
        assert_eq!(
            parse_input(&ValueKind::Choice, &provider, name, None).unwrap(),
            json!(name)
        );
    }
}
