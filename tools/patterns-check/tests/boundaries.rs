//! Boundary scans for terminal-only calls, banned lint tables and the unattended flag.
//!
//! Planted files live under `env!("CARGO_TARGET_TMPDIR")`. The repository root is
//! `env!("CARGO_MANIFEST_DIR")` joined with `../..`. Reports use `path:line: token`
//! with `/` separators, relative to the scanned root.

use std::path::{Path, PathBuf};
use std::process::Command;

/// Relative paths allowed to contain terminal-only tokens.
/// Empty in the red run, so the allow-list test fails on this constant.
pub const TERMINAL_MODULE: &[&str] = &[];

/// Red-run stub. Reports nothing, so planted terminal, viewport and lint-table
/// violations fail the boundary tests.
///
/// The replacement walks `root` and returns `path:line: token` using `/`.
/// (a) In `crates/**/*.rs`, these tokens are allowed only on a [`TERMINAL_MODULE`]
/// path: `execute!`, `queue!`, `enable_raw_mode`, `disable_raw_mode`,
/// `EnterAlternateScreen`, `LeaveAlternateScreen`, `EventStream`,
/// `clippy::disallowed_methods`, `clippy::disallowed_types`.
/// (b) In `crates/**/*.rs`, nowhere: `Viewport::Inline`, `insert_before`, `Rc<RefCell`.
/// (c) In the root `Cargo.toml` and `crates/**/Cargo.toml`, nowhere:
/// `disallowed_methods`, `disallowed-methods`, `disallowed_types`, `disallowed-types`.
fn boundary_violations(root: &Path) -> Vec<String> {
    let _ = root;
    Vec::new()
}

/// Red-run stub. Reports nothing, so a planted quoted key fails the unattended test.
///
/// The replacement reports `"unattended"`, quotes included, in `crates/**/*.rs`.
/// A file is exempt when a path component is `tests`, or when `src` is immediately
/// followed by `generated`. Issue #197 REQ-16(d) exempts generated schema bindings.
/// That exemption is settled contradiction 2: both `tests/` and `src/generated/` are exempt.
fn unattended_violations(root: &Path) -> Vec<String> {
    let _ = root;
    Vec::new()
}

const BANNED_PATHS: &[&str] = &[
    "ratatui::Terminal::insert_before",
    "ratatui::Terminal::get_cursor_position",
    "ratatui::crossterm::cursor::position",
    "ratatui::init",
    "ratatui::try_init",
    "ratatui::init_with_options",
    "ratatui::try_init_with_options",
    "ratatui::run",
    "ratatui::restore",
    "ratatui::try_restore",
    "ratatui::DefaultTerminal",
];

const CANARY_SOURCE: &str = r#"pub fn touch_every_banned_interface() {
    let backend = ratatui::backend::TestBackend::new(10, 5);
    let mut terminal = ratatui::Terminal::new(backend).expect("backend");
    let _ = terminal.insert_before(1, |_buffer| {});
    let _ = terminal.get_cursor_position();
    let _ = ratatui::crossterm::cursor::position();
    let _ = crossterm::cursor::position();
    let options = ratatui::TerminalOptions {
        viewport: ratatui::Viewport::Fixed(ratatui::layout::Rect::new(0, 0, 10, 5)),
    };
    let _ = ratatui::init_with_options(options.clone());
    let _ = ratatui::try_init_with_options(options);
    let _ = ratatui::init();
    let _ = ratatui::try_init();
    ratatui::run(|_terminal| ());
    ratatui::restore();
    let _ = ratatui::try_restore();
    let _: Option<ratatui::DefaultTerminal> = None;
}
"#;

#[test]
fn terminal_calls_live_only_in_the_terminal_module() {
    let root = scratch("terminal_calls_live_only_in_the_terminal_module");
    plant(
        &root,
        "crates/x/src/lib.rs",
        "\
insert_before
Viewport::Inline
Rc<RefCell
#[expect(clippy::disallowed_methods)]
#[allow(dead_code, clippy::disallowed_types)]
",
    );
    plant(
        &root,
        "crates/x/Cargo.toml",
        "\
[lints.clippy]
disallowed_methods = \"allow\"
",
    );
    assert_reports(
        boundary_violations(&root),
        &[
            "crates/x/src/lib.rs:1: insert_before",
            "crates/x/src/lib.rs:2: Viewport::Inline",
            "crates/x/src/lib.rs:3: Rc<RefCell",
            "crates/x/src/lib.rs:4: clippy::disallowed_methods",
            "crates/x/src/lib.rs:5: clippy::disallowed_types",
            "crates/x/Cargo.toml:2: disallowed_methods",
        ],
        "planted boundary tokens were not reported",
    );
    assert_reports(
        boundary_violations(&repo_root()),
        &[],
        "repository boundary violations",
    );
}

#[test]
fn terminal_module_allow_list_is_terminal_rs_only() {
    assert_eq!(
        TERMINAL_MODULE,
        ["crates/talaria-tui/src/terminal.rs"].as_slice(),
        "TERMINAL_MODULE must name only the terminal module"
    );
    let root = scratch("terminal_module_allow_list_is_terminal_rs_only");
    plant(&root, "crates/talaria-tui/src/terminal.rs", "execute!\n");
    plant(&root, "crates/talaria-tui/src/live.rs", "execute!\n");
    assert_reports(
        boundary_violations(&root),
        &["crates/talaria-tui/src/live.rs:1: execute!"],
        "execute! is allowed only in the terminal module",
    );
}

#[test]
fn no_code_sends_the_unattended_flag() {
    let root = scratch("no_code_sends_the_unattended_flag");
    let line = "let _ = \"unattended\";\n";
    plant(&root, "crates/x/src/lib.rs", line);
    plant(&root, "crates/x/tests/t.rs", line);
    plant(&root, "crates/x/src/generated/g.rs", line);
    assert_reports(
        unattended_violations(&root),
        &["crates/x/src/lib.rs:1: \"unattended\""],
        "only a hand-written source file may be reported for the quoted key",
    );
    assert_reports(
        unattended_violations(&repo_root()),
        &[],
        "repository unattended violations",
    );
}

#[test]
#[ignore]
fn clippy_catches_each_banned_interface() {
    let root = scratch("clippy_catches_each_banned_interface");
    let ratatui = locked_version("ratatui");
    let crossterm = locked_version("crossterm");
    let manifest = format!(
        "\
[package]
name = \"ban-canary\"
version = \"0.0.0\"
edition = \"2024\"
publish = false

[workspace]

[dependencies]
ratatui = {{ version = \"={ratatui}\", features = [\"scrolling-regions\"] }}
crossterm = \"={crossterm}\"
"
    );
    plant(&root, "Cargo.toml", &manifest);
    plant(&root, "src/lib.rs", CANARY_SOURCE);
    copy_toolchain(&root);
    let offline = run_clippy(&root, true);
    let output = if needs_online(&offline) {
        run_clippy(&root, false)
    } else {
        offline
    };
    reject_if_unbuilt(&output);
    let missing: Vec<&str> = BANNED_PATHS
        .iter()
        .copied()
        .filter(|path| !output_names(&output, path))
        .collect();
    assert!(
        missing.is_empty(),
        "clippy output did not name: {}",
        missing.join(", ")
    );
    assert!(
        !output.contains("does not refer to a reachable"),
        "clippy reported a banned path that does not resolve"
    );
}

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn scratch(name: &str) -> PathBuf {
    let root = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(name);
    if root.exists() {
        std::fs::remove_dir_all(&root).unwrap_or_else(|error| {
            panic!("could not clear the planted directory ({error})");
        });
    }
    std::fs::create_dir_all(&root).unwrap_or_else(|error| {
        panic!("could not create the planted directory ({error})");
    });
    root
}

fn plant(root: &Path, relative: &str, body: &str) {
    let path = root.join(relative);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).unwrap_or_else(|error| {
            panic!("could not create a planted directory ({error})");
        });
    }
    std::fs::write(&path, body).unwrap_or_else(|error| {
        panic!("could not write a planted file ({error})");
    });
}

fn assert_reports(found: Vec<String>, expected: &[&str], what: &str) {
    let mut found = found;
    found.sort();
    let mut expected: Vec<String> = expected.iter().map(|line| (*line).to_string()).collect();
    expected.sort();
    assert_eq!(found, expected, "{what}");
}

fn not_run(reason: &str) -> ! {
    println!("NOT RUN: {reason}");
    panic!("NOT RUN: {reason}");
}

fn locked_version(name: &str) -> String {
    let text = std::fs::read_to_string(repo_root().join("Cargo.lock"))
        .unwrap_or_else(|_| not_run("Cargo.lock is missing"));
    let mut versions = Vec::new();
    let mut pending = None;
    let name_marker = format!("name = \"{name}\"");
    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("[[") {
            pending = None;
            continue;
        }
        if trimmed == name_marker {
            pending = Some(());
            continue;
        }
        if pending.is_some()
            && let Some(value) = trimmed
                .strip_prefix("version = \"")
                .and_then(|rest| rest.strip_suffix('"'))
        {
            versions.push(value.to_string());
            pending = None;
        }
    }
    if versions.len() != 1 {
        not_run(&format!("Cargo.lock does not list {name} exactly once"));
    }
    versions.remove(0)
}

fn copy_toolchain(canary: &Path) {
    let bytes = std::fs::read(repo_root().join("rust-toolchain.toml"))
        .unwrap_or_else(|_| not_run("rust-toolchain.toml is missing"));
    std::fs::write(canary.join("rust-toolchain.toml"), bytes).unwrap_or_else(|error| {
        not_run(&format!("could not write the toolchain file ({error})"));
    });
}

fn run_clippy(canary: &Path, offline: bool) -> String {
    let mut command = Command::new(env!("CARGO"));
    command.current_dir(canary).arg("clippy");
    if offline {
        command.arg("--offline");
    }
    command
        .arg("--")
        .arg("-D")
        .arg("warnings")
        .env("CLIPPY_CONF_DIR", repo_root())
        .env("CARGO_TARGET_DIR", canary.join("clippy-target"));
    let output = command.output().unwrap_or_else(|error| {
        not_run(&format!("cargo could not be started ({error})"));
    });
    let mut text = String::new();
    text.push_str(&String::from_utf8_lossy(&output.stdout));
    text.push_str(&String::from_utf8_lossy(&output.stderr));
    text
}

fn needs_online(text: &str) -> bool {
    if text.contains("use of a disallowed method") {
        return false;
    }
    text.contains("failed to download")
        || text.contains("failed to select a version")
        || text.contains("no matching package")
        || (text.contains("--offline") && text.contains("error"))
}

fn reject_if_unbuilt(text: &str) {
    if BANNED_PATHS.iter().all(|path| output_names(text, path)) {
        return;
    }
    if text.contains("no such command") || text.contains("is not installed") {
        not_run("clippy is not available");
    }
    if text.contains("failed to select a version")
        || text.contains("failed to download")
        || text.contains("no matching package")
    {
        not_run("dependencies could not be resolved");
    }
    if text.contains("error[E")
        || text.contains("could not compile")
        || text.contains("failed to parse manifest")
        || text.contains("failed to load manifest")
    {
        not_run("the canary crate did not compile");
    }
}

fn output_names(text: &str, path: &str) -> bool {
    text.contains(&format!("`{path}`"))
}
