//! Test row 14 of P1-04: the committed fixtures hold no secret-shaped string, no home directory,
//! login name or host name, and no foreign profile name (spec section 7).
//!
//! A failure prints the file name, the line number and the rule id. It never prints the matched
//! text, a path, a login name or a host name.

use std::path::PathBuf;

use hermes_client::{local_forbidden_literals, scan_for_secrets, scan_literals};
use regex::Regex;
use serde_json::Value;

fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join("fixtures/frames")
}

fn jsonl_files() -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(fixtures_dir()) else {
        return Vec::new();
    };
    let mut files: Vec<PathBuf> = entries
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "jsonl"))
        .collect();
    files.sort();
    files
}

/// Asserts the seven fixtures exist and returns `(file name, text)` for each, in name order.
fn fixture_texts() -> Vec<(String, String)> {
    let files = jsonl_files();
    assert!(
        files.len() == 7,
        "expected seven fixtures in fixtures/frames, found {}",
        files.len()
    );
    files
        .iter()
        .map(|path| {
            let name = path
                .file_name()
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_default();
            let text = std::fs::read_to_string(path)
                .unwrap_or_else(|_| panic!("{name} could not be read"));
            (name, text)
        })
        .collect()
}

/// Calls `visit(key, text)` for every string value under `value`; `key` is the object key the
/// string sits directly under, if any.
fn walk_strings(value: &Value, key: Option<&str>, visit: &mut dyn FnMut(Option<&str>, &str)) {
    match value {
        Value::String(text) => visit(key, text),
        Value::Array(items) => {
            for item in items {
                walk_strings(item, None, visit);
            }
        }
        Value::Object(map) => {
            for (name, inner) in map {
                walk_strings(inner, Some(name), visit);
            }
        }
        _ => {}
    }
}

/// Runs `visit` over every string of every line of every fixture and returns the findings as
/// `<file>:<line> <rule>`.
fn findings(rule_for: &dyn Fn(Option<&str>, &str) -> Option<&'static str>) -> Vec<String> {
    let mut found = Vec::new();
    for (name, text) in fixture_texts() {
        for (index, line) in text.lines().enumerate() {
            let Ok(value) = serde_json::from_str::<Value>(line) else {
                found.push(format!("{name}:{} not-json", index + 1));
                continue;
            };
            walk_strings(&value, None, &mut |key, string| {
                if let Some(rule) = rule_for(key, string) {
                    found.push(format!("{name}:{} {rule}", index + 1));
                }
            });
        }
    }
    found
}

#[test]
fn no_secret_shaped_strings() {
    let mut found = Vec::new();
    for (name, text) in fixture_texts() {
        for hit in scan_for_secrets(&text) {
            found.push(format!("{name}:{} {}", hit.line, hit.rule));
        }
    }
    assert!(
        found.is_empty(),
        "secret-shaped strings in the fixtures:\n{}",
        found.join("\n")
    );
}

/// The label of each local fact `local_forbidden_literals` is meant to include, with whether the
/// machine provides it at four characters or more. Values are never printed.
fn local_facts() -> [(&'static str, bool); 3] {
    let long_enough = |value: Option<String>| value.is_some_and(|text| text.chars().count() >= 4);
    [
        ("home", long_enough(std::env::var("HOME").ok())),
        (
            "login",
            long_enough(
                std::env::var("USER")
                    .ok()
                    .or_else(|| std::env::var("LOGNAME").ok()),
            ),
        ),
        (
            "host",
            long_enough(Some(
                gethostname::gethostname().to_string_lossy().into_owned(),
            )),
        ),
    ]
}

#[test]
fn no_home_login_name_or_host_name() {
    let texts = fixture_texts();
    for (label, available) in local_facts() {
        if !available {
            println!("SKIPPED: {label}");
        }
    }
    let literals = local_forbidden_literals();
    let mut found = Vec::new();
    for (name, text) in &texts {
        for hit in scan_literals(text, &literals) {
            found.push(format!("{name}:{} {}", hit.line, hit.rule));
        }
    }
    assert!(
        found.is_empty(),
        "a local home directory, login name or host name is in the fixtures:\n{}",
        found.join("\n")
    );
}

#[test]
fn only_allowed_profile_names() {
    const PROFILE_KEYS: [&str; 3] = ["profile", "profile_id", "profile_name"];
    const ALLOWED_VALUES: [&str; 3] = ["fixture", "default", "<profile>"];
    const ALLOWED_SEGMENTS: [&str; 2] = ["fixture", "<profile>"];
    let segment = Regex::new(r#"/profiles/([^/\s"'\\]+)"#).expect("profile segment pattern");
    let found = findings(&|key, text| {
        if key.is_some_and(|name| PROFILE_KEYS.contains(&name)) && !ALLOWED_VALUES.contains(&text) {
            return Some("profile-field-value");
        }
        let foreign = segment
            .captures_iter(text)
            .any(|capture| !ALLOWED_SEGMENTS.contains(&&capture[1]));
        foreign.then_some("profile-path-segment")
    });
    assert!(
        found.is_empty(),
        "profile names other than fixture, default or <profile>:\n{}",
        found.join("\n")
    );
}

#[test]
fn no_home_directory_shaped_paths() {
    let user_roots = Regex::new(r"^/(Users|home|root)(/|$)").expect("user directory root pattern");
    let temporary_roots =
        Regex::new(r"^/(private/var|var/folders)(/|$)").expect("temporary directory root pattern");
    let found = findings(&|_key, text| {
        if user_roots.is_match(text) {
            Some("user-directory-root")
        } else if temporary_roots.is_match(text) {
            Some("temporary-directory-root")
        } else if text.to_ascii_lowercase().contains(r"c:\users\") {
            Some("windows-user-directory")
        } else {
            None
        }
    });
    assert!(
        found.is_empty(),
        "home or temporary directory paths in the fixtures:\n{}",
        found.join("\n")
    );
}
