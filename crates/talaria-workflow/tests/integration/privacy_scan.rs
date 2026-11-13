use hermes_client::{ReplaySource, local_forbidden_literals, scan_for_secrets, scan_literals};
use serde_json::Value;
use std::path::PathBuf;

fn check(value: &Value, key: &str) {
    match value {
        Value::String(text) => {
            if ["profile", "profile_id", "profile_name"].contains(&key) {
                assert!(
                    ["fixture", "default", "<profile>"].contains(&text.as_str()),
                    "foreign profile"
                );
            }
            assert!(
                !text.starts_with("/Users/")
                    && !text.starts_with("/home/")
                    && !text.starts_with("/root/")
                    && !text.starts_with("/private/var/")
                    && !text.starts_with("/var/folders/")
                    && !text.to_ascii_lowercase().contains("c:\\users\\"),
                "private path"
            );
        }
        Value::Array(items) => {
            for item in items {
                check(item, "");
            }
        }
        Value::Object(items) => {
            for (name, item) in items {
                check(item, name);
            }
        }
        _ => {}
    }
}

#[test]
fn recorded_roster_is_private_and_replayable() {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/frames/p5-04-model-options.jsonl");
    let text = std::fs::read_to_string(&path).expect("recorded roster fixture");
    assert!(
        !text.contains("http") && !text.contains('@'),
        "address or URL in recording"
    );
    assert!(scan_for_secrets(&text).is_empty(), "secret-shaped content");
    assert!(
        scan_literals(&text, &local_forbidden_literals()).is_empty(),
        "local identity"
    );
    for line in text.lines() {
        check(&serde_json::from_str(line).expect("valid JSON"), "");
    }
    let source = ReplaySource::open(&path).expect("valid replay");
    assert!(
        source
            .outbound()
            .any(|f| f.frame["method"] == "model.options"),
        "roster request missing"
    );
}
