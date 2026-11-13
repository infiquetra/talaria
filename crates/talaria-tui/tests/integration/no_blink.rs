//! Blinking is impossible: no shipped source may name the blink modifiers or
//! write an SGR sequence that turns blink on. Behavior contract: issue 227.

use std::path::{Path, PathBuf};

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
}

/// Every Rust source file under `src/`, `examples/` and `benches/` of each crate.
fn shipped_sources(root: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    let mut stack = vec![root.join("crates")];
    while let Some(dir) = stack.pop() {
        let entries =
            std::fs::read_dir(&dir).unwrap_or_else(|_| panic!("cannot read {}", dir.display()));
        for entry in entries {
            let entry = entry.unwrap();
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
                continue;
            }
            if path.extension().and_then(|ext| ext.to_str()) != Some("rs") {
                continue;
            }
            let relative = path.strip_prefix(root).unwrap_or(&path);
            let mut parts = relative.components();
            let in_scope = matches!(
                (
                    parts.next(),
                    parts.next(),
                    parts.next(),
                    parts.next()
                ),
                (
                    Some(std::path::Component::Normal(top)),
                    Some(std::path::Component::Normal(_)),
                    Some(std::path::Component::Normal(area)),
                    _,
                ) if top.to_str() == Some("crates")
                    && matches!(area.to_str(), Some("src" | "examples" | "benches"))
            );
            if in_scope {
                files.push(path);
            }
        }
    }
    files.sort();
    files
}

/// True when `text` (source text, so escapes are written out) holds an SGR
/// sequence that turns blink on: an escape introducer (`\x1b`, `\u{1b}`,
/// `\033` or `\e` as written) followed by `[`, digits and semicolons, and
/// `m`, with a parameter equal to `5` or `6` outside a colour selector.
/// `38`, `48` or `58` start a colour selector: the next parameter is
/// skipped, then one more if it was `5`, three more if it was `2`.
fn writes_blink_sgr(text: &str) -> bool {
    const INTRODUCERS: [&str; 4] = ["\\x1b", "\\u{1b}", "\\033", "\\e"];
    let bytes = text.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        let Some(rest) = text.get(index..) else {
            index += 1;
            continue;
        };
        let introducer = INTRODUCERS.iter().find(|needle| rest.starts_with(*needle));
        let Some(introducer) = introducer else {
            index += 1;
            continue;
        };
        let mut end = index + introducer.len();
        if bytes.get(end) != Some(&b'[') {
            index += 1;
            continue;
        }
        end += 1;
        let params_start = end;
        while end < bytes.len() && (bytes[end].is_ascii_digit() || bytes[end] == b';') {
            end += 1;
        }
        if bytes.get(end) != Some(&b'm') {
            index += 1;
            continue;
        }
        let params: Vec<&str> = text[params_start..end].split(';').collect();
        let mut slot = 0;
        while slot < params.len() {
            let param = params[slot];
            if matches!(param, "38" | "48" | "58") {
                slot += 1;
                if slot >= params.len() {
                    break;
                }
                let kind = params[slot];
                slot += 1;
                if kind == "5" {
                    slot += 1;
                } else if kind == "2" {
                    slot += 3;
                }
                continue;
            }
            if param == "5" || param == "6" {
                return true;
            }
            slot += 1;
        }
        index = end + 1;
    }
    false
}

#[test]
fn no_source_uses_blink() {
    let root = workspace_root();
    let files = shipped_sources(&root);
    assert!(!files.is_empty(), "the walk found shipped sources");
    let mut offenders = Vec::new();
    for path in &files {
        let text = std::fs::read_to_string(path).unwrap();
        for token in ["SLOW_BLINK", "RAPID_BLINK", "SlowBlink", "RapidBlink"] {
            if text.contains(token) {
                offenders.push(format!("{}: names {token}", path.display()));
            }
        }
        if writes_blink_sgr(&text) {
            offenders.push(format!("{}: writes a blink SGR sequence", path.display()));
        }
    }
    assert!(
        offenders.is_empty(),
        "blinking is impossible:\n{}",
        offenders.join("\n")
    );
}

#[test]
fn blink_sgr_walker_spots_forms() {
    for guilty in ["\\x1b[5m", "\\x1b[1;5m", "\\u{1b}[0;6;31m"] {
        assert!(writes_blink_sgr(guilty), "{guilty:?} turns blink on");
    }
    for innocent in ["\\x1b[15m", "\\x1b[1;25m", "\\x1b[38;5;208m"] {
        assert!(!writes_blink_sgr(innocent), "{innocent:?} is not blink");
    }
}
