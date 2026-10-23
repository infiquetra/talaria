//! Test 40. No home path, temporary path, user-name segment, or foreign profile.
//!
//! Patterns are built at run time so this file does not contain them.

use serde_json::Value as Json;
use std::fs;
use std::path::{Path, PathBuf};

fn piece(chars: &[&str]) -> String {
    chars.concat()
}

fn forbidden_patterns() -> Vec<(&'static str, String)> {
    let slash = "/";
    let tilde = "~";
    vec![
        (
            "macos-home",
            format!("{slash}{}", piece(&["U", "s", "e", "r", "s"])),
        ),
        (
            "linux-home",
            format!("{slash}{}", piece(&["h", "o", "m", "e"])),
        ),
        (
            "private-temporary",
            format!(
                "{slash}{}{slash}{}",
                piece(&["p", "r", "i", "v", "a", "t", "e"]),
                piece(&["t", "m", "p"])
            ),
        ),
        (
            "variable-folders",
            format!(
                "{slash}{}{slash}{}",
                piece(&["v", "a", "r"]),
                piece(&["f", "o", "l", "d", "e", "r", "s"])
            ),
        ),
        ("temporary", format!("{slash}{}", piece(&["t", "m", "p"]))),
        ("tilde-home", format!("{tilde}{slash}")),
    ]
}

fn exempt_placeholder() -> String {
    let slash = "/";
    format!(
        "{slash}{}{slash}{}",
        piece(&["w", "o", "r", "k"]),
        piece(&["p", "r", "o", "j", "e", "c", "t"])
    )
}

fn collect_files(dir: &Path, files: &mut Vec<PathBuf>) {
    if !dir.exists() {
        return;
    }
    let entries =
        fs::read_dir(dir).unwrap_or_else(|error| panic!("read {}: {error}", dir.display()));
    for entry in entries {
        let path = entry.expect("dir entry").path();
        if path.is_dir() {
            // Bytecode caches are not source, and their files are not UTF-8.
            if path.file_name().and_then(|name| name.to_str()) == Some("__pycache__") {
                continue;
            }
            collect_files(&path, files);
        } else if path.is_file() {
            files.push(path);
        }
    }
}

fn scan_roots() -> Vec<PathBuf> {
    let crate_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let workspace = crate_dir
        .parent()
        .and_then(Path::parent)
        .expect("workspace root")
        .to_path_buf();
    vec![
        crate_dir.join("src").join("generated"),
        crate_dir.join("tests"),
        crate_dir.join("examples"),
        workspace.join("tools").join("contract-gen"),
    ]
}

fn walk_profiles(value: &Json, origin: &str, problems: &mut Vec<String>, depth: usize) {
    if depth > 6 {
        return;
    }
    match value {
        Json::Object(map) => {
            for (key, child) in map {
                if key == "profile" || key == "profile_id" || key == "profile_name" {
                    match child.as_str() {
                        Some("default") | Some("fixture") => {}
                        other => problems.push(format!("{origin} {key}={other:?}")),
                    }
                }
                walk_profiles(child, origin, problems, depth + 1);
            }
        }
        Json::Array(items) => {
            for item in items {
                walk_profiles(item, origin, problems, depth + 1);
            }
        }
        Json::String(text) => {
            if let Ok(nested) = serde_json::from_str::<Json>(text)
                && (nested.is_object() || nested.is_array())
            {
                walk_profiles(&nested, origin, problems, depth + 1);
            }
        }
        _ => {}
    }
}

fn user_segment_hits(text: &str) -> bool {
    let user = std::env::var("USER").unwrap_or_default();
    if user.chars().count() < 4 {
        return false;
    }
    let marker = format!("/{user}");
    let mut rest = text;
    while let Some(index) = rest.find(&marker) {
        let after = index + marker.len();
        match rest[after..].chars().next() {
            None | Some('/') => return true,
            Some(_) => rest = &rest[after..],
        }
    }
    false
}

#[test]
fn no_absolute_paths_or_foreign_profiles() {
    let patterns = forbidden_patterns();
    let exempt = exempt_placeholder();
    let mut files = Vec::new();
    for root in scan_roots() {
        collect_files(&root, &mut files);
    }
    // The drift workspace copy holds only the crate, so the generator is absent there.
    let crate_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    if crate_dir
        .parent()
        .and_then(Path::parent)
        .is_some_and(|workspace| workspace.join("tools").join("contract-gen").exists())
    {
        assert!(files.iter().any(|path| path.ends_with("contract_gen.py")));
    }
    assert!(files.iter().any(|path| path.ends_with("README.md")));

    let mut problems = Vec::new();
    for path in &files {
        let bytes =
            fs::read(path).unwrap_or_else(|error| panic!("read {}: {error}", path.display()));
        let text =
            String::from_utf8(bytes).unwrap_or_else(|_| panic!("{} is not utf-8", path.display()));
        let cleaned = text.replace(&exempt, "");
        for (label, pattern) in &patterns {
            if cleaned.contains(pattern) {
                problems.push(format!("{} contains {label}", path.display()));
            }
        }
        if user_segment_hits(&cleaned) {
            problems.push(format!(
                "{} contains the current user as a path segment",
                path.display()
            ));
        }
        if path.extension().and_then(|ext| ext.to_str()) == Some("json")
            || path.extension().and_then(|ext| ext.to_str()) == Some("jsonl")
        {
            if path.extension().and_then(|ext| ext.to_str()) == Some("jsonl") {
                for (index, line) in text.lines().enumerate() {
                    if line.is_empty() {
                        continue;
                    }
                    let value: Json = serde_json::from_str(line).unwrap_or_else(|error| {
                        panic!("{}:{} is not JSON: {error}", path.display(), index + 1)
                    });
                    walk_profiles(
                        &value,
                        &format!("{}:{}", path.display(), index + 1),
                        &mut problems,
                        0,
                    );
                }
            } else {
                let value: Json = serde_json::from_str(&text)
                    .unwrap_or_else(|error| panic!("{} is not JSON: {error}", path.display()));
                walk_profiles(&value, &path.display().to_string(), &mut problems, 0);
            }
        }
    }
    assert!(problems.is_empty(), "{}", problems.join("\n"));
}
