//! The tab guard and the scratch project: the tab is closed (and checked to be gone) on a pass,
//! a failure, a panic inside a step, SIGINT and SIGTERM, and the scratch project is removed on
//! every path. In process runs use the scripted terminal and a manual clock; the signal tests
//! start the real `smoke` binary against the fake `herdr`.

mod support;

use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::AtomicBool;
use std::thread::sleep;
use std::time::{Duration, Instant};

use smoke::{ManualClock, SmokeConfig, SmokeReport, StepOutcome, run_smoke};
use support::{
    Call, FailBuilder, FakeTerminal, Harness, PANIC_MESSAGE, Recorded, Scenario, Startup,
    write_executable,
};

const UNCLOSED_TAB_LINE: &str = "cleanup: tab w1:t5 was not closed";

/// A temporary folder holding the system temporary directory of a run and the binary under test.
struct Bench {
    dir: tempfile::TempDir,
    talaria: PathBuf,
}

impl Bench {
    fn new() -> Self {
        let dir = tempfile::tempdir().expect("temporary folder for the bench");
        fs::create_dir(dir.path().join("tmp")).expect("create the temporary root");
        let talaria = dir.path().join("talaria");
        write_executable(&talaria, "#!/bin/sh\nexit 0\n");
        Bench { dir, talaria }
    }

    fn temp_root(&self) -> PathBuf {
        self.dir.path().join("tmp")
    }

    fn scratch_dirs(&self) -> Vec<PathBuf> {
        fs::read_dir(self.temp_root())
            .expect("read the temporary root")
            .flatten()
            .map(|e| e.path())
            .filter(|p| {
                p.file_name()
                    .is_some_and(|n| n.to_string_lossy().starts_with("talaria-smoke-"))
            })
            .collect()
    }

    fn config(&self) -> SmokeConfig {
        SmokeConfig {
            talaria: self.talaria.clone(),
            profile: "fixture".to_owned(),
            model: "minimax-m3.1-flash".to_owned(),
            provider: "ollama-cloud".to_owned(),
            deadline: Duration::from_secs(300),
            step_scale: 1.0,
            report_dir: None,
            require: false,
            temp_root: self.temp_root(),
            seed: Some(7),
        }
    }
}

/// How the scripted terminal misbehaves in one run.
#[derive(Default)]
struct Plan {
    panic_when: Option<fn(&Call) -> bool>,
    keep_tab: bool,
}

struct Played {
    report: SmokeReport,
    calls: Vec<Recorded>,
}

impl Played {
    fn kinds(&self) -> Vec<&'static str> {
        self.calls.iter().map(|r| r.call.kind()).collect()
    }

    fn closes(&self) -> usize {
        self.kinds().iter().filter(|k| **k == "tab close").count()
    }

    /// The working folder the tab was created with.
    fn scratch(&self) -> PathBuf {
        self.calls
            .iter()
            .find_map(|r| match &r.call {
                Call::CreateTab { cwd, .. } => Some(cwd.clone()),
                _ => None,
            })
            .expect("the run created a tab")
    }

    fn lines(&self) -> String {
        self.report.step_lines()
    }
}

fn play(bench: &Bench, scenario: &Scenario, plan: &Plan) -> Played {
    let clock = ManualClock::new();
    let mut term = FakeTerminal::new(scenario.build(), &clock);
    if let Some(predicate) = plan.panic_when {
        term = term.panic_when(predicate);
    }
    if plan.keep_tab {
        term = term.keep_tab_after_close();
    }
    let stop = AtomicBool::new(false);
    let report = run_smoke(&term, &clock, &bench.config(), &stop);
    Played {
        report,
        calls: term.calls(),
    }
}

fn panics_on_launch(call: &Call) -> bool {
    matches!(call, Call::Run { text, .. } if text.contains("--no-ensure"))
}

fn panics_on_first_prompt(call: &Call) -> bool {
    matches!(call, Call::Run { text, .. } if text.contains("join these two words"))
}

/// The outcome list ends in a failure of this step with this reason.
fn assert_last_failure(played: &Played, step: &str, reason: &str) {
    match played.report.outcomes.last() {
        Some(StepOutcome::Fail {
            step: failed,
            reason: why,
            ..
        }) => {
            assert_eq!(*failed, step, "lines:\n{}", played.lines());
            assert_eq!(why, reason, "lines:\n{}", played.lines());
        }
        _ => panic!(
            "the last outcome must be a failure of {step}; lines:\n{}",
            played.lines()
        ),
    }
}

fn explain(outcome: &support::Outcome) -> String {
    format!(
        "exit code {:?}\n--- stdout ---\n{}\n--- stderr ---\n{}",
        outcome.code, outcome.stdout, outcome.stderr
    )
}

/// True once the process is gone or only waiting to be collected.
fn has_ended(pid: u32) -> bool {
    match Command::new("ps")
        .args(["-o", "stat=", "-p", &pid.to_string()])
        .output()
    {
        Ok(output) => {
            let stat = String::from_utf8_lossy(&output.stdout);
            let stat = stat.trim();
            stat.is_empty() || stat.starts_with('Z')
        }
        Err(_) => false,
    }
}

/// Starts `smoke` against a fake whose panes never start a talaria process, waits (in real time)
/// until the launch line was typed, sends the signal and returns what the process did.
fn run_and_signal(signal: &str) -> (Harness, support::Outcome) {
    let harness = Harness::plain();
    let mut running = harness.cmd().step_scale(0.2).spawn();
    let pid = running.pid();
    let deadline = Instant::now() + Duration::from_secs(25);
    let mut typed = false;
    while Instant::now() < deadline {
        if harness.fake.saw(&["pane", "run"]) {
            typed = true;
            break;
        }
        if has_ended(pid) {
            break;
        }
        sleep(Duration::from_millis(20));
    }
    if !typed {
        running.kill();
        let outcome = running.finish();
        panic!(
            "smoke never typed the launch line (it ended or hung before `pane run`); the fake saw {:?}\n{}",
            harness.fake.log(),
            explain(&outcome)
        );
    }
    running.signal(signal);
    let outcome = running.finish();
    (harness, outcome)
}

fn assert_interrupted(signal: &str) {
    let (harness, outcome) = run_and_signal(signal);

    assert_eq!(outcome.code, Some(1), "{signal}: {}", explain(&outcome));
    assert_eq!(
        outcome.last_line(),
        Some("SMOKE_RESULT: FAILED (interrupted)"),
        "{signal}: {}",
        explain(&outcome)
    );
    assert!(
        outcome
            .lines()
            .contains(&"FAIL startup (interrupted by a signal)"),
        "{signal}: the step in progress fails; {}",
        explain(&outcome)
    );
    let log = harness.fake.log();
    let close = log
        .iter()
        .position(|args| args.starts_with(&["tab".to_owned(), "close".to_owned()]))
        .unwrap_or_else(|| panic!("{signal}: the fake never saw `tab close`; log: {log:?}"));
    assert_eq!(
        harness.fake.calls_starting(&["tab", "close"]).len(),
        1,
        "{signal}: the tab is closed once"
    );
    assert!(
        log[close + 1..]
            .iter()
            .any(|args| args.starts_with(&["tab".to_owned(), "list".to_owned()])),
        "{signal}: the tab is checked to be gone after the close; log: {log:?}"
    );
    assert!(
        harness.scratch_dirs().is_empty(),
        "{signal}: the scratch project is removed: {:?}",
        harness.scratch_dirs()
    );
}

#[test]
fn tab_closed_after_pass() {
    let bench = Bench::new();
    let played = play(&bench, &Scenario::happy(), &Plan::default());

    assert_eq!(played.report.exit_code(), 0, "lines:\n{}", played.lines());
    assert!(played.report.unclosed_tab.is_none());
    assert_eq!(played.closes(), 1, "calls: {:?}", played.kinds());
    let kinds = played.kinds();
    let close = kinds.iter().rposition(|k| *k == "tab close").unwrap_or(0);
    assert!(
        kinds[close + 1..].contains(&"tab list"),
        "the tab is checked to be gone after the close; calls: {kinds:?}"
    );
    assert!(
        played
            .report
            .result_line()
            .starts_with("SMOKE_RESULT: PASSED ("),
        "{}",
        played.report.result_line()
    );
}

#[test]
fn tab_closed_after_failure() {
    let bench = Bench::new();
    let scenario = Scenario::happy().startup(Startup::ExitsWith(7));
    let played = play(&bench, &scenario, &Plan::default());

    assert_last_failure(
        &played,
        "startup",
        "talaria exited with code 7 before showing its screen",
    );
    assert_eq!(played.report.exit_code(), 1);
    assert_eq!(
        played.report.result_line(),
        "SMOKE_RESULT: FAILED (startup)"
    );
    assert_eq!(played.closes(), 1, "calls: {:?}", played.kinds());
    assert!(played.report.unclosed_tab.is_none());
}

#[test]
fn tab_closed_after_panic() {
    let bench = Bench::new();
    let plan = Plan {
        panic_when: Some(panics_on_launch),
        ..Plan::default()
    };
    let played = play(&bench, &Scenario::happy(), &plan);

    assert_last_failure(
        &played,
        "startup",
        &format!("internal error: {PANIC_MESSAGE}"),
    );
    assert!(
        played
            .lines()
            .contains(&format!("FAIL startup (internal error: {PANIC_MESSAGE})")),
        "lines:\n{}",
        played.lines()
    );
    assert_eq!(played.report.exit_code(), 1);
    assert_eq!(
        played.report.result_line(),
        "SMOKE_RESULT: FAILED (startup)"
    );
    assert_eq!(played.closes(), 1, "calls: {:?}", played.kinds());
    assert!(played.report.unclosed_tab.is_none());
}

#[test]
fn tab_closed_after_panic_in_a_later_step() {
    let bench = Bench::new();
    let plan = Plan {
        panic_when: Some(panics_on_first_prompt),
        ..Plan::default()
    };
    let played = play(&bench, &Scenario::happy(), &plan);

    assert!(
        played.lines().contains("PASS startup "),
        "the step before the panic still passed; lines:\n{}",
        played.lines()
    );
    assert_last_failure(
        &played,
        "plain-answer",
        &format!("internal error: {PANIC_MESSAGE}"),
    );
    assert_eq!(
        played.report.result_line(),
        "SMOKE_RESULT: FAILED (plain-answer)"
    );
    assert_eq!(played.closes(), 1, "calls: {:?}", played.kinds());
}

#[test]
fn tab_closed_after_sigint() {
    assert_interrupted("INT");
}

#[test]
fn tab_closed_after_sigterm() {
    assert_interrupted("TERM");
}

#[test]
fn unclosed_tab_ends_failed_cleanup() {
    // The binary: a quick failing startup, and a `tab close` the fake refuses every time.
    let scenario = Scenario::happy().startup(Startup::ExitsWith(7));
    let script = scenario
        .script()
        .fail(FailBuilder::call("tab close", "internal_error").message("cannot close the tab"));
    let harness = Harness::new(&script);

    let outcome = harness.cmd().run();

    assert_eq!(outcome.code, Some(1), "{}", explain(&outcome));
    assert!(
        outcome.lines().contains(&UNCLOSED_TAB_LINE),
        "{}",
        explain(&outcome)
    );
    assert_eq!(
        outcome.last_line(),
        Some("SMOKE_RESULT: FAILED (cleanup)"),
        "an unclosed tab outranks the failing step; {}",
        explain(&outcome)
    );
    assert!(
        outcome
            .lines()
            .iter()
            .any(|l| l.starts_with("FAIL startup (")),
        "the earlier lines still print; {}",
        explain(&outcome)
    );
    assert_eq!(
        outcome
            .lines()
            .iter()
            .filter(|l| l.starts_with("SMOKE_RESULT:"))
            .count(),
        1,
        "{}",
        explain(&outcome)
    );
    assert!(
        harness.scratch_dirs().is_empty(),
        "the scratch project goes even when the tab stays"
    );
}

#[test]
fn unclosed_tab_after_every_step_passed() {
    let bench = Bench::new();
    let plan = Plan {
        keep_tab: true,
        ..Plan::default()
    };
    let played = play(&bench, &Scenario::happy(), &plan);

    assert!(
        played.lines().lines().all(|l| !l.starts_with("FAIL ")),
        "every step passed; lines:\n{}",
        played.lines()
    );
    assert_eq!(
        played.report.unclosed_tab,
        Some(smoke::TabId("w1:t5".to_owned()))
    );
    assert_eq!(played.report.exit_code(), 1);
    assert_eq!(
        played.report.result_line(),
        "SMOKE_RESULT: FAILED (cleanup)"
    );
    assert!(
        bench.scratch_dirs().is_empty(),
        "the scratch project is removed: {:?}",
        bench.scratch_dirs()
    );
}

#[test]
fn scratch_removed_on_every_path() {
    let panic_in_startup = Plan {
        panic_when: Some(panics_on_launch),
        ..Plan::default()
    };
    let unclosed = Plan {
        keep_tab: true,
        ..Plan::default()
    };
    let cases = [
        ("pass", Scenario::happy(), Plan::default()),
        (
            "failure",
            Scenario::happy().startup(Startup::ExitsWith(7)),
            Plan::default(),
        ),
        ("panic", Scenario::happy(), panic_in_startup),
        ("unclosed tab", Scenario::happy(), unclosed),
    ];
    for (name, scenario, plan) in cases {
        let bench = Bench::new();
        let played = play(&bench, &scenario, &plan);

        let scratch = played.scratch();
        let name_ok = scratch
            .file_name()
            .and_then(|n| n.to_str())
            .is_some_and(|n| {
                n.starts_with("talaria-smoke-") && n.len() == "talaria-smoke-".len() + 8
            });
        assert!(name_ok, "{name}: scratch folder name, got {scratch:?}");
        let parent = scratch.parent().expect("the scratch folder has a parent");
        assert_eq!(
            parent.canonicalize().expect("the parent exists"),
            bench.temp_root().canonicalize().expect("the root exists"),
            "{name}: the scratch project sits in the system temporary directory"
        );
        assert!(!scratch.exists(), "{name}: {scratch:?} must be removed");
        assert!(
            bench.scratch_dirs().is_empty(),
            "{name}: nothing is left behind: {:?}",
            bench.scratch_dirs()
        );
    }

    // The same through the binary: a failing run leaves nothing in its temporary directory.
    let harness = Harness::from_scenario(&Scenario::happy().startup(Startup::ExitsWith(7)));
    let outcome = harness.cmd().run();
    assert_eq!(outcome.code, Some(1), "{}", explain(&outcome));
    assert!(
        harness.fake.saw(&["tab", "create"]),
        "the run did create its tab"
    );
    assert!(
        harness.scratch_dirs().is_empty(),
        "binary run: {:?}",
        harness.scratch_dirs()
    );
}
