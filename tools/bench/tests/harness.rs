//! End to end tests of the harness against the test double client.
//!
//! Every test starts real child processes and measures wall-clock time, so the tests
//! in this file take a lock and run one at a time: two runs measuring each other's
//! processor use or key latency would make the limits meaningless. Each passing run
//! is meant to stay under half of its limit and each failing flag to exceed its limit
//! at least twice, so a busy machine neither fails a good run nor rescues a bad one.
//! Failing demonstrations select the one row they target with `only` so each takes
//! seconds, not minutes, and use a single attempt unless the test is about attempts.

#![cfg(unix)]

mod support;

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Mutex, MutexGuard};
use std::time::Instant;

use talaria_bench::{BenchError, Report, Row, RunOptions, Status, exit_code, generate, run};

static SERIAL: Mutex<()> = Mutex::new(());

/// Held for the whole test. A failed test poisons the lock, which must not stop the rest.
fn serial() -> MutexGuard<'static, ()> {
    SERIAL
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// The budget rows a good client must pass, as substrings of the row names of the table.
const ENFORCED_ROWS: &[&str] = &[
    "launch to first frame",
    "keystroke p95, stream at 1x",
    "keystroke max, stream at 1x",
    "keystroke p95, stream at 10x",
    "keystroke max, stream at 10x",
    "CPU mean while streaming",
    "CPU peak",
    "bytes per frame, median",
    "bytes per frame, late over early",
    "worst single frame",
    "idle frames and bytes",
    "resident memory at start",
    "resident memory after",
    "no queries after start-up",
    "frame count matches",
];

const INFO_ROW: &str = "idle CPU mean";
const SKIPPED_ROW: &str = "launch to usable composer";
const LAUNCH_ROW: &str = "launch to first frame";
const WIRING_LINE: &str =
    "bench wiring: streaming tail and composer only (no status line, no prompt cards)";

// ---- running ----

/// Options for `binary` with the given extra arguments and row selection. One attempt.
fn options(binary: PathBuf, args: &[&str], only: &[&str]) -> RunOptions {
    let mut options = RunOptions::new(binary, support::test_budgets());
    options.fixtures = support::templates_dir();
    options.idle_seconds = Some(2);
    options.attempts = 1;
    options.binary_args = args.iter().map(|arg| (*arg).to_string()).collect();
    options.only = only.iter().map(|row| (*row).to_string()).collect();
    options
}

fn fake_options(args: &[&str], only: &[&str]) -> RunOptions {
    options(support::fake_tui_path(), args, only)
}

fn run_fake(args: &[&str], only: &[&str]) -> Report {
    run(&fake_options(args, only)).expect("the run completes and measures")
}

// ---- reading a report ----

fn rows_matching<'a>(report: &'a Report, needle: &str) -> Vec<&'a Row> {
    report
        .rows
        .iter()
        .filter(|row| row.name.contains(needle))
        .collect()
}

fn the_row<'a>(report: &'a Report, needle: &str) -> &'a Row {
    let rows = rows_matching(report, needle);
    let names: Vec<&str> = report.rows.iter().map(|row| row.name.as_str()).collect();
    assert_eq!(
        rows.len(),
        1,
        "expected exactly one row named like {needle:?} in {names:?}"
    );
    rows[0]
}

/// Everything a row says about itself.
fn row_text(row: &Row) -> String {
    format!(
        "{}\n{}\n{}\n{}",
        row.name,
        row.measured,
        row.limit,
        row.attempts.join("\n")
    )
}

fn contains_ignoring_case(haystack: &str, needle: &str) -> bool {
    haystack.to_lowercase().contains(&needle.to_lowercase())
}

fn assert_status(report: &Report, needle: &str, expected: Status) {
    let row = the_row(report, needle);
    assert_eq!(
        row.status,
        expected,
        "row {needle:?} in:\n{}",
        report.render()
    );
}

/// The row is FAIL and the table says why, in the row or on a line beside it.
fn assert_fails_saying(report: &Report, needle: &str, message: &str) {
    assert_status(report, needle, Status::Fail);
    let row = the_row(report, needle);
    let table = report.render();
    assert!(
        contains_ignoring_case(&row_text(row), message) || contains_ignoring_case(&table, message),
        "expected {message:?} for {needle:?} in:\n{table}"
    );
}

/// Rows of one keystroke speed, `tag` being `at 1x` or `at 10x`.
fn keystroke_rows<'a>(report: &'a Report, tag: &str) -> Vec<&'a Row> {
    report
        .rows
        .iter()
        .filter(|row| row.name.contains("keystroke") && row.name.contains(tag))
        .collect()
}

fn assert_keystroke_rows_fail(report: &Report, tag: &str) {
    let rows = keystroke_rows(report, tag);
    assert!(
        !rows.is_empty(),
        "no keystroke rows {tag} in:\n{}",
        report.render()
    );
    assert!(
        rows.iter().any(|row| row.status == Status::Fail),
        "no keystroke row {tag} failed in:\n{}",
        report.render()
    );
    assert!(
        rows.iter().all(|row| row.status != Status::Pass),
        "a keystroke row {tag} passed in:\n{}",
        report.render()
    );
}

fn assert_exit_code(report: &Report, expected: u8) {
    assert_eq!(
        exit_code(report),
        expected,
        "exit code for:\n{}",
        report.render()
    );
}

// ---- looking for leaks ----

/// Words of a text that start at a root: `/x/y`, `C:\x`, `~/x`. A lone `/` (as in `3 / 3`) is not one.
fn rooted_words(text: &str) -> Vec<String> {
    let delimiters = |c: char| {
        c.is_whitespace()
            || matches!(
                c,
                '"' | '\'' | ',' | '(' | ')' | '[' | ']' | '{' | '}' | ';'
            )
    };
    text.split(delimiters)
        .filter(|word| {
            let bytes = word.as_bytes();
            let drive = bytes.len() > 2
                && bytes[0].is_ascii_alphabetic()
                && bytes[1] == b':'
                && (bytes[2] == b'\\' || bytes[2] == b'/');
            (word.starts_with('/') && word.len() > 1) || word.starts_with("~/") || drive
        })
        .map(str::to_string)
        .collect()
}

fn assert_no_rooted_path(label: &str, text: &str) {
    let rooted = rooted_words(text);
    assert!(
        rooted.is_empty(),
        "{label} holds rooted paths {rooted:?}:\n{text}"
    );
}

/// Places a run touches that must never be printed.
fn private_places() -> Vec<String> {
    let mut places = vec![
        support::fake_tui_path().display().to_string(),
        std::env::temp_dir().display().to_string(),
        env!("CARGO_MANIFEST_DIR").to_string(),
    ];
    if let Some(parent) = support::fake_tui_path().parent() {
        places.push(parent.display().to_string());
    }
    if let Ok(root) = fs::canonicalize(support::workspace_root()) {
        places.push(root.display().to_string());
    }
    if let Ok(home) = std::env::var("HOME")
        && home.len() > 1
    {
        places.push(home);
    }
    places
}

/// The host name and the user name, lower case, when they can be found and are long enough
/// to tell apart from ordinary words.
fn identity_words() -> Vec<String> {
    let mut words = Vec::new();
    if let Ok(output) = Command::new("hostname").output() {
        let host = String::from_utf8_lossy(&output.stdout)
            .trim()
            .to_lowercase();
        if let Some(label) = host.split('.').next()
            && label.len() >= 4
        {
            words.push(label.to_string());
        }
        if host.len() >= 4 {
            words.push(host);
        }
    }
    for variable in ["USER", "LOGNAME"] {
        if let Ok(user) = std::env::var(variable)
            && user.len() >= 3
        {
            words.push(user.to_lowercase());
        }
    }
    words
}

/// Nothing printed or written may name a place on this machine, the host or the user.
fn assert_nothing_private(label: &str, text: &str) {
    assert_no_rooted_path(label, text);
    for place in private_places() {
        assert!(
            !text.contains(&place),
            "{label} holds the path {place:?}:\n{text}"
        );
    }
    let lower = text.to_lowercase();
    let tokens: Vec<&str> = lower
        .split(|c: char| !(c.is_alphanumeric() || matches!(c, '-' | '_' | '.')))
        .map(|token| token.trim_matches('.'))
        .collect();
    for word in identity_words() {
        assert!(
            !tokens.contains(&word.as_str()),
            "{label} holds {word:?}:\n{text}"
        );
    }
}

// ---- stand-in clients ----

/// A shell script that stands in for the client: it optionally answers `--version` with the
/// given lines, optionally prints a line of noise before the real double starts, then becomes
/// the real double with the same arguments (so exit statuses and flags behave as they do there).
fn client_script(dir: &Path, version_lines: Option<&[&str]>, noise: Option<&str>) -> PathBuf {
    let mut body = String::from("#!/bin/sh\n");
    if let Some(lines) = version_lines {
        body.push_str("if [ \"$1\" = \"--version\" ]; then\n");
        for line in lines {
            body.push_str(&format!("  printf '%s\\n' '{line}'\n"));
        }
        body.push_str("  exit 0\nfi\n");
    }
    if let Some(noise) = noise {
        body.push_str(&format!(
            "if [ \"$1\" != \"--version\" ]; then printf '%s\\n' '{noise}'; fi\n"
        ));
    }
    body.push_str(&format!(
        "exec '{}' \"$@\"\n",
        support::fake_tui_path().display()
    ));
    let path = dir.join("client.sh");
    fs::write(&path, body).expect("write the stand-in client");
    fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).expect("make it executable");
    path
}

/// A short reference stream (two turns of three seconds) made by the generator from the templates.
fn short_stream(dir: &Path, name: &str) -> PathBuf {
    let generated = dir.join("generated");
    fs::create_dir_all(&generated).expect("create the generator output directory");
    let mut budgets = support::test_budgets();
    budgets.scenario.turn_seconds = 3;
    generate(&support::templates_dir(), &generated, &budgets).expect("generate the short stream");
    let target = dir.join(name);
    fs::copy(generated.join("reference-stream.jsonl"), &target).expect("copy the stream");
    target
}

fn pid_is_alive(pid: u32) -> bool {
    let output = Command::new("ps")
        .args(["-p", &pid.to_string(), "-o", "pid="])
        .output()
        .expect("ps runs");
    !String::from_utf8_lossy(&output.stdout).trim().is_empty()
}

fn starts_with_number(text: &str) -> bool {
    text.trim_start()
        .chars()
        .next()
        .is_some_and(|c| c.is_ascii_digit())
}

// ---- tests ----

#[test]
fn good_client_passes_every_enforced_budget() {
    let _guard = serial();
    let mut options = fake_options(&[], &[]);
    options.attempts = 3;
    let report = run(&options).expect("the run completes and measures");
    let table = report.render();

    for needle in ENFORCED_ROWS {
        assert_status(&report, needle, Status::Pass);
    }
    for row in &report.rows {
        assert!(
            matches!(row.status, Status::Pass | Status::Info | Status::Skipped),
            "row {:?} did not pass in:\n{table}",
            row.name
        );
    }
    assert_status(&report, INFO_ROW, Status::Info);
    assert_status(&report, SKIPPED_ROW, Status::Skipped);
    assert_exit_code(&report, 0);
}

#[test]
fn slow_startup_fails_launch_budget() {
    let _guard = serial();
    let report = run_fake(&["--startup-delay-ms", "3000"], &["launch"]);

    assert_status(&report, LAUNCH_ROW, Status::Fail);
    assert_exit_code(&report, 1);
}

#[test]
fn slow_echo_fails_keystroke_budgets() {
    let _guard = serial();
    let report = run_fake(&["--echo-delay-ms", "300"], &["keystroke"]);
    assert_keystroke_rows_fail(&report, "at 1x");
    assert_keystroke_rows_fail(&report, "at 10x");
    assert_exit_code(&report, 1);

    let report = run_fake(&["--drop-key", "3"], &["keystroke"]);
    assert_keystroke_rows_fail(&report, "at 1x");
    assert_keystroke_rows_fail(&report, "at 10x");
    let p95 = the_row(&report, "keystroke p95, stream at 1x");
    assert_fails_saying(
        &report,
        "keystroke p95, stream at 1x",
        "key 3 not visible within 1 s",
    );
    assert!(
        !starts_with_number(&p95.measured),
        "a key that never showed must not report a percentile, got {:?}",
        p95.measured
    );
    assert_exit_code(&report, 1);
}

#[test]
fn busy_client_fails_cpu_budget() {
    let _guard = serial();
    let report = run_fake(&["--spin"], &["cpu"]);

    assert_status(&report, "CPU mean while streaming", Status::Fail);
    assert_status(&report, "CPU peak", Status::Fail);
    assert_exit_code(&report, 1);
}

#[test]
fn growing_or_large_frames_fail_both_bytes_rows() {
    let _guard = serial();
    let report = run_fake(&["--grow-bytes", "64"], &["bytes"]);
    assert_status(&report, "bytes per frame, late over early", Status::Fail);
    assert_exit_code(&report, 1);

    let report = run_fake(&["--frame-bytes", "9000"], &["bytes"]);
    assert_status(&report, "bytes per frame, median", Status::Fail);
    assert_exit_code(&report, 1);
}

#[test]
fn idle_repaint_and_missing_markers_fail() {
    let _guard = serial();
    let report = run_fake(&["--idle-repaint-ms", "500"], &["idle"]);
    assert_status(&report, "idle frames and bytes", Status::Fail);
    assert_exit_code(&report, 1);

    let report = run_fake(&["--no-markers"], &["launch"]);
    assert!(
        report.rows.iter().any(|row| row.status == Status::Fail),
        "a client without synchronized output markers must fail:\n{}",
        report.render()
    );
    assert!(
        contains_ignoring_case(&report.render(), "no synchronized frames seen"),
        "expected the no frames message in:\n{}",
        report.render()
    );
    assert_exit_code(&report, 1);
}

#[test]
fn ballast_fails_start_and_long_session_memory_rows() {
    let _guard = serial();
    let report = run_fake(&["--ballast-mib", "250"], &["memory"]);
    assert_status(&report, "resident memory at start", Status::Fail);
    assert_exit_code(&report, 1);

    let report = run_fake(
        &["--ballast-mib", "250", "--ballast-when", "replay"],
        &["memory"],
    );
    assert_status(&report, "resident memory at start", Status::Pass);
    assert_status(&report, "resident memory after", Status::Fail);
    assert_exit_code(&report, 1);
}

#[test]
fn slow_pass_fails_worst_frame_and_count_mismatch_fails_run() {
    let _guard = serial();
    let report = run_fake(&["--slow-pass-ms", "80"], &["worst-frame"]);
    assert_status(&report, "worst single frame", Status::Fail);
    assert_status(&report, "frame count matches", Status::Pass);
    assert_exit_code(&report, 1);

    let report = run_fake(&["--frames-drawn-skew-pct", "5"], &["worst-frame"]);
    assert_status(&report, "frame count matches", Status::Fail);
    assert_status(&report, "worst single frame", Status::Pass);
    let json: serde_json::Value =
        serde_json::from_str(&report.to_json()).expect("the report is JSON");
    assert!(
        json["checks"].get("frame_count_mismatch").is_some(),
        "the JSON checks name frame_count_mismatch: {json}"
    );
    assert_exit_code(&report, 1);
}

#[test]
fn late_query_is_reported_as_query_after_startup() {
    let _guard = serial();
    let report = run_fake(&["--late-query"], &["cpu"]);

    assert_status(&report, "no queries after start-up", Status::Fail);
    let json: serde_json::Value =
        serde_json::from_str(&report.to_json()).expect("the report is JSON");
    assert!(
        json["checks"].get("query_after_startup").is_some(),
        "the JSON checks name query_after_startup: {json}"
    );
    assert_exit_code(&report, 1);
}

#[test]
fn child_is_killed_when_measurement_ends() {
    let _guard = serial();
    // The double writes its report 300 ms after the start frame and then stays alive, so
    // the keystroke rows fail with a child that only the harness can end.
    let report = run_fake(&["--early-report-ms", "300"], &["keystroke"]);

    assert_exit_code(&report, 1);
    assert!(
        !report.child_pids.is_empty(),
        "the report lists the children it started"
    );
    for pid in &report.child_pids {
        assert!(
            !pid_is_alive(*pid),
            "child {pid} is still alive after the run"
        );
    }
}

#[test]
fn missing_binary_exits_2_and_output_has_no_absolute_paths() {
    let _guard = serial();

    // A missing binary is the plain error of the library, as one line.
    let error = run(&RunOptions::new(
        PathBuf::from("./no-such-binary"),
        support::test_budgets(),
    ))
    .expect_err("a missing binary measures nothing");
    assert!(matches!(&error, BenchError::BinaryNotFound(path) if path == "./no-such-binary"));
    let line = error.to_string();
    assert!(!line.contains('\n'), "one line, got {line:?}");
    assert!(
        line.contains("./no-such-binary"),
        "names the path as given: {line:?}"
    );
    assert!(
        line.contains("cargo build --release -p talaria-cli"),
        "says what to do: {line:?}"
    );

    // A normal run prints and writes no path, host name or user name.
    let report = run_fake(&[], &["launch"]);
    assert_nothing_private("the table", &report.render());
    assert_nothing_private("the JSON", &report.to_json());
    let json: serde_json::Value =
        serde_json::from_str(&report.to_json()).expect("the report is JSON");
    let version = json["binary_version"]
        .as_str()
        .expect("binary_version is a string");
    assert!(
        version.starts_with("fake-tui"),
        "the first line of --version, got {version:?}"
    );
    assert!(!version.contains('/') && !version.contains('\\') && !version.contains('\n'));

    // Only the first line of --version is kept, and only a short line without a slash.
    let scratch = tempfile::tempdir().expect("create a scratch directory");
    let long_line = "v".repeat(120);
    let cases: [(&[&str], &str); 4] = [
        (&["tool 1.0", "second line"], "tool 1.0"),
        (&["tool 1.0 src/build"], "unknown"),
        (&["tool 1.0 src\\build"], "unknown"),
        (&[long_line.as_str()], "unknown"),
    ];
    for (lines, expected) in cases {
        let script = client_script(scratch.path(), Some(lines), None);
        let report = run(&options(script, &[], &["launch"])).expect("the run completes");
        let json: serde_json::Value =
            serde_json::from_str(&report.to_json()).expect("the report is JSON");
        assert_eq!(
            json["binary_version"], expected,
            "for version output {lines:?}"
        );
    }

    // The command line says the same in one line and exits 2 with no table.
    let work = tempfile::tempdir().expect("create a working directory");
    let output = Command::new(support::bench_bin_path())
        .current_dir(work.path())
        .args(["run", "--binary", "./no-such-binary", "--budgets"])
        .arg(support::data_dir().join("budgets-test.toml"))
        .arg("--fixtures")
        .arg(support::templates_dir())
        .output()
        .expect("run talaria-bench");
    assert_eq!(output.status.code(), Some(2), "a missing binary is exit 2");
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let lines: Vec<&str> = text
        .lines()
        .filter(|line| !line.trim().is_empty())
        .collect();
    assert_eq!(lines.len(), 1, "one plain line, got:\n{text}");
    assert!(
        lines[0].contains("./no-such-binary"),
        "names the path as given: {text}"
    );
    assert!(
        lines[0].contains("cargo build --release -p talaria-cli"),
        "says what to do: {text}"
    );
    assert!(
        !text.contains("PASS") && !text.contains("os:"),
        "no table: {text}"
    );
    assert_nothing_private("the exit 2 line", &text);
}

#[test]
fn extra_fixture_rows_are_labelled_and_table_lists_all_rows() {
    let _guard = serial();
    let scratch = tempfile::tempdir().expect("create a scratch directory");
    let extra = short_stream(scratch.path(), "extra-stream.jsonl");
    let scenario_dir = scratch.path().join("scenarios");
    fs::create_dir_all(&scenario_dir).expect("create the scenario directory");
    fs::copy(&extra, scenario_dir.join("queue-preview.jsonl")).expect("add a scenario file");
    fs::write(scenario_dir.join("README.md"), "Notes, not a scenario.\n").expect("add a readme");

    let mut options = fake_options(&[], &[]);
    options.attempts = 3;
    options.extra_fixtures = vec![extra];
    options.scenario_dir = Some(scenario_dir);
    let report = run(&options).expect("the run completes and measures");
    let table = report.render();

    // Line 1 names the machine, line 2 says what the wiring covers.
    let mut lines = table.lines();
    let first = lines.next().expect("a first line");
    let prefix = format!(
        "os: {} arch: {} cpus: ",
        std::env::consts::OS,
        std::env::consts::ARCH
    );
    assert!(
        first.starts_with(&prefix),
        "first line {first:?} should start {prefix:?}"
    );
    let cpus: usize = first[prefix.len()..]
        .trim()
        .parse()
        .expect("a processor count");
    assert!(cpus >= 1, "at least one processor, got {cpus}");
    assert_eq!(lines.next().map(str::trim_end), Some(WIRING_LINE));

    // Every budget row and both checks are in the table and in the rows.
    let labels = ["extra-stream", "queue-preview"];
    let unlabelled = |needle: &str| {
        report
            .rows
            .iter()
            .filter(|row| row.name.contains(needle) && !labels.iter().any(|l| row.name.contains(l)))
            .count()
    };
    for needle in ENFORCED_ROWS.iter().chain([&INFO_ROW, &SKIPPED_ROW]) {
        assert!(unlabelled(needle) >= 1, "no row {needle:?} in:\n{table}");
        assert!(
            table.contains(needle),
            "the table lacks {needle:?}:\n{table}"
        );
    }
    assert_status(&report, INFO_ROW, Status::Info);
    assert_status(&report, SKIPPED_ROW, Status::Skipped);
    for word in ["PASS", "INFO", "SKIPPED"] {
        assert!(
            table.contains(word),
            "the table never says {word}:\n{table}"
        );
    }
    let counts = table
        .lines()
        .find(|line| line.starts_with("counts:"))
        .unwrap_or_else(|| panic!("no counts line in:\n{table}"));
    for part in [
        "unanswered queries",
        "bytes outside frames",
        "nested markers",
        "unterminated frames",
    ] {
        assert!(
            counts.contains(part),
            "counts line lacks {part:?}: {counts}"
        );
    }
    assert!(
        counts.contains("lag"),
        "counts line lacks the write lag: {counts}"
    );

    // Each extra file adds streaming rows labelled by its file name, and a non-scenario file adds none.
    for label in labels {
        for kind in [
            "keystroke",
            "CPU",
            "bytes per frame",
            "worst single frame",
            "idle frames",
        ] {
            assert!(
                report
                    .rows
                    .iter()
                    .any(|r| r.name.contains(label) && r.name.contains(kind)),
                "no {kind:?} row labelled {label:?} in:\n{table}"
            );
        }
    }
    assert!(
        !table.contains("README"),
        "a readme is not a scenario:\n{table}"
    );
    assert_nothing_private("the table", &table);

    // Selecting launch prints launch and the skipped gateway row, and nothing else.
    let report = run_fake(&[], &["launch"]);
    assert_status(&report, LAUNCH_ROW, Status::Pass);
    assert_status(&report, SKIPPED_ROW, Status::Skipped);
    for row in &report.rows {
        let name = row.name.to_lowercase();
        assert!(
            name.contains("launch") || name.starts_with("check:"),
            "row {:?} should not run for --only launch:\n{}",
            row.name,
            report.render()
        );
    }
    for other in [
        "keystroke",
        "CPU",
        "bytes per frame",
        "worst single frame",
        "idle",
        "resident",
    ] {
        assert!(
            rows_matching(&report, other).is_empty(),
            "the {other:?} rows ran for --only launch:\n{}",
            report.render()
        );
    }
}

#[test]
fn missing_report_fails_the_worst_frame_row() {
    let _guard = serial();
    let mut options = fake_options(&["--no-report"], &["worst-frame"]);
    options.report_grace_seconds = 1;
    let started = Instant::now();
    let report = run(&options).expect("the run completes");
    let elapsed = started.elapsed();

    assert_fails_saying(&report, "worst single frame", "binary report missing");
    assert_exit_code(&report, 1);
    // The deadline is the replay's length (12 s at the test budgets) plus the grace period.
    assert!(
        elapsed.as_secs_f64() >= 12.0,
        "gave up after {elapsed:?}, before the replay could end"
    );
    assert!(
        elapsed.as_secs_f64() < 40.0,
        "took {elapsed:?} to give up on the report"
    );
}

#[test]
fn early_exit_is_reported_by_status_only() {
    let _guard = serial();
    let scratch = tempfile::tempdir().expect("create a scratch directory");
    let noisy = client_script(scratch.path(), None, Some("child-output-marker"));

    let report = run(&options(
        noisy.clone(),
        &["--exit-before-first-frame"],
        &["launch"],
    ))
    .expect("the run completes");
    assert_fails_saying(
        &report,
        LAUNCH_ROW,
        "exited with status 3 before its first frame",
    );
    assert_exit_code(&report, 1);
    for (label, text) in [
        ("the table", report.render()),
        ("the JSON", report.to_json()),
    ] {
        assert!(
            !text.contains("child-output-marker"),
            "{label} printed child output:\n{text}"
        );
    }

    let report =
        run(&options(noisy, &["--exit-after-ms", "800"], &["cpu"])).expect("the run completes");
    let cpu_rows = rows_matching(&report, "CPU");
    assert!(!cpu_rows.is_empty(), "no CPU rows in:\n{}", report.render());
    assert!(
        cpu_rows.iter().all(|row| row.status != Status::Pass),
        "{}",
        report.render()
    );
    assert_fails_saying(&report, "CPU mean while streaming", "exited with status 4");
    assert_exit_code(&report, 1);
    for (label, text) in [
        ("the table", report.render()),
        ("the JSON", report.to_json()),
    ] {
        assert!(
            !text.contains("child-output-marker"),
            "{label} printed child output:\n{text}"
        );
    }
}

#[test]
fn report_before_last_keystroke_fails_the_keystroke_row() {
    let _guard = serial();
    let mut options = fake_options(&["--early-report-ms", "300"], &["keystroke"]);
    options.attempts = 3;
    let report = run(&options).expect("the run completes");

    // At the test budgets the last key at 1x is due 1.4 s after the start frame, long after 300 ms.
    assert_fails_saying(
        &report,
        "keystroke p95, stream at 1x",
        "stream ended before the last keystroke",
    );
    assert_eq!(
        the_row(&report, "keystroke p95, stream at 1x")
            .attempts
            .len(),
        3,
        "every attempt is listed under its row:\n{}",
        report.render()
    );
    assert_exit_code(&report, 1);
}
