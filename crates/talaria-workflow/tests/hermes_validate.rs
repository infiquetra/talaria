#[path = "hermes_common/mod.rs"]
mod common;

use proptest::prelude::*;
use talaria_workflow::hermes::{
    brief, correction_message, extract_json_candidate, validate_output,
};

#[test]
fn output_contract_matches_hermes_wording() {
    let schema = common::read_json("schemas/finding.json");
    let got = brief("Report one finding.", Some(&schema));
    let want = std::fs::read_to_string(common::data("golden/contract_block.txt")).unwrap();
    assert_eq!(got, want, "contract block differs");
    assert_eq!(brief("leave me", None), "leave me");
}

#[test]
fn extract_json_candidate_cases() {
    let fenced = "```json\n{\"a\":1}\n```";
    let cases = [
        (r#"{"a":1}"#, r#"{"a":1}"#),
        (fenced, r#"{"a":1}"#),
        (r#"Here you go: {"a":1} thanks"#, r#"{"a":1}"#),
        (r#"[{"a":1},{"a":2}]"#, r#"[{"a":1},{"a":2}]"#),
        (r#"{"a": [1,2]}"#, r#"{"a": [1,2]}"#),
        (r#"[1, {"a": oops}"#, r#"{"a": oops}"#),
        ("no json here", "no json here"),
        ("", ""),
    ];
    for (input, expected) in cases {
        assert_eq!(extract_json_candidate(input), expected, "{input}");
    }
}

#[test]
fn errors_sorted_and_capped_at_ten() {
    let schema = common::read_json("schemas/many_types.json");
    let instance = serde_json::json!({
        "a": 1, "b": 1, "c": [1, 1], "d": {"n": 1},
        "e": 1, "f": 1, "g": 1, "h": 1, "i": 1, "j": 1, "k": 1
    });
    let errors = validate_output(&instance.to_string(), &schema).unwrap_err();
    assert_eq!(errors.len(), 10, "{errors:?}");
    let paths: Vec<&str> = errors
        .iter()
        .map(|line| line.split(':').next().unwrap_or(""))
        .collect();
    assert_eq!(
        paths,
        [
            "$.a", "$.b", "$.c[0]", "$.c[1]", "$.d.n", "$.e", "$.f", "$.g", "$.h", "$.i",
        ]
    );
    assert!(!paths.contains(&"$.j") && !paths.contains(&"$.k"));
    for line in &errors {
        assert!(line.contains("type"), "{line}");
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(8))]
    #[test]
    fn valid_instances_always_pass(title in "[A-Za-z]{1,12}") {
        let schema = common::read_json("schemas/finding.json");
        let text = serde_json::json!({"title": title, "severity": "low"}).to_string();
        let value = validate_output(&text, &schema).unwrap();
        prop_assert_eq!(value["title"].as_str(), Some(title.as_str()));
        prop_assert_eq!(value["severity"].as_str(), Some("low"));
    }
}

#[test]
fn correction_message_matches_hermes_wording() {
    let got = correction_message(&["$.a: first error".into(), "$.b[0]: second error".into()]);
    let want = std::fs::read_to_string(common::data("golden/correction.txt")).unwrap();
    assert_eq!(got, want, "correction wording differs");
    assert!(
        !got.contains("\"type\""),
        "schema text leaked into the correction"
    );
}
