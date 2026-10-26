//! The built `smoke` binary against the fake multiplexer: the phase 3 suite option,
//! preflight and exit codes.
//!
//! `default_suite_is_phase1` and `unknown_suite_exits_2_with_the_parse_text` guard behaviour
//! that already exists. The other tests fail until phase 3 is accepted.

mod support;

use smoke::Suite;
use support::{Harness, Outcome, Startup};

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
    Harness::from_scenario(&support::Scenario::happy().startup(Startup::ExitsWith(7)))
}

const PHASE3_PROFILE_SENTENCE: &str = "phase3 needs the default profile, provider and model (fixture, ollama-cloud, minimax-m3.1-flash): its expected texts name them";
const PHASE3_LONG_LINE: &str = "a phase 3 shell line is longer than 600 characters; use a shorter temporary directory or binary path";

#[test]
fn help_lists_the_four_suites() {
    let harness = Harness::plain();
    let outcome = harness.bare_cmd().arg("run").arg("--help").run();
    assert_eq!(outcome.code, Some(0), "{}", explain(&outcome));
    for suite in ["phase1", "phase2a", "phase2b", "phase3"] {
        assert!(
            outcome.stdout.contains(suite),
            "help lists {suite}:\n{}",
            outcome.stdout
        );
    }
}

#[test]
fn phase3_is_accepted() {
    let outcome = quick_failure().cmd().arg("--suite").arg("phase3").run();
    assert_eq!(outcome.code, Some(1), "{}", explain(&outcome));
    assert!(
        outcome.stdout.contains("SMOKE_RESULT: FAILED (startup)"),
        "phase3 runs and stops at the first step: {}",
        explain(&outcome)
    );
}

#[test]
fn default_suite_is_phase1() {
    let default = quick_failure().cmd().run();
    assert_eq!(default.code, Some(1), "{}", explain(&default));
    assert!(
        default.stdout.contains("SMOKE_RESULT: FAILED (startup)"),
        "the default run behaves as the first suite: {}",
        explain(&default)
    );
}

#[test]
fn unknown_suite_exits_2_with_the_parse_text() {
    let harness = Harness::plain();
    let outcome = harness.cmd().arg("--suite").arg("bogus").run();
    assert_eq!(outcome.code, Some(2), "{}", explain(&outcome));
    assert!(
        everything(&outcome).contains("unknown suite 'bogus'; use phase1 or phase2a"),
        "the parse text is shown: {}",
        explain(&outcome)
    );
}

#[test]
fn phase3_exit_codes() {
    let outcome = Harness::plain()
        .cmd()
        .arg("--suite")
        .arg("phase3")
        .arg("--model")
        .arg("other-model")
        .run();
    assert_eq!(outcome.code, Some(2), "{}", explain(&outcome));
    assert!(
        everything(&outcome).contains(PHASE3_PROFILE_SENTENCE),
        "a foreign model is refused before the run: {}",
        explain(&outcome)
    );
    assert!(
        !everything(&outcome).contains("SMOKE_RESULT"),
        "a preflight failure prints no result line: {}",
        explain(&outcome)
    );

    let outcome = quick_failure().cmd().arg("--suite").arg("phase3").run();
    assert_eq!(outcome.code, Some(1), "{}", explain(&outcome));
    assert!(
        outcome.stdout.contains("SMOKE_RESULT: FAILED (startup)"),
        "a failing step exits 1: {}",
        explain(&outcome)
    );

    let outcome = quick_failure()
        .cmd()
        .arg("--suite")
        .arg("phase3")
        .env("TALARIA_LIVE_REQUIRE", "1")
        .run();
    assert_ne!(
        outcome.code,
        Some(0),
        "require does not turn a failure into a pass: {}",
        explain(&outcome)
    );
}

#[test]
fn phase3_with_another_profile_model_or_provider_exits_2() {
    for (flag, value) in [
        ("--profile", "other"),
        ("--model", "other-model"),
        ("--provider", "other-provider"),
    ] {
        let outcome = Harness::plain()
            .cmd()
            .arg("--suite")
            .arg("phase3")
            .arg(flag)
            .arg(value)
            .run();
        assert_eq!(outcome.code, Some(2), "{flag}: {}", explain(&outcome));
        assert!(
            everything(&outcome).contains(PHASE3_PROFILE_SENTENCE),
            "{flag} says the sentence: {}",
            explain(&outcome)
        );
        assert!(
            !everything(&outcome).contains("SMOKE_RESULT"),
            "{flag} prints no result line: {}",
            explain(&outcome)
        );
    }
}

#[test]
fn phase3_needs_no_tmux() {
    let outcome = quick_failure().cmd().arg("--suite").arg("phase3").run();
    assert_eq!(outcome.code, Some(1), "{}", explain(&outcome));
    assert!(
        outcome.stdout.contains("SMOKE_RESULT: FAILED (startup)"),
        "no tmux preflight stops the run: {}",
        explain(&outcome)
    );
    assert!(
        !everything(&outcome).to_lowercase().contains("tmux"),
        "the refusal does not mention tmux: {}",
        explain(&outcome)
    );
}

#[test]
fn phase3_default_deadline_is_1200() {
    assert_eq!(Suite::Phase3.default_deadline_secs(), 1200);
}

#[test]
fn phase3_over_long_line_exits_2() {
    let long = format!("/{}", "x".repeat(620));
    let outcome = Harness::plain()
        .cmd_without_talaria()
        .arg("--suite")
        .arg("phase3")
        .arg("--talaria")
        .arg(long)
        .run();
    assert_eq!(outcome.code, Some(2), "{}", explain(&outcome));
    assert!(
        everything(&outcome).contains(PHASE3_LONG_LINE),
        "a long shell line is refused: {}",
        explain(&outcome)
    );
    assert!(
        !everything(&outcome).contains("SMOKE_RESULT"),
        "the refusal prints no result line: {}",
        explain(&outcome)
    );
}
