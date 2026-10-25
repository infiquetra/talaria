//! The built `smoke` binary against the fake multiplexer: the new suite option,
//! preflight, result lines and exit codes, and the failure report. All fail until the
//! dispatch in `src/main.rs` and the options exist, except `default_suite_is_phase1`,
//! which guards behaviour that already exists.

mod support;

use std::fs;
use std::path::{Path, PathBuf};

use smoke::Suite;
use support::scripts2b::{Shapes, Step, script_2b};
use support::{FailBuilder, Harness, Outcome, RuleBuilder, Scenario, ScriptBuilder, Startup};

fn explain(outcome: &Outcome) -> String {
    format!(
        "exit code {:?}\n--- stdout ---\n{}\n--- stderr ---\n{}",
        outcome.code, outcome.stdout, outcome.stderr
    )
}

fn everything(outcome: &Outcome) -> String {
    format!("{}\n{}", outcome.stdout, outcome.stderr)
}

/// A run that ends at the first step: the binary exits with code 1 at once.
fn quick_failure() -> Harness {
    Harness::from_scenario(&Scenario::happy().startup(Startup::ExitsWith(7)))
}

fn find_report(root: &Path) -> PathBuf {
    let mut found = Vec::new();
    let mut pending = vec![root.to_path_buf()];
    while let Some(dir) = pending.pop() {
        for entry in fs::read_dir(&dir).expect("read a folder").flatten() {
            let path = entry.path();
            if path.is_dir() {
                pending.push(path);
            } else if path.file_name().is_some_and(|n| n == "smoke-report.txt") {
                found.push(path);
            }
        }
    }
    assert_eq!(found.len(), 1, "exactly one report is written: {found:?}");
    found.remove(0)
}

#[test]
fn help_lists_the_three_suites() {
    let harness = Harness::plain();
    let outcome = harness.bare_cmd().arg("run").arg("--help").run();
    assert_eq!(outcome.code, Some(0), "{}", explain(&outcome));
    for suite in ["phase1", "phase2a", "phase2b"] {
        assert!(
            outcome.stdout.contains(suite),
            "help lists {suite}:\n{}",
            outcome.stdout
        );
    }
}

#[test]
fn unknown_suite_is_exit_2() {
    let harness = Harness::plain();
    let outcome = harness.cmd().arg("--suite").arg("bogus").run();
    assert_eq!(outcome.code, Some(2), "{}", explain(&outcome));
    // The parse text stays as it is: the phase 2a suite pins it exactly.
    assert!(
        everything(&outcome).contains("unknown suite 'bogus'; use phase1 or phase2a"),
        "the parse text is shown: {}",
        explain(&outcome)
    );
}

#[test]
fn default_suite_is_phase1() {
    // No --suite flag: the run behaves as the first suite. The byte-for-byte comparison
    // against an explicit --suite phase1 waits for green (the suite parser is a stub).
    let default = quick_failure().cmd().run();
    assert_eq!(default.code, Some(1), "{}", explain(&default));
    assert!(
        default.stdout.contains("SMOKE_RESULT: FAILED (startup)"),
        "the default run behaves as the first suite: {}",
        explain(&default)
    );
}

#[test]
fn phase2b_default_deadline_is_1500() {
    assert_eq!(Suite::Phase2b.default_deadline_secs(), 1500);
    assert_eq!(Suite::Phase1.default_deadline_secs(), 300);
    assert_eq!(Suite::Phase2a.default_deadline_secs(), 1800);
}

#[test]
fn phase2b_preflight_needs_no_tmux() {
    let outcome = quick_failure().cmd().arg("--suite").arg("phase2b").run();
    assert_eq!(outcome.code, Some(1), "{}", explain(&outcome));
    assert!(
        outcome.stdout.contains("SMOKE_RESULT: FAILED (startup)"),
        "no tmux preflight stops the run: {}",
        explain(&outcome)
    );
}

#[test]
fn phase2b_prints_fifteen_pass_lines_on_fake_herdr() {
    let script = script_2b(&Shapes::default(), Step::Headless);
    let harness = Harness::new(&script);
    let outcome = harness.cmd().arg("--suite").arg("phase2b").run();
    assert_eq!(outcome.code, Some(0), "{}", explain(&outcome));
    let passes = outcome
        .stdout
        .lines()
        .filter(|line| line.starts_with("PASS "))
        .count();
    assert_eq!(passes, 15, "fifteen pass lines: {}", explain(&outcome));
    assert!(
        outcome.stdout.contains("SMOKE_RESULT: PASSED"),
        "the result line passes: {}",
        explain(&outcome)
    );
}

#[test]
fn phase2b_exit_codes() {
    let sentence = "phase2b needs the default provider and model (ollama-cloud, minimax-m3.1-flash): its expected texts name them";
    for (flag, value) in [("--model", "other-model"), ("--provider", "other-provider")] {
        let outcome = Harness::plain()
            .cmd()
            .arg("--suite")
            .arg("phase2b")
            .arg(flag)
            .arg(value)
            .run();
        assert_eq!(outcome.code, Some(2), "{flag}: {}", explain(&outcome));
        assert!(
            everything(&outcome).contains(sentence),
            "{flag} says the sentence: {}",
            explain(&outcome)
        );
        assert!(
            !everything(&outcome).contains("SMOKE_RESULT"),
            "{flag} prints no result line: {}",
            explain(&outcome)
        );
    }

    let outcome = quick_failure().cmd().arg("--suite").arg("phase2b").run();
    assert_eq!(outcome.code, Some(1), "{}", explain(&outcome));
    assert!(
        outcome.stdout.contains("SMOKE_RESULT: FAILED (startup)"),
        "a failing step exits 1: {}",
        explain(&outcome)
    );

    let script = script_2b(&Shapes::default(), Step::Permission)
        .fail(FailBuilder::call("pane send-keys", "invalid_key").message("unsupported key"));
    let harness = Harness::new(&script);
    let outcome = harness.cmd().arg("--suite").arg("phase2b").run();
    assert_eq!(outcome.code, Some(3), "{}", explain(&outcome));
    assert!(
        outcome
            .stdout
            .contains("SMOKE_RESULT: PASSED WITH SKIPS (permission)"),
        "a skip exits 3: {}",
        explain(&outcome)
    );
}

#[test]
fn phase2b_failure_report_is_scrubbed() {
    // Run 1 names the report folder, run 2 uses the default one.
    for explicit in [true, false] {
        let harness = Harness::plain();
        let tmp = harness.tmp_dir();
        let tmp_real = tmp.canonicalize().expect("tmp exists");
        let home = harness.home_dir();
        let hermes = harness.root().join("hermes-data");
        fs::create_dir(&hermes).expect("hermes home");
        let user = harness.user().to_owned();

        // The pane shows private values, as a shell prompt and a stack trace would.
        let planted = format!(
            "$0\nlogin={user} home={} tmp={}/stuff tmp_real={}/stuff hermes={}\n",
            home.display(),
            tmp.display(),
            tmp_real.display(),
            hermes.display(),
        );
        let script = ScriptBuilder::new()
            .on_first(RuleBuilder::run("--no-ensure --profile ").append("$0\ntalaria\nidle"))
            .on_first(RuleBuilder::run(r"^/context$").append(&planted));
        fs::write(harness.fake.dir().join("script.json"), script.to_json()).expect("script");
        let reports = tmp.join("reports");
        fs::create_dir(&reports).expect("report folder");

        let mut cmd = harness.cmd().env("HERMES_HOME", &hermes);
        cmd = cmd.arg("--suite").arg("phase2b");
        if explicit {
            cmd = cmd.arg("--report-dir").arg(&reports);
        }
        let outcome = cmd.run();

        let name = if explicit {
            "named folder"
        } else {
            "default folder"
        };
        assert_eq!(outcome.code, Some(1), "{name}: {}", explain(&outcome));
        let report_path = if explicit {
            reports.join("smoke-report.txt")
        } else {
            find_report(&tmp)
        };
        let report = fs::read_to_string(&report_path)
            .unwrap_or_else(|e| panic!("{name}: no report at {report_path:?}: {e}"));

        let needles = [
            ("login name", user),
            ("home", home.display().to_string()),
            ("tmp", tmp.display().to_string()),
            ("real tmp", tmp_real.display().to_string()),
            ("hermes home", hermes.display().to_string()),
        ];
        let printed = everything(&outcome);
        for (kind, needle) in &needles {
            assert!(
                !printed.contains(needle.as_str()),
                "{name}: the {kind} leaked into the output; {}",
                explain(&outcome)
            );
            assert!(
                !report.contains(needle.as_str()),
                "{name}: the {kind} leaked into the report:\n{report}"
            );
        }
    }
}
