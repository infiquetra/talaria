use std::process::Command;
#[test]
fn skill_names_only_real_commands_and_flags() {
    let executable = env!("CARGO_BIN_EXE_talaria");
    for (path, flags) in [
        (
            vec!["workflow", "roster"],
            vec![
                "--profile",
                "--staffing",
                "--budget-tokens",
                "--budget-usd",
                "--json",
            ],
        ),
        (
            vec!["workflow", "skill", "install"],
            vec!["--profile", "--command", "--force", "--dry-run", "--json"],
        ),
        (
            vec!["workflow", "skill", "status"],
            vec!["--profile", "--json"],
        ),
        (
            vec!["workflow", "skill", "uninstall"],
            vec!["--profile", "--force", "--json"],
        ),
    ] {
        let output = Command::new(executable)
            .args(&path)
            .arg("--help")
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}: {}",
            path.join(" "),
            String::from_utf8_lossy(&output.stderr)
        );
        let help = String::from_utf8(output.stdout).unwrap();
        for flag in flags {
            assert!(help.contains(flag), "{}: missing {flag}", path.join(" "));
        }
    }
}

#[cfg(unix)]
#[test]
fn discovery_never_runs_ensure() {
    use std::{fs, os::unix::fs::PermissionsExt};
    let tmp = tempfile::tempdir().unwrap();
    let home = tmp.path().join("hermes");
    let config = tmp.path().join("talaria");
    let bin = tmp.path().join("bin");
    let log = tmp.path().join("ensure.log");
    fs::create_dir_all(home.join("profiles/fixture")).unwrap();
    fs::create_dir_all(&bin).unwrap();
    let fake = bin.join("hermes");
    fs::write(
        &fake,
        format!("#!/bin/sh\nprintf invoked >> '{}'\n", log.display()),
    )
    .unwrap();
    fs::set_permissions(fake, fs::Permissions::from_mode(0o755)).unwrap();
    for args in [
        vec![
            "workflow",
            "skill",
            "install",
            "--profile",
            "fixture",
            "--dry-run",
        ],
        vec!["workflow", "roster", "--profile", "fixture", "--json"],
    ] {
        let output = Command::new(env!("CARGO_BIN_EXE_talaria"))
            .args(args)
            .env("HERMES_HOME", &home)
            .env("TALARIA_CONFIG_DIR", &config)
            .env("PATH", &bin)
            .output()
            .unwrap();
        let text = String::from_utf8_lossy(&output.stdout);
        assert!(text.contains("gateway not running") || text.contains("gateway_unavailable"));
        assert!(!log.exists(), "gateway discovery invoked hermes");
    }
}
#[test]
fn json_output_golden() {
    let tmp = tempfile::tempdir().unwrap();
    let home = tmp.path().join("hermes");
    let config = tmp.path().join("talaria");
    std::fs::create_dir_all(home.join("profiles/fixture")).unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_talaria"))
        .args([
            "workflow",
            "skill",
            "install",
            "--profile",
            "fixture",
            "--dry-run",
            "--json",
        ])
        .env("HERMES_HOME", &home)
        .env("TALARIA_CONFIG_DIR", &config)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let value: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(value["version"], 1);
    assert_eq!(value["profile"], "fixture");
    assert_eq!(value["state"], "missing");
    assert_eq!(value["action"], "write");
    assert_eq!(value["skill_version_embedded"], "1.0.0");
    assert!(value["skill_version_installed"].is_null());
    assert!(value["backup"].is_null());
    assert_eq!(value["changed_files"], serde_json::json!([]));
    assert!(
        !home
            .join("profiles/fixture/skills/talaria-workflows")
            .exists()
    );
    assert!(!config.join("skills/installs.json").exists());
}
