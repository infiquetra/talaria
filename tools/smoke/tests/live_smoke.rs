//! The real thing: the built `smoke` binary against the real Herdr, a real `talaria` binary and
//! the running Hermes gateway. It is ignored in the offline suite; run it by hand, inside a Herdr
//! pane, with the operator preconditions of the README:
//!
//! `TALARIA_LIVE_REQUIRE=1 cargo test -p smoke --test live_smoke -- --ignored --nocapture`

mod support;

use std::process::{Command, Stdio};

use regex::Regex;

#[test]
#[ignore = "drives the real Herdr, talaria and Hermes gateway; run by hand inside a Herdr pane"]
fn live_full_smoke_in_herdr() {
    // A skipped live run must never look like a pass, so a missing pane is a failure, not a skip.
    assert_eq!(
        std::env::var("HERDR_ENV").as_deref(),
        Ok("1"),
        "the live smoke test must run inside a Herdr pane (HERDR_ENV=1 is not set); start it \
         from a Herdr tab"
    );
    assert!(
        std::env::var("HERDR_WORKSPACE_ID").is_ok_and(|id| !id.is_empty()),
        "the live smoke test must run inside a Herdr pane (HERDR_WORKSPACE_ID is not set); start \
         it from a Herdr tab"
    );

    let output = Command::new(support::smoke_exe())
        .arg("run")
        .env("TALARIA_LIVE_REQUIRE", "1")
        .stdin(Stdio::null())
        .output()
        .expect("start the smoke binary");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    println!("{stdout}");
    eprintln!("{stderr}");

    let passed = Regex::new(r"^SMOKE_RESULT: PASSED \(elapsed \d+ s\)$").expect("regex");
    let matching = stdout.lines().filter(|line| passed.is_match(line)).count();
    assert_eq!(
        output.status.code(),
        Some(0),
        "the live smoke run must exit 0 (a skip, a failure or a preflight problem is no pass); \
         stdout:\n{stdout}\nstderr:\n{stderr}"
    );
    assert_eq!(
        matching, 1,
        "exactly one line must read `SMOKE_RESULT: PASSED (elapsed <n> s)`; stdout:\n{stdout}\n\
         stderr:\n{stderr}"
    );
}
