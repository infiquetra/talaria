use std::{fs, path::Path, process::Command};

#[test]
fn no_terminal_or_hermes_dependencies() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let manifest_text = fs::read_to_string(root.join("Cargo.toml")).unwrap();
    let manifest: toml::Value = toml::from_str(&manifest_text).unwrap();
    let normal = manifest
        .get("dependencies")
        .and_then(toml::Value::as_table)
        .unwrap();
    for forbidden in ["ratatui", "crossterm", "talaria-tui", "hermes-client"] {
        assert!(
            !normal.contains_key(forbidden),
            "forbidden normal dependency {forbidden}"
        );
    }
    let output = Command::new("cargo")
        .args([
            "tree",
            "-p",
            "talaria-workflow",
            "-e",
            "normal",
            "--prefix",
            "none",
        ])
        .current_dir(root)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "cargo tree failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let tree = String::from_utf8(output.stdout).unwrap();
    for forbidden in ["ratatui", "crossterm", "talaria-tui", "hermes-client"] {
        assert!(
            !tree
                .lines()
                .any(|line| line.split_whitespace().next() == Some(forbidden)),
            "forbidden transitive dependency {forbidden}"
        );
    }
    fn scan(path: &Path) {
        if path.is_dir() {
            for entry in fs::read_dir(path).unwrap() {
                scan(&entry.unwrap().path());
            }
        } else if matches!(path.extension().and_then(|s| s.to_str()), Some("rs" | "js")) {
            let text = fs::read_to_string(path).unwrap();
            for forbidden in [
                "hermes_contract",
                "hermes_client",
                "ratatui",
                "crossterm",
                "talaria_tui",
            ] {
                assert!(
                    !text.contains(forbidden),
                    "forbidden engine dependency {forbidden} in {}",
                    path.display()
                );
            }
        }
    }
    for path in [
        root.join("src/engine"),
        root.join("src/mock.rs"),
        root.join("src/dry_run.rs"),
    ] {
        scan(&path);
    }
}
