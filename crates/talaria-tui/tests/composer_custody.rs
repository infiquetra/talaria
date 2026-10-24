//! The composer and the key map stay free of terminal writes and other crates.

use std::fs;
use std::path::{Path, PathBuf};

/// Findings read `<file name relative to path>: <token>`.
///
/// A directory is walked for `.rs` files. One file is read as itself. Each file is cut at
/// the first `#[cfg(test)]` line, and `//` comments are removed, before the tokens are sought.
fn scan(path: &Path) -> Vec<String> {
    let _ = path;
    todo!()
}

#[test]
fn composer_code_is_terminal_and_hermes_free() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut hits = scan(&root.join("composer"));
    hits.extend(scan(&root.join("keys.rs")));
    assert!(hits.is_empty(), "{hits:?}");
}

#[test]
fn scanner_finds_planted_violations() {
    let dir = tempfile::tempdir().unwrap();
    let clean = dir.path().join("clean.rs");
    fs::write(&clean, "fn ready() {}\n").unwrap();
    assert!(scan(&clean).is_empty(), "clean file");

    let noted = dir.path().join("note.rs");
    let mut comment = String::from("// ");
    comment.push_str(&forbidden()[0]);
    comment.push('\n');
    fs::write(&noted, comment).unwrap();
    assert!(scan(&noted).is_empty(), "a comment is not a hit");

    let hidden = dir.path().join("hidden.rs");
    let mut later = String::from("fn ready() {}\n#[cfg(test)]\nfn hidden() {\n");
    later.push_str(&forbidden()[0]);
    later.push_str("\n}\n");
    fs::write(&hidden, later).unwrap();
    assert!(
        scan(&hidden).is_empty(),
        "code after the test gate is not scanned"
    );

    for (index, token) in forbidden().into_iter().enumerate() {
        let file = dir.path().join(format!("bad-{index}.rs"));
        fs::write(&file, format!("fn bad() {{ {token} }}\n")).unwrap();
        let hits = scan(dir.path());
        let name = file.file_name().unwrap().to_string_lossy();
        assert!(
            hits.iter().any(|hit| hit == &format!("{name}: {token}")),
            "missing {token} in {hits:?}"
        );
    }
}

fn forbidden() -> Vec<String> {
    let byte = 0x1b;
    vec![
        concat!("cross", "term::cursor").to_string(),
        concat!("cross", "term::terminal").to_string(),
        concat!("queue", "!").to_string(),
        concat!("execute", "!").to_string(),
        concat!("hermes_", "client").to_string(),
        concat!("hermes_", "contract").to_string(),
        concat!("talaria_", "core").to_string(),
        concat!("talaria_", "app").to_string(),
        concat!("talaria_", "cli").to_string(),
        concat!("tok", "io").to_string(),
        concat!("uns", "afe").to_string(),
        format!("\\x{byte:x}"),
        format!("\\u{{{byte:x}}}"),
        format!("\\0{}", "33"),
    ]
}
