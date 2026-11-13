mod support;

use std::fs;
use std::path::Path;
use std::process::{Command, Stdio};

use support::{Harness, Outcome, smoke_exe, write_executable};

const NOT_IN_HERDR: &str =
    "This test must run inside a Herdr pane (HERDR_ENV=1). Start it from a Herdr tab.";

fn fake_hc_exe() -> &'static str {
    env!("CARGO_BIN_EXE_fake-hc")
}

fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

fn explain(outcome: &Outcome) -> String {
    format!(
        "exit={:?}\nstdout:\n{}\nstderr:\n{}",
        outcome.code, outcome.stdout, outcome.stderr
    )
}

fn run_phase1_with_hc(harness: &Harness, herdr_arg: &Path) -> Outcome {
    let output = Command::new(smoke_exe())
        .env_clear()
        .env("PATH", "/usr/bin:/bin")
        .env("HOME", harness.home_dir())
        .env("USER", harness.user())
        .env("LOGNAME", harness.user())
        .env("TMPDIR", harness.tmp_dir())
        .env("TALARIA_SMOKE_TERMINAL", "hc")
        .current_dir(harness.root())
        .stdin(Stdio::null())
        .args(["run", "--suite", "phase1"])
        .arg("--herdr")
        .arg(herdr_arg)
        .arg("--talaria")
        .arg(harness.talaria())
        .output()
        .expect("run smoke binary");

    Outcome {
        code: output.status.code(),
        stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
    }
}

fn read_log(path: &Path) -> Vec<Vec<String>> {
    let raw = fs::read_to_string(path)
        .unwrap_or_else(|err| panic!("failed to read log {}: {err}", path.display()));
    raw.lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| {
            serde_json::from_str::<Vec<String>>(line)
                .unwrap_or_else(|err| panic!("invalid JSON log line `{line}`: {err}"))
        })
        .collect()
}

#[test]
fn unknown_selector_is_refused() {
    let harness = Harness::plain();
    let outcome = harness
        .cmd()
        .outside_herdr()
        .env("TALARIA_SMOKE_TERMINAL", "nope")
        .run();

    assert_eq!(
        outcome.code,
        Some(2),
        "expected exit 2:\n{}",
        explain(&outcome)
    );
    assert!(
        outcome
            .stderr
            .contains("unknown TALARIA_SMOKE_TERMINAL 'nope'; use herdr or hc"),
        "expected refusal message in stderr:\n{}",
        explain(&outcome)
    );
    assert!(
        !outcome.stdout.contains(NOT_IN_HERDR) && !outcome.stderr.contains(NOT_IN_HERDR),
        "unexpected NOT_IN_HERDR message:\n{}",
        explain(&outcome)
    );
}

#[test]
fn hc_selector_probes_with_ls() {
    let harness = Harness::plain();
    let hc_dir = tempfile::tempdir().expect("create fake-hc tempdir");
    fs::write(hc_dir.path().join("replies.json"), "{}").expect("write replies.json");

    let wrapper = hc_dir.path().join("hc");
    let body = format!(
        "#!/bin/sh\nFAKE_HC_DIR={} exec {} \"$@\"\n",
        shell_quote(&hc_dir.path().to_string_lossy()),
        shell_quote(fake_hc_exe()),
    );
    write_executable(&wrapper, &body);

    let outcome = run_phase1_with_hc(&harness, &wrapper);

    assert!(
        !outcome.stderr.contains(NOT_IN_HERDR),
        "unexpected NOT_IN_HERDR in stderr:\n{}",
        explain(&outcome)
    );

    let entries = read_log(&hc_dir.path().join("log"));
    assert!(
        entries
            .iter()
            .any(|entry| entry.first().map(String::as_str) == Some("ls")),
        "expected hc ls probe in log, got: {entries:?}"
    );
    for entry in &entries {
        assert!(
            !entry
                .iter()
                .any(|arg| arg == "tab" || arg == "--workspace" || arg.starts_with("--workspace")),
            "unexpected herdr flag/subcommand in hc invocation: {entry:?}"
        );
    }
}

#[test]
fn hc_liveness_failure_exits_2() {
    let harness = Harness::plain();
    let missing = harness.root().join("no-such-hc");

    let outcome = run_phase1_with_hc(&harness, &missing);

    assert_eq!(
        outcome.code,
        Some(2),
        "expected exit 2:\n{}",
        explain(&outcome)
    );
    let missing_str = missing.to_str().expect("missing path is valid utf8");
    assert!(
        outcome.stderr.contains(missing_str),
        "expected missing executable path `{missing_str}` in stderr:\n{}",
        explain(&outcome)
    );
    assert!(
        !outcome.stderr.contains(NOT_IN_HERDR),
        "unexpected NOT_IN_HERDR in stderr:\n{}",
        explain(&outcome)
    );
}
