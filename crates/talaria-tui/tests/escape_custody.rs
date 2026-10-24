//! Only the terminal module may write escape sequences or call the terminal crate's cursor and
//! mode helpers; every other source file must stay free of them.

use std::fs;
use std::path::{Path, PathBuf};

/// Names the root to scan; a crate directory (holding `src/`) or a source directory.
const ROOT_VARIABLE: &str = "TALARIA_TUI_SCAN_ROOT";

/// The tokens that mark a direct write of escape sequences or a direct terminal call. They are
/// built from pieces so this file never contains them itself.
fn forbidden_tokens() -> [String; 7] {
    [
        concat!("\\", "x1b").to_string(),
        concat!("\\", "u{1b}").to_string(),
        concat!("\\", "033").to_string(),
        concat!("que", "ue!").to_string(),
        concat!("exe", "cute!").to_string(),
        concat!("crossterm::", "cursor").to_string(),
        concat!("crossterm::", "terminal").to_string(),
    ]
}

fn source_files(dir: &Path, found: &mut Vec<PathBuf>) {
    let mut entries: Vec<PathBuf> = fs::read_dir(dir)
        .unwrap_or_else(|error| panic!("cannot read {}: {error}", dir.display()))
        .map(|entry| entry.expect("directory entry").path())
        .collect();
    entries.sort();
    for path in entries {
        if path.is_dir() {
            source_files(&path, found);
        } else if path.extension().is_some_and(|extension| extension == "rs") {
            found.push(path);
        }
    }
}

/// Every `path:line: token` found under `root`, skipping the terminal module itself. Paths are
/// relative to `root` and use forward slashes.
fn violations(root: &Path) -> Vec<String> {
    let tokens = forbidden_tokens();
    let mut files = Vec::new();
    source_files(root, &mut files);
    assert!(
        !files.is_empty(),
        "no source files under {}",
        root.display()
    );
    let mut found = Vec::new();
    for file in files {
        let relative = file
            .strip_prefix(root)
            .expect("file under root")
            .components()
            .map(|part| part.as_os_str().to_string_lossy().into_owned())
            .collect::<Vec<_>>()
            .join("/");
        if relative == "terminal.rs" {
            continue;
        }
        let text = fs::read_to_string(&file)
            .unwrap_or_else(|error| panic!("cannot read {}: {error}", file.display()));
        for (index, line) in text.lines().enumerate() {
            for token in &tokens {
                if line.contains(token.as_str()) {
                    found.push(format!("{relative}:{}: {token}", index + 1));
                }
            }
        }
    }
    found
}

fn scan_root() -> PathBuf {
    let root = match std::env::var_os(ROOT_VARIABLE) {
        Some(value) => PathBuf::from(value),
        None => PathBuf::from(env!("CARGO_MANIFEST_DIR")),
    };
    let source = root.join("src");
    if source.is_dir() { source } else { root }
}

fn copy_tree(from: &Path, to: &Path) {
    fs::create_dir_all(to).expect("create directory");
    for entry in fs::read_dir(from).expect("read directory") {
        let entry = entry.expect("directory entry");
        let target = to.join(entry.file_name());
        if entry.path().is_dir() {
            copy_tree(&entry.path(), &target);
        } else {
            fs::copy(entry.path(), &target).expect("copy file");
        }
    }
}

#[test]
fn only_terminal_module_writes_escape_sequences() {
    let root = scan_root();
    let found = violations(&root);
    assert!(
        found.is_empty(),
        "escape sequences or direct terminal calls outside terminal.rs:\n{}",
        found.join("\n")
    );
}

#[test]
fn escape_scan_reports_a_planted_violation() {
    let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let scratch = tempfile::tempdir().expect("scratch directory");
    let copy = scratch.path().join("src");
    copy_tree(&source, &copy);
    let tokens = forbidden_tokens();

    let live = copy.join("live.rs");
    let original = fs::read_to_string(&live).expect("read live.rs copy");
    let planted_at = original.lines().count() + 1;
    let escape = format!("const X: &str = \"{}x1b\";", '\\');
    assert!(
        escape.contains(tokens[0].as_str()),
        "run time escape literal matches the token"
    );
    let mut planted = original;
    if !planted.ends_with('\n') {
        planted.push('\n');
    }
    planted.push_str(&escape);
    planted.push('\n');
    fs::write(&live, planted).expect("write live.rs copy");

    let found = violations(&copy);
    let expected = format!("live.rs:{planted_at}: {}", tokens[0]);
    assert!(
        found.contains(&expected),
        "expected {expected} among the reported violations, got {found:?}"
    );

    // A line holding each token is reported once per token, in a nested file too.
    let nested = copy.join("nested");
    fs::create_dir_all(&nested).expect("nested directory");
    let lines: Vec<String> = tokens.iter().map(|token| format!("// {token}")).collect();
    fs::write(nested.join("extra.rs"), lines.join("\n") + "\n").expect("write nested file");
    let found = violations(&copy);
    for (index, token) in tokens.iter().enumerate() {
        let expected = format!("nested/extra.rs:{}: {token}", index + 1);
        assert!(found.contains(&expected), "missing {expected} in {found:?}");
    }
}

#[test]
fn escape_scan_skips_the_terminal_module() {
    let scratch = tempfile::tempdir().expect("scratch directory");
    let root = scratch.path().join("src");
    fs::create_dir_all(&root).expect("directory");
    let tokens = forbidden_tokens();
    fs::write(root.join("terminal.rs"), format!("// {}\n", tokens[0])).expect("write terminal.rs");
    fs::write(root.join("lib.rs"), "// clean\n").expect("write lib.rs");
    assert_eq!(violations(&root), Vec::<String>::new());
}
