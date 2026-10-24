//! The composer and the key map stay free of terminal writes and other crates.

use std::fs;
use std::path::{Path, PathBuf};

/// Findings read `<file name relative to path>: <token>`.
///
/// A directory is walked for `.rs` files. One file is read as itself. Each file is cut at
/// the first `#[cfg(test)]` line, and `//` comments are removed, before the tokens are sought.
fn scan(path: &Path) -> Vec<String> {
    let mut files = Vec::new();
    if path.is_dir() {
        collect(path, &mut files);
    } else {
        files.push(path.to_path_buf());
    }
    files.sort();
    let tokens = forbidden();
    let mut hits = Vec::new();
    for file in files {
        let name = if path.is_dir() {
            file.strip_prefix(path)
                .unwrap_or(&file)
                .components()
                .map(|part| part.as_os_str().to_string_lossy().into_owned())
                .collect::<Vec<_>>()
                .join("/")
        } else {
            file.file_name()
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_default()
        };
        let text = fs::read_to_string(&file).unwrap_or_default();
        let code = code_of(&text);
        for token in &tokens {
            if contains_token(&code, token) {
                hits.push(format!("{name}: {token}"));
            }
        }
    }
    hits
}

/// Every `.rs` file under `dir`.
fn collect(dir: &Path, files: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect(&path, files);
        } else if path.extension().is_some_and(|extension| extension == "rs") {
            files.push(path);
        }
    }
}

/// The text before the first `#[cfg(test)]` line, with `//` comments removed.
fn code_of(text: &str) -> String {
    let mut code = String::new();
    for line in text.lines() {
        if line.trim() == "#[cfg(test)]" {
            break;
        }
        let kept = line.find("//").map_or(line, |at| &line[..at]);
        code.push_str(kept);
        code.push('\n');
    }
    code
}

/// The token appears, as a whole word where it begins or ends with a word character.
fn contains_token(code: &str, token: &str) -> bool {
    let word = |c: char| c.is_alphanumeric() || c == '_';
    let starts_word = token.chars().next().is_some_and(word);
    let ends_word = token.chars().last().is_some_and(word);
    code.match_indices(token).any(|(at, _)| {
        let before = code[..at].chars().next_back();
        let after = code[at + token.len()..].chars().next();
        !(starts_word && before.is_some_and(word)) && !(ends_word && after.is_some_and(word))
    })
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
