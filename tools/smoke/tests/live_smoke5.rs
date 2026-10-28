//! The real gateway smoke run is opt-in and fails outside Herdr.
mod support;
use std::process::Command;

#[test]
#[ignore]
fn live_phase5_smoke_in_herdr() {
    assert!(
        std::env::var("HERDR_ENV").is_ok(),
        "the live run fails outside Herdr"
    );
    let talaria = std::env::var_os("TALARIA_SMOKE_BIN")
        .map(std::path::PathBuf::from)
        .or_else(|| {
            std::env::var_os("CARGO_TARGET_DIR")
                .map(std::path::PathBuf::from)
                .map(|target| target.join("debug/talaria"))
        })
        .unwrap_or_else(|| {
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .ancestors()
                .nth(2)
                .unwrap()
                .join("target/debug/talaria")
        });
    assert!(talaria.is_file(), "build the binary before the live run");
    let root = tempfile::tempdir().unwrap();
    let output = Command::new(support::smoke_exe())
        .args(["run", "--suite", "phase5", "--talaria"])
        .arg(talaria)
        .env("TMPDIR", root.path())
        .output()
        .expect("run smoke");
    let stdout = String::from_utf8_lossy(&output.stdout);
    print!("{stdout}");
    let pattern = regex::Regex::new(r"^SMOKE_RESULT: PASSED \(elapsed [0-9]+ s\)$").unwrap();
    assert!(output.status.success(), "{stdout}");
    assert!(
        stdout.lines().any(|line| pattern.is_match(line)),
        "{stdout}"
    );
    assert!(!stdout.lines().any(|line| line.starts_with("SKIP ")));
    assert_eq!(std::fs::read_dir(root.path()).unwrap().count(), 0);
}
