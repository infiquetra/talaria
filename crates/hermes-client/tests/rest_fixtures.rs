#[path = "support/mock_rest.rs"]
mod mock_rest;

use regex::Regex;
use serde_json::{Value, json};
use std::{fs, path::Path};

const REVIEWED_LONG_TOKENS: &[&str] = &[
    "allow_data_training_tiers_noninteractive",
    "voice_channel_inactivity_timeout_seconds",
];

fn check_profiles(value: &Value) {
    match value {
        Value::Object(map) => {
            for (key, value) in map {
                if key == "profile" {
                    assert!(value == "default" || value == "fixture");
                }
                check_profiles(value);
            }
        }
        Value::Array(items) => {
            for item in items {
                check_profiles(item);
            }
        }
        _ => {}
    }
}

fn check_folder(path: &Path, token: &Regex, email: &Regex) {
    for entry in fs::read_dir(path).expect("fixture folder") {
        let entry = entry.expect("fixture entry");
        let path = entry.path();
        if path.is_dir() {
            check_folder(&path, token, email);
            continue;
        }
        let text = fs::read_to_string(&path).expect("text fixture");
        for prefix in [
            concat!("/", "Users", "/"),
            concat!("/", "home", "/"),
            concat!("C:", "\\", "Users", "\\"),
            concat!("~", "/"),
        ] {
            assert!(!text.contains(prefix), "fixture contains a home prefix");
        }
        assert!(!email.is_match(&text), "fixture contains an email address");
        for found in token.find_iter(&text) {
            assert!(
                REVIEWED_LONG_TOKENS.contains(&found.as_str()),
                "unreviewed long token"
            );
        }
        if path.extension().is_some_and(|ext| ext == "json") {
            let value: Value = serde_json::from_str(&text).expect("valid fixture JSON");
            check_profiles(&value);
        }
    }
}

#[test]
fn fixtures_are_consistent_and_private() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/rest");
    let read = |name: &str| -> Value {
        serde_json::from_slice(&fs::read(root.join(name)).expect("fixture exists"))
            .expect("fixture JSON")
    };
    let schema = read("config-schema.json");
    let fields = schema["fields"].as_object().expect("fields");
    for (key, expected) in [
        (
            "tts.provider",
            json!([
                "edge",
                "elevenlabs",
                "openai",
                "xai",
                "minimax",
                "mistral",
                "gemini",
                "neutts",
                "kittentts",
                "piper"
            ]),
        ),
        (
            "stt.provider",
            json!([
                "local",
                "groq",
                "openai",
                "mistral",
                "xai",
                "elevenlabs",
                "deepinfra"
            ]),
        ),
        ("memory.provider", json!(["", "byterover", "retaindb"])),
        (
            "terminal.backend",
            json!([
                "local",
                "docker",
                "ssh",
                "modal",
                "daytona",
                "vercel_sandbox",
                "singularity"
            ]),
        ),
    ] {
        if let Some(field) = fields.get(key) {
            assert_eq!(field["options"], expected, "{key}");
        } else {
            assert_eq!(key, "stt.provider", "missing required field");
        }
    }
    check_folder(
        &root,
        &Regex::new(r"[A-Za-z0-9_\-]{40,}").unwrap(),
        &Regex::new(r"@[A-Za-z0-9.-]+\.[A-Za-z]{2,}").unwrap(),
    );
    let defaults = read("config-defaults.json");
    let raw = read("config-raw.json");
    let effective = read("config-effective.json");
    assert_eq!(mock_rest::effective_config(&defaults, &raw), effective);
}
