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
    "docs/install.md",
    "docs/themes.md",
    "docs/terminal-ui.md",
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
