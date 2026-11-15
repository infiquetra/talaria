//! Ensure doctor tests derive the pinned Hermes tag and commit from `PINNED`.

use std::fs;
use std::path::Path;

fn quoted_strings(source: &str) -> Vec<String> {
    let bytes = source.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        if i + 1 < bytes.len() && bytes[i] == b'/' && bytes[i + 1] == b'/' {
            while i < bytes.len() && bytes[i] != b'\n' {
                i += 1;
            }
            continue;
        }
        if i + 1 < bytes.len() && bytes[i] == b'/' && bytes[i + 1] == b'*' {
            i += 2;
            while i + 1 < bytes.len() && !(bytes[i] == b'*' && bytes[i + 1] == b'/') {
                i += 1;
            }
            i += 2;
            continue;
        }
        if bytes[i] == b'"' {
            i += 1;
            let start = i;
            while i < bytes.len() {
                if bytes[i] == b'\\' {
                    i += 2;
                    continue;
                }
                if bytes[i] == b'"' {
                    break;
                }
                i += 1;
            }
            out.push(source[start..i].to_owned());
            i += 1;
            continue;
        }
        i += 1;
    }
    out
}

#[test]
fn doctor_literals_come_from_the_pin() {
    let crate_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let pinned = fs::read_to_string(crate_dir.join("../hermes-contract/PINNED"))
        .expect("PINNED file is readable");
    let tag = pinned
        .lines()
        .find_map(|line| line.strip_prefix("tag="))
        .expect("PINNED has tag=")
        .trim();
    let bare_tag = tag.strip_prefix('v').unwrap_or(tag);
    let commit = pinned
        .lines()
        .find_map(|line| line.strip_prefix("commit="))
        .expect("PINNED has commit=")
        .trim();
    let commit_prefix = &commit[..8];

    let doctor_source = fs::read_to_string(crate_dir.join("tests/integration/doctor_diagnose.rs"))
        .expect("doctor_diagnose.rs is readable");

    let mut violations = Vec::new();
    for literal in quoted_strings(&doctor_source) {
        if literal.contains(tag) || literal.contains(bare_tag) || literal.contains(commit_prefix) {
            violations.push(literal);
        }
    }
    assert!(
        violations.is_empty(),
        "doctor_diagnose.rs contains hardcoded pin literals: {violations:?}"
    );
}
