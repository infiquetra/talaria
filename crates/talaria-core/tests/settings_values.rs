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
