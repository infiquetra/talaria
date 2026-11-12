use std::path::{Path, PathBuf};
use std::process::Command;

fn workspace_root() -> PathBuf {
    PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/../.."))
}

const REMOVED_PATHS: &[&str] = &[
    "talaria/",
    "tests/",
    "scripts/",
    "src/",
    "package.json",
    "package-lock.json",
    "tsconfig.json",
    "vitest.config.ts",
    "pyproject.toml",
    "uv.lock",
    ".python-version",
    "docs/acceptance/",
    ":(exclude)docs/acceptance/v0.7.0/",
    "docs/plans/",
    "docs/reviews/",
    "docs/code-reviews/",
    "docs/work-sessions/",
    "docs/brainstorms/",
    "docs/ideation/",
    "docs/evidence/",
    "docs/design/",
    "docs/releases/",
    "docs/examples/",
    "GEMINI.md",
    "IDEA.md",
    ".prettierignore",
    ".github/copilot-instructions.md",
    ".github/workflows/validate.yml",
];

const BASELINE_COMMANDS: &[&str] = &[
    "cargo fmt --check",
    "cargo clippy --all-targets -- -D warnings",
    "cargo test --workspace",
    "cargo llvm-cov --workspace --ignore-filename-regex /generated/ --fail-under-lines 80",
    "git diff --check origin/main...HEAD",
];

fn expected_profile() -> serde_json::Value {
    serde_json::json!({
        "schema": "repository_profile.v1",
        "repo": "infiquetra/talaria",
        "visibility": "public",
        "languages": ["rust"],
        "concurrency_allocation": 10,
        "nonproduction_destination": "none",
        "main_consumed_directly": false,
        "functional_test_environment": {
            "kind": "local",
            "test_command": "cargo test --workspace",
            "scope": "private"
        },
        "mechanical_tool_baseline": BASELINE_COMMANDS,
        "preflight_checks": [
            "the plan cleared /doc-review with nothing above P3 open",
            "the work branch exists and is not the default branch"
        ],
        "qa": {
            "schema": "qa_profile.v1",
            "ceiling": {
                "max_duration_seconds": 600,
                "max_direct_cost": 0
            },
            "strategies": {
                "cli-smoke": {
                    "required": true,
                    "file_patterns": ["crates/talaria-cli/**"],
                    "commands": [
                        {
                            "name": "talaria-version",
                            "command": "cargo run --quiet --package talaria-cli -- --version"
                        }
                    ]
                }
            }
        }
    })
}

fn read_repo_file(root: &Path, relative: &str) -> String {
    assert!(root.join(relative).is_file(), "{relative} is absent");
    std::fs::read_to_string(root.join(relative))
        .unwrap_or_else(|_| panic!("{relative} could not be read"))
}

#[test]
fn python_and_node_toolchain_files_are_gone() {
    let root = workspace_root();
    let output = match Command::new("git")
        .arg("-C")
        .arg(&root)
        .arg("ls-files")
        .arg("--")
        .args(REMOVED_PATHS)
        .output()
    {
        Ok(output) => output,
        Err(_) => panic!("git could not run"),
    };
    assert!(output.status.success(), "git could not run");
    let listed = String::from_utf8_lossy(&output.stdout);
    assert!(
        listed.trim().is_empty(),
        "git ls-files still tracks Python and Node toolchain paths:\n{listed}"
    );
}

#[test]
fn saga_profile_declares_the_project_check() {
    let text = read_repo_file(&workspace_root(), ".saga-profile.json");
    let value: serde_json::Value = serde_json::from_str(&text)
        .unwrap_or_else(|_| panic!(".saga-profile.json is not valid JSON"));
    assert_eq!(value, expected_profile());
}

#[test]
fn ci_workflow_runs_every_baseline_command() {
    let text = read_repo_file(&workspace_root(), ".github/workflows/ci.yml");
    let mut missing = Vec::new();
    for command in BASELINE_COMMANDS {
        let present = text.lines().any(|line| {
            let trimmed = line.trim_start();
            let command_line = trimmed.strip_prefix("run: ").unwrap_or(trimmed);
            command_line == *command
        });
        if !present {
            missing.push(*command);
        }
    }
    assert!(
        missing.is_empty(),
        "ci.yml is missing baseline commands:\n{}",
        missing.join("\n")
    );
    assert!(
        text.contains("name: rust-check (${{ matrix.os }})"),
        "ci.yml is missing name: rust-check (${{ matrix.os }})"
    );
    assert!(
        text.contains("name: rust-coverage"),
        "ci.yml is missing name: rust-coverage"
    );
    assert!(
        text.contains("fetch-depth: 0"),
        "ci.yml is missing fetch-depth: 0"
    );
    for forbidden in ["uv ", "npm ", "pytest"] {
        assert!(!text.contains(forbidden), "ci.yml contains `{forbidden}`");
    }
}

const UBUNTU_CI_COMMANDS: &[&str] = &[
    "cargo fmt --check",
    "cargo clippy --all-targets -- -D warnings",
    "cargo test -p talaria-app --test review_timing -- --exact keystroke_budget_while_gh_sleeps --list | grep -Fx 'keystroke_budget_while_gh_sleeps: test'",
    "cargo test -p talaria-app --test review_timing -- --exact keystroke_budget_while_gh_sleeps --include-ignored",
    "cargo test --workspace --doc",
    "cargo llvm-cov --workspace --ignore-filename-regex /generated/ --fail-under-lines 80",
    "cargo test -p patterns-check --test boundaries -- --ignored clippy_catches_each_banned_interface",
    "git diff --check origin/main...HEAD",
    r#"! git grep -nE '^(<{7}|={7}|>{7})( |$)' -- . || (echo "conflict marker" && false)"#,
];

const BENCH_COMMANDS: &[&str] = &[
    "cargo build --release -p talaria-cli -p talaria-bench",
    "target/release/talaria-bench run --binary target/release/talaria --report bench-report.json",
    "cargo bench -p talaria-tui --bench pager",
    "cargo bench -p talaria-tui --bench session_picker",
    "cargo bench -p talaria-tui --bench fleet_view",
    "cargo bench -p talaria-core --bench background_output",
    "cargo bench -p talaria-tui --bench background_overlay",
    "cargo bench -p talaria-core --bench fleet_registry",
    "jq -e '.median.point_estimate < 1000000' target/criterion/fleet_update_200/new/estimates.json",
    "cargo bench -p talaria-core --bench subagent_tree",
    "cargo bench -p talaria-tui --bench subagent_panel",
];

const SHORT_LOOP_COMMANDS: &[&str] = &[
    "cargo fmt --check",
    "cargo clippy --all-targets -- -D warnings",
    "cargo test --workspace",
];

const MISSING_TOOL_MESSAGES: &[&str] = &[
    "prepush: missing cargo",
    "prepush: missing cargo-llvm-cov",
    "prepush: missing jq",
];

fn collapse_whitespace(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn is_folded_yaml_run_header(trimmed: &str) -> bool {
    let Some(rest) = trimmed.strip_prefix("run:") else {
        return false;
    };
    let rest = rest.trim_start();
    let Some(indicator) = rest.strip_prefix('>') else {
        return false;
    };
    indicator
        .chars()
        .all(|ch| matches!(ch, '-' | '+' | '0'..='9'))
}

fn read_folded_yaml_block(lines: &[&str], start: usize) -> (String, usize) {
    let base_indent = lines[start].len() - lines[start].trim_start().len();
    let mut parts = vec!["run:".to_string()];
    let mut i = start + 1;
    while i < lines.len() {
        let line = lines[i];
        let trimmed = line.trim();
        let indent = line.len() - line.trim_start().len();
        if !trimmed.is_empty() && indent <= base_indent {
            break;
        }
        if !trimmed.is_empty() {
            parts.push(trimmed.strip_suffix('\\').unwrap_or(trimmed).to_string());
        }
        i += 1;
    }
    (collapse_whitespace(&parts.join(" ")), i)
}

fn read_continued_line(lines: &[&str], start: usize) -> (String, usize) {
    let mut combined = String::new();
    let mut i = start;
    while i < lines.len() {
        let line = lines[i];
        if let Some(prefix) = line.strip_suffix('\\') {
            if !combined.is_empty() {
                combined.push(' ');
            }
            combined.push_str(prefix);
            i += 1;
        } else {
            if !combined.is_empty() {
                combined.push(' ');
            }
            combined.push_str(line);
            i += 1;
            break;
        }
    }
    (collapse_whitespace(&combined), i)
}

fn normalize_text_lines(text: &str, fold_yaml_run: bool) -> Vec<String> {
    let raw_lines: Vec<&str> = text.lines().collect();
    let mut logical = Vec::new();
    let mut i = 0;

    while i < raw_lines.len() {
        let line = raw_lines[i];
        let trimmed = line.trim();
        if trimmed.starts_with('#') {
            i += 1;
            continue;
        }
        if fold_yaml_run && is_folded_yaml_run_header(trimmed) {
            let (folded, next_i) = read_folded_yaml_block(&raw_lines, i);
            logical.push(folded);
            i = next_i;
            continue;
        }
        let (joined, next_i) = read_continued_line(&raw_lines, i);
        if !joined.is_empty() {
            logical.push(joined);
        }
        i = next_i;
    }

    logical
}

fn contains_missing_tool_message(text: &str, message: &str) -> bool {
    let mut rest = text;
    while let Some(pos) = rest.find(message) {
        let after = &rest[pos + message.len()..];
        let at_boundary = after
            .chars()
            .next()
            .is_none_or(|ch| !ch.is_ascii_alphanumeric() && ch != '-' && ch != '_');
        if at_boundary {
            return true;
        }
        rest = after;
    }
    false
}

#[test]
fn prepush_script_matches_the_ubuntu_pull_request_checks() {
    let root = workspace_root();
    let script = read_repo_file(&root, "tools/prepush.sh");
    let ci = read_repo_file(&root, ".github/workflows/ci.yml");
    let bench = read_repo_file(&root, ".github/workflows/bench.yml");
    let agents_guide = read_repo_file(&root, "AGENTS.md");
    let claude_guide = read_repo_file(&root, "CLAUDE.md");

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = std::fs::metadata(root.join("tools/prepush.sh"))
            .unwrap_or_else(|error| panic!("tools/prepush.sh metadata could not be read: {error}"))
            .permissions()
            .mode();
        assert!(mode & 0o111 != 0, "tools/prepush.sh must be executable");
    }

    assert_eq!(
        script.lines().next(),
        Some("#!/usr/bin/env bash"),
        "tools/prepush.sh must start with #!/usr/bin/env bash"
    );

    let script_lines = normalize_text_lines(&script, false);
    let normalized_script = script_lines.join("\n");
    let normalized_ci = normalize_text_lines(&ci, true).join("\n");
    let normalized_bench = normalize_text_lines(&bench, true).join("\n");

    assert!(
        normalized_script.contains("set -euo pipefail"),
        "tools/prepush.sh must contain set -euo pipefail"
    );
    assert!(
        normalized_script.contains("git rev-parse --show-toplevel"),
        "tools/prepush.sh must contain git rev-parse --show-toplevel"
    );

    for command in UBUNTU_CI_COMMANDS {
        assert!(
            normalized_script.contains(command),
            "tools/prepush.sh is missing Ubuntu pull-request command: {command}"
        );
        assert!(
            normalized_ci.contains(command),
            ".github/workflows/ci.yml is missing Ubuntu pull-request command: {command}"
        );
    }

    for command in BENCH_COMMANDS {
        assert!(
            normalized_script.contains(command),
            "tools/prepush.sh is missing bench command: {command}"
        );
        assert!(
            normalized_bench.contains(command),
            ".github/workflows/bench.yml is missing bench command: {command}"
        );
    }

    for message in MISSING_TOOL_MESSAGES {
        assert!(
            contains_missing_tool_message(&normalized_script, message),
            "tools/prepush.sh is missing tool check message: {message}"
        );
    }

    let allowed_commands: Vec<&str> = UBUNTU_CI_COMMANDS
        .iter()
        .chain(BENCH_COMMANDS.iter())
        .copied()
        .collect();
    for line in &script_lines {
        let Some(first_word) = line.split_whitespace().next() else {
            continue;
        };
        if matches!(first_word, "cargo" | "git" | "python3" | "rustup" | "jq") {
            assert!(
                allowed_commands.contains(&line.as_str()),
                "tools/prepush.sh runs an unapproved `{first_word}` command: {line}"
            );
        }
    }

    for (path, guide) in [("AGENTS.md", &agents_guide), ("CLAUDE.md", &claude_guide)] {
        assert!(
            guide.contains("tools/prepush.sh"),
            "{path} must mention tools/prepush.sh"
        );
        assert!(
            !guide.contains("sh tools/prepush.sh"),
            "{path} must not contain `sh tools/prepush.sh`"
        );
        for command in SHORT_LOOP_COMMANDS {
            assert!(
                guide.lines().any(|line| line.trim() == *command),
                "{path} is missing short-loop command: {command}"
            );
        }
    }
}
