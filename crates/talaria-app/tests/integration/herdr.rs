//! Herdr pane state reporting through the Herdr command line.
//!
//! The fake Herdr appends every run to a log file. Behavior contract: issue 228.

#![cfg(unix)]

use std::collections::HashMap;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use talaria_app::comforts::herdr::HerdrReporter;
use talaria_core::attention::{PaneReport, PaneState};

fn fake_herdr() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/support/fake_herdr.sh")
}

/// A wrapper that bakes this run's fake controls into the environment and execs the
/// shared fake, so tests never mutate the process environment.
fn fake_wrapper(dir: &Path, name: &str, log: &Path, sleep: &str, exit: &str) -> PathBuf {
    fn quote(text: &str) -> String {
        text.replace('\\', "\\\\").replace('"', "\\\"")
    }
    let script = dir.join(name);
    let body = format!(
        "#!/bin/sh\nFAKE_HERDR_LOG=\"{}\" FAKE_HERDR_SLEEP=\"{sleep}\" FAKE_HERDR_EXIT=\"{exit}\" exec \"{}\" \"$0\" \"$@\"\n",
        quote(&log.to_string_lossy()),
        quote(&fake_herdr().to_string_lossy()),
    );
    std::fs::write(&script, body).unwrap();
    let mut permissions = std::fs::metadata(&script).unwrap().permissions();
    permissions.set_mode(0o755);
    std::fs::set_permissions(&script, permissions).unwrap();
    script
}

fn herdr_vars(bin: &Path) -> HashMap<String, String> {
    let mut vars = HashMap::new();
    vars.insert("HERDR_ENV".to_owned(), "1".to_owned());
    vars.insert("HERDR_PANE_ID".to_owned(), "p_test".to_owned());
    vars.insert(
        "HERDR_BIN_PATH".to_owned(),
        bin.to_string_lossy().into_owned(),
    );
    vars
}

fn lookup(map: &HashMap<String, String>) -> impl Fn(&str) -> Option<String> + '_ {
    move |name: &str| map.get(name).cloned()
}

/// Complete runs from the fake log: argument lists split on `--end--`.
fn read_runs(log: &Path) -> Vec<Vec<String>> {
    let Ok(text) = std::fs::read_to_string(log) else {
        return Vec::new();
    };
    let mut runs = Vec::new();
    let mut current = Vec::new();
    for line in text.lines() {
        if line == "--end--" {
            runs.push(std::mem::take(&mut current));
        } else {
            current.push(line.to_owned());
        }
    }
    runs
}

fn wait_for_runs(log: &Path, count: usize, timeout: Duration) -> Vec<Vec<String>> {
    let start = Instant::now();
    loop {
        let runs = read_runs(log);
        if runs.len() >= count || start.elapsed() > timeout {
            return runs;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
}

fn pane(state: PaneState, message: Option<&str>) -> PaneReport {
    PaneReport {
        state,
        message: message.map(str::to_owned),
    }
}

fn flag(run: &[String], name: &str) -> Option<String> {
    run.iter()
        .skip_while(|arg| arg.as_str() != name)
        .nth(1)
        .cloned()
}

fn seq_of(run: &[String]) -> u64 {
    flag(run, "--seq")
        .expect("a seq flag")
        .parse()
        .expect("a numeric seq")
}

fn state_of(run: &[String]) -> String {
    flag(run, "--state").expect("a state flag")
}

#[test]
fn no_report_outside_herdr() {
    fn clock() -> u64 {
        1
    }
    let tmp = tempfile::tempdir().unwrap();
    let log = tmp.path().join("herdr.log");
    let bin = fake_wrapper(tmp.path(), "herdr.sh", &log, "0", "0");

    let mut vars = herdr_vars(&bin);
    vars.remove("HERDR_ENV");
    assert!(HerdrReporter::from_env(true, &lookup(&vars), clock).is_none());

    let vars = herdr_vars(&bin);
    assert!(HerdrReporter::from_env(false, &lookup(&vars), clock).is_none());

    std::thread::sleep(Duration::from_millis(200));
    assert!(
        !log.exists(),
        "no report process runs outside Herdr or when disabled"
    );
}

#[test]
fn report_arguments_match_herdr_cli() {
    fn clock() -> u64 {
        1_700_000_000_000
    }
    let tmp = tempfile::tempdir().unwrap();
    let log = tmp.path().join("herdr.log");
    let bin = fake_wrapper(tmp.path(), "herdr.sh", &log, "0", "0");
    let vars = herdr_vars(&bin);
    let reporter = HerdrReporter::from_env(true, &lookup(&vars), clock)
        .expect("inside Herdr with reporting enabled");
    let invoked = bin.to_string_lossy().into_owned();

    reporter.report(&pane(PaneState::Idle, None), None);
    let runs = wait_for_runs(&log, 1, Duration::from_secs(5));
    assert_eq!(
        runs[0],
        [
            invoked.as_str(),
            "pane",
            "report-agent",
            "--source",
            "talaria",
            "--agent",
            "talaria",
            "--state",
            "idle",
            "--seq",
            "1700000000000",
            "p_test",
        ]
    );

    reporter.report(
        &pane(PaneState::Blocked, Some("approval: rm -rf .pytest_cache")),
        Some(("session-9", "fixture")),
    );
    let runs = wait_for_runs(&log, 2, Duration::from_secs(5));
    assert_eq!(
        runs[1],
        [
            invoked.as_str(),
            "pane",
            "report-agent",
            "--source",
            "talaria",
            "--agent",
            "talaria",
            "--state",
            "blocked",
            "--seq",
            "1700000000001",
            "--message",
            "approval: rm -rf .pytest_cache",
            "--agent-session-id",
            "session-9",
            "p_test",
            "--",
            "talaria",
            "--resume",
            "session-9",
            "--profile",
            "fixture",
        ]
    );

    reporter.report(
        &pane(PaneState::Working, None),
        Some(("session-9", "default")),
    );
    let runs = wait_for_runs(&log, 3, Duration::from_secs(5));
    assert!(
        !runs[2].contains(&"--profile".to_owned()),
        "no profile flag for the default profile: {:?}",
        runs[2]
    );
    let tail = runs[2]
        .iter()
        .skip_while(|arg| arg.as_str() != "--")
        .cloned()
        .collect::<Vec<_>>();
    assert_eq!(tail, ["--", "talaria", "--resume", "session-9"]);
}

/// The clock this sequence test drives up and down.
static SEQ_CLOCK_MS: AtomicU64 = AtomicU64::new(0);

fn seq_clock_ms() -> u64 {
    SEQ_CLOCK_MS.load(Ordering::SeqCst)
}

#[test]
fn seq_rises_and_follows_the_clock() {
    let tmp = tempfile::tempdir().unwrap();
    let log = tmp.path().join("herdr.log");
    let bin = fake_wrapper(tmp.path(), "herdr.sh", &log, "0", "0");
    let vars = herdr_vars(&bin);
    SEQ_CLOCK_MS.store(1_000, Ordering::SeqCst);
    let reporter = HerdrReporter::from_env(true, &lookup(&vars), seq_clock_ms).unwrap();
    reporter.report(&pane(PaneState::Idle, None), None);
    let runs = wait_for_runs(&log, 1, Duration::from_secs(5));
    assert_eq!(seq_of(&runs[0]), 1_000);

    // The clock steps back: the sequence still rises by one.
    SEQ_CLOCK_MS.store(500, Ordering::SeqCst);
    reporter.report(&pane(PaneState::Working, None), None);
    let runs = wait_for_runs(&log, 2, Duration::from_secs(5));
    assert_eq!(seq_of(&runs[1]), 1_001);

    // A later reporter starts above the earlier one's numbers.
    SEQ_CLOCK_MS.store(2_000, Ordering::SeqCst);
    let later = HerdrReporter::from_env(true, &lookup(&vars), seq_clock_ms).unwrap();
    later.report(&pane(PaneState::Idle, None), None);
    let runs = wait_for_runs(&log, 3, Duration::from_secs(5));
    assert_eq!(seq_of(&runs[2]), 2_000);
}

#[test]
fn newest_state_wins_one_in_flight() {
    fn clock() -> u64 {
        1_000_000
    }
    let tmp = tempfile::tempdir().unwrap();
    let log = tmp.path().join("herdr.log");
    let bin = fake_wrapper(tmp.path(), "herdr.sh", &log, "0.5", "0");
    let vars = herdr_vars(&bin);
    let reporter = HerdrReporter::from_env(true, &lookup(&vars), clock).unwrap();
    for state in [
        PaneState::Idle,
        PaneState::Working,
        PaneState::Blocked,
        PaneState::Working,
        PaneState::Idle,
    ] {
        reporter.report(&pane(state, None), None);
    }
    let runs = wait_for_runs(&log, 2, Duration::from_secs(5));
    assert!(!runs.is_empty());
    // Room for stragglers to appear if coalescing is broken.
    std::thread::sleep(Duration::from_secs(1));
    let runs = read_runs(&log);
    assert!(runs.len() <= 2, "at most two runs, saw {}", runs.len());
    assert_eq!(
        state_of(runs.last().unwrap()),
        "idle",
        "the newest state is reported last"
    );
}

#[test]
fn apostrophe_resume_args_are_dropped() {
    fn clock() -> u64 {
        3_000
    }
    let tmp = tempfile::tempdir().unwrap();
    let log = tmp.path().join("herdr.log");
    let bin = fake_wrapper(tmp.path(), "herdr.sh", &log, "0", "0");
    let vars = herdr_vars(&bin);
    let reporter = HerdrReporter::from_env(true, &lookup(&vars), clock).unwrap();
    reporter.report(
        &pane(PaneState::Blocked, Some("approval: ls")),
        Some(("session-9", "pro'file")),
    );
    let runs = wait_for_runs(&log, 1, Duration::from_secs(5));
    assert!(
        !runs[0].contains(&"--".to_owned()),
        "no resume part when an argument holds an apostrophe: {:?}",
        runs[0]
    );

    reporter.report(
        &pane(PaneState::Blocked, Some("approval: ls")),
        Some(("sess\nion", "fixture")),
    );
    let runs = wait_for_runs(&log, 2, Duration::from_secs(5));
    assert!(
        !runs[1].contains(&"--".to_owned()),
        "no resume part when an argument holds a control character: {:?}",
        runs[1]
    );
}

fn wait_for_notices(reporter: &HerdrReporter, timeout: Duration) -> (Vec<String>, Duration) {
    let start = Instant::now();
    loop {
        let notices = reporter.take_notices();
        if !notices.is_empty() || start.elapsed() > timeout {
            return (notices, start.elapsed());
        }
        std::thread::sleep(Duration::from_millis(20));
    }
}

#[test]
fn slow_or_failing_herdr_never_blocks() {
    fn clock() -> u64 {
        9_000
    }

    // A report that hangs is killed at the limit; the caller never waits.
    let tmp = tempfile::tempdir().unwrap();
    let log = tmp.path().join("herdr.log");
    let bin = fake_wrapper(tmp.path(), "slow.sh", &log, "5", "0");
    let vars = herdr_vars(&bin);
    let reporter = HerdrReporter::from_env(true, &lookup(&vars), clock).unwrap();
    let start = Instant::now();
    reporter.report(&pane(PaneState::Working, None), None);
    assert!(
        start.elapsed() < Duration::from_millis(50),
        "report returns at once"
    );
    let (notices, waited) = wait_for_notices(&reporter, Duration::from_secs(6));
    assert_eq!(notices.len(), 1, "one notice for the killed report");
    assert!(
        (Duration::from_secs(2)..=Duration::from_millis(3_500)).contains(&waited),
        "killed at the limit, waited {waited:?}"
    );
    assert!(
        notices[0].starts_with("Herdr pane state could not be reported (")
            && notices[0].ends_with("); continuing without it."),
        "the failure notice shape: {}",
        notices[0]
    );
    reporter.report(&pane(PaneState::Idle, None), None);
    let (notices, _) = wait_for_notices(&reporter, Duration::from_secs(4));
    assert!(notices.is_empty(), "the notice shows once: {notices:?}");

    // A report that fails at once is tolerated the same way.
    let tmp = tempfile::tempdir().unwrap();
    let log = tmp.path().join("herdr.log");
    let bin = fake_wrapper(tmp.path(), "failing.sh", &log, "0", "1");
    let vars = herdr_vars(&bin);
    let reporter = HerdrReporter::from_env(true, &lookup(&vars), clock).unwrap();
    let start = Instant::now();
    reporter.report(&pane(PaneState::Working, None), None);
    assert!(
        start.elapsed() < Duration::from_millis(50),
        "report returns at once"
    );
    let (notices, _) = wait_for_notices(&reporter, Duration::from_secs(5));
    assert_eq!(notices.len(), 1);
    reporter.report(&pane(PaneState::Idle, None), None);
    std::thread::sleep(Duration::from_millis(500));
    assert!(reporter.take_notices().is_empty(), "the notice shows once");

    // A missing binary names its reason exactly.
    let vars = herdr_vars(Path::new("/nonexistent-herdr-binary"));
    let reporter = HerdrReporter::from_env(true, &lookup(&vars), clock).unwrap();
    reporter.report(&pane(PaneState::Working, None), None);
    let (notices, _) = wait_for_notices(&reporter, Duration::from_secs(5));
    assert_eq!(
        notices,
        ["Herdr pane state could not be reported (not found); continuing without it."]
    );
}

#[test]
fn release_on_exit_within_500_ms() {
    fn clock() -> u64 {
        5_000
    }
    let tmp = tempfile::tempdir().unwrap();
    let log = tmp.path().join("herdr.log");
    let bin = fake_wrapper(tmp.path(), "herdr.sh", &log, "5", "0");
    let vars = herdr_vars(&bin);
    let reporter = HerdrReporter::from_env(true, &lookup(&vars), clock).unwrap();
    let invoked = bin.to_string_lossy().into_owned();

    let start = Instant::now();
    reporter.release(Duration::from_millis(500));
    assert!(
        start.elapsed() < Duration::from_millis(600),
        "release waits at most the budget"
    );
    let runs = wait_for_runs(&log, 1, Duration::from_secs(5));
    assert_eq!(
        runs[0],
        [
            invoked.as_str(),
            "pane",
            "release-agent",
            "--source",
            "talaria",
            "--agent",
            "talaria",
            "--seq",
            "5000",
            "p_test",
        ]
    );
}
