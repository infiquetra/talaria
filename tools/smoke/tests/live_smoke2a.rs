//! The phase 2a suite against the live gateway, in a Herdr pane.
//!
//! Ignored by default; fails, never skips, outside Herdr.

mod support;

use std::path::PathBuf;
use std::process::Command;

use regex::Regex;
use support::smoke_exe;

#[test]
#[ignore]
fn live_phase2a_smoke_in_herdr() {
    assert!(
        std::env::var("HERDR_ENV").is_ok(),
        "the live run fails, never skips, outside Herdr"
    );
    let talaria = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .expect("the workspace root")
        .join("target/debug/talaria");
    assert!(
        talaria.is_file(),
        "the debug binary is built: {}",
        talaria.display()
    );
    let output = Command::new(smoke_exe())
        .arg("run")
        .arg("--suite")
        .arg("phase2a")
        .arg("--talaria")
        .arg(&talaria)
        .output()
        .expect("run the smoke binary");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let result = Regex::new(r"^SMOKE_RESULT: PASSED \(elapsed [0-9]+ s\)$").expect("regex");
    assert!(
        output.status.success() && stdout.lines().any(|line| result.is_match(line)),
        "exit code {:?}\n--- stdout ---\n{stdout}\n--- stderr ---\n{}",
        output.status.code(),
        String::from_utf8_lossy(&output.stderr)
    );
}
