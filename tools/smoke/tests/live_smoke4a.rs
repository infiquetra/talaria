//! Live run against the gateway; intentionally ignored in ordinary testing.
mod support;
use std::process::Command;
#[test]
#[ignore]
fn live_phase4a_smoke_in_herdr() {
    assert!(
        std::env::var("HERDR_ENV").is_ok(),
        "the live run fails outside Herdr"
    );
    let talaria = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .unwrap()
        .join("target/debug/talaria");
    assert!(talaria.is_file());
    let root = tempfile::tempdir().unwrap();
    let out = Command::new(support::smoke_exe())
        .args(["run", "--suite", "phase4a", "--talaria"])
        .arg(talaria)
        .env("TMPDIR", root.path())
        .output()
        .expect("run smoke");
    let stdout = String::from_utf8_lossy(&out.stdout);
    print!("{stdout}");
    eprint!("{}", String::from_utf8_lossy(&out.stderr));
    let result = regex::Regex::new(r"^SMOKE_RESULT: PASSED \(elapsed [0-9]+ s\)$").unwrap();
    assert!(out.status.success() && stdout.lines().any(|line| result.is_match(line)));
    assert_eq!(std::fs::read_dir(root.path()).unwrap().count(), 0);
}
