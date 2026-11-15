use std::process::Command;

#[test]
fn installed_binary_reports_its_version() {
    let mut command = Command::new(env!("CARGO_BIN_EXE_talaria"));
    command
        .arg("--version")
        .env_clear()
        .env("PATH", "/usr/bin:/bin");
    if let Ok(profile) = std::env::var("LLVM_PROFILE_FILE") {
        command.env("LLVM_PROFILE_FILE", profile);
    }
    let output = command
        .output()
        .unwrap_or_else(|_| panic!("the talaria binary could not be started"));
    let stdout = String::from_utf8_lossy(&output.stdout);
    let current = hermes_contract::compat::CURRENT;
    let expected = format!(
        "talaria {}\nhermes: pinned {} ({}), supported from {}, tested to {}\n",
        env!("CARGO_PKG_VERSION"),
        current.pinned_tag,
        &current.pinned_commit[..8],
        current.floor,
        current.newest_tested,
    );
    assert_eq!(stdout.as_ref(), expected, "stdout");
    assert_eq!(output.status.code(), Some(0), "exit code");
}
