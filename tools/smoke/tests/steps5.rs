//! Ordered workflow smoke steps, failure reasons, and terminal boundaries.
#[path = "support/scripts5.rs"]
mod scripts5;
mod support;

use scripts5::{Exec5, LIMITS, STEPS, exec_5, script_5};
use smoke::{ReadSource, StepOutcome};
use support::Call;

fn run(modes: &[&str]) -> Exec5 {
    exec_5(&script_5(modes), false, false, 1.0, 600)
}

fn sent(call: &Call) -> Option<&str> {
    match call {
        Call::Run { text, .. } | Call::SendText { text, .. } => Some(text),
        _ => None,
    }
}

fn passes(step: &str) {
    let exec = run(&[]);
    let index = STEPS.iter().position(|name| *name == step).unwrap();
    assert!(
        matches!(exec.report.outcomes.get(index), Some(StepOutcome::Pass { step: got, elapsed }) if *got == step && elapsed.as_secs() <= LIMITS[index]),
        "{}",
        exec.lines()
    );
    assert_eq!(exec.report.exit_code(), 0, "{}", exec.lines());
    assert_eq!(exec.close_tabs(), 1);
    let calls = exec.term.calls();
    let text: Vec<_> = calls.iter().filter_map(|entry| sent(&entry.call)).collect();
    match step {
        "startup" => assert!(text.iter().any(|line| line.contains("--no-ensure")
            && line.contains("--profile fixture")
            && line.contains("--model minimax-m3.1-flash")
            && line.contains("--provider ollama-cloud"))),
        "plain-answer" | "resume" => {
            let tokens = exec.term.memory();
            assert_eq!(tokens.len(), 2);
            assert_ne!(tokens[0], tokens[1]);
            if step == "resume" {
                assert!(
                    text.iter()
                        .any(|line| line.contains("--no-ensure -c --profile fixture"))
                );
            }
        }
        "workflows-empty" => assert!(text.contains(&"/workflows")),
        "ultracode-status" => assert!(text.contains(&"/ultracode status")),
        "quit" => assert!(text.iter().filter(|line| **line == "/quit").count() >= 2),
        "builtins-cli" => {
            assert_eq!(
                text.iter()
                    .filter(|line| line.contains("workflow builtins install"))
                    .count(),
                2
            );
            assert!(
                text.iter()
                    .any(|line| line.contains("workflow builtins list"))
            );
            assert!(text.iter().any(
                |line| line.contains("workflow builtins check review-branch")
                    && line.contains("--staffing")
            ));
        }
        "roster-cli" => assert!(text.iter().any(|line| line.contains("workflow roster")
            && line.contains("--profile fixture")
            && line.contains("--json")
            && line.contains("jq"))),
        "list-cli" => {
            assert!(
                text.iter()
                    .any(|line| line.contains("workflow list") && !line.contains("--json"))
            );
            assert!(
                text.iter()
                    .any(|line| line.contains("workflow list") && line.contains("--json"))
            );
        }
        "no-gateway" => assert!(text.iter().any(|line| line.contains("HERMES_HOME=")
            && line.contains("workflow run")
            && line.contains("one.js"))),
        _ => unreachable!(),
    }
}

fn fails(step: &str, cases: &[(&[&str], &str)]) {
    for (modes, reason) in cases {
        let exec = run(modes);
        let index = STEPS.iter().position(|name| *name == step).unwrap();
        assert_eq!(
            exec.report.outcomes.len(),
            index + 1,
            "{modes:?}: {}",
            exec.lines()
        );
        assert!(
            exec.report.outcomes[..index]
                .iter()
                .all(|outcome| matches!(outcome, StepOutcome::Pass { .. })),
            "{}",
            exec.lines()
        );
        assert!(
            matches!(exec.report.outcomes.last(), Some(StepOutcome::Fail { step: got, reason: got_reason, .. }) if *got == step && got_reason == reason),
            "{modes:?}: {}",
            exec.lines()
        );
        assert_eq!(
            exec.lines().lines().last(),
            Some(format!("FAIL {step} ({reason})").as_str())
        );
        assert_eq!(exec.report.exit_code(), 1);
        assert_eq!(exec.close_tabs(), 1);
        assert!(!exec.report.scratch.exists());
        assert_eq!(std::fs::read_dir(&exec.root).unwrap().count(), 0);
    }
}

macro_rules! pair {
    ($pass:ident, $fail:ident, $step:literal, [$($modes:expr => $reason:literal),+ $(,)?]) => {
        #[test] fn $pass() { passes($step); }
        #[test] fn $fail() { fails($step, &[$(($modes, $reason)),+]); }
    };
}

pair!(startup_passes, startup_fails, "startup", [
    &["startup-exit-1"] => "talaria exited with code 1 before showing its screen",
    &["startup-exit"] => "talaria exited with code 7 before showing its screen",
    &["startup-missing"] => "the composer was not shown",
]);
pair!(plain_answer_passes, plain_answer_fails, "plain-answer", [
    &["reply-missing"] => "the reply did not carry the token",
    &["reply-busy"] => "a turn did not settle",
]);
pair!(workflows_empty_passes, workflows_empty_fails, "workflows-empty", [
    &["workflows-missing"] => "the empty-runs text was not shown",
    &["workflows-stuck"] => "Esc did not return to the idle composer",
    &["workflows-missing", "workflows-stuck"] => "the empty-runs text was not shown",
    &["before-workflows"] => "the step did not start idle",
    &["after-workflows"] => "the step did not end idle",
]);
pair!(ultracode_status_passes, ultracode_status_fails, "ultracode-status", [
    &["status-missing"] => "the status text was not shown",
    &["turn-started"] => "a turn started",
    &["toggle-on"] => "the toggle did not stay off",
    &["status-missing", "turn-started", "toggle-on"] => "the status text was not shown",
    &["turn-started", "toggle-on"] => "a turn started",
    &["before-status"] => "the step did not start idle",
    &["after-status"] => "the step did not end idle",
]);
pair!(quit_passes, quit_fails, "quit", [
    &["quit-stuck"] => "Talaria did not exit",
    &["token-lost"] => "the token did not survive",
]);
pair!(resume_passes, resume_fails, "resume", [
    &["resume-token-missing"] => "the token was not shown after resume",
    &["resume-reply-missing"] => "the fresh token did not answer",
    &["resume-quit-stuck"] => "Talaria did not exit on /quit",
    &["resume-shell-missing"] => "the shell was not shown",
]);
pair!(builtins_cli_passes, builtins_cli_fails, "builtins-cli", [
    &["install-bad"] => "install did not print the expected lines",
    &["install-again-bad"] => "install did not print the expected lines",
    &["install-missing-one"] => "install did not print the expected lines",
    &["install-again-missing-one"] => "install did not print the expected lines",
    &["builtins-list-bad"] => "list did not show the four workflows",
    &["check-bad"] => "check did not refuse the missing tiers",
    &["install-bad", "builtins-list-bad", "check-bad"] => "install did not print the expected lines",
]);
pair!(roster_cli_passes, roster_cli_fails, "roster-cli", [
    &["roster-invalid"] => "roster did not exit 0 or parse",
    &["roster-exit"] => "roster did not exit 0 or parse",
    &["roster-missing-talaria_home"] => "a required path field is missing or outside the scratch configuration",
    &["roster-missing-runs_dir"] => "a required path field is missing or outside the scratch configuration",
    &["roster-missing-workflows_dir"] => "a required path field is missing or outside the scratch configuration",
    &["roster-missing-drafts_dir"] => "a required path field is missing or outside the scratch configuration",
    &["roster-missing-workflow"] => "a required path field is missing or outside the scratch configuration",
    &["roster-outside-talaria_home"] => "a required path field is missing or outside the scratch configuration",
    &["roster-outside-runs_dir"] => "a required path field is missing or outside the scratch configuration",
    &["roster-outside-workflows_dir"] => "a required path field is missing or outside the scratch configuration",
    &["roster-outside-drafts_dir"] => "a required path field is missing or outside the scratch configuration",
    &["roster-outside-workflow"] => "a required path field is missing or outside the scratch configuration",
    &["roster-exit", "roster-outside-drafts_dir"] => "roster did not exit 0 or parse",
]);
pair!(list_cli_passes, list_cli_fails, "list-cli", [
    &["list-text-bad"] => "the empty-list text was not shown",
    &["list-json-bad"] => "the JSON list was not empty",
    &["list-text-bad", "list-json-bad"] => "the empty-list text was not shown",
]);
pair!(no_gateway_passes, no_gateway_fails, "no-gateway", [
    &["gateway-incorrect"] => "run did not fail with the gateway line",
    &["gateway-bad-exit"] => "run did not fail with the gateway line",
    &["gateway-missing-line"] => "run did not fail with the gateway line",
    &["created-run"] => "run created a run folder",
    &["gateway-list-fails"] => "list did not exit 0",
    &["gateway-incorrect", "created-run", "gateway-list-fails"] => "run did not fail with the gateway line",
]);

#[test]
fn a_full_run_returns_to_idle_between_steps() {
    let exec = run(&[]);
    let names: Vec<_> = exec
        .report
        .outcomes
        .iter()
        .map(|outcome| match outcome {
            StepOutcome::Pass { step, .. } => *step,
            _ => panic!("{}", exec.lines()),
        })
        .collect();
    assert_eq!(names, STEPS);
    let calls = exec.term.calls();
    for (open, next) in [
        ("/workflows", "/ultracode status"),
        ("/ultracode status", "/quit"),
    ] {
        let start = calls
            .iter()
            .position(|entry| sent(&entry.call) == Some(open))
            .expect("screen command");
        let end = calls[start + 1..]
            .iter()
            .position(|entry| sent(&entry.call) == Some(next))
            .map(|offset| start + 1 + offset)
            .expect("next command");
        let before = exec
            .reads
            .iter()
            .rfind(|read| read.index < start && read.source == ReadSource::Visible)
            .expect("pre-step screen");
        let after = exec
            .reads
            .iter()
            .rfind(|read| {
                read.index > start && read.index < end && read.source == ReadSource::Visible
            })
            .expect("post-step screen");
        assert!(
            smoke::screen5::idle_screen(&before.text),
            "before {open}: {}",
            before.text
        );
        assert!(
            smoke::screen5::idle_screen(&after.text),
            "after {open}: {}",
            after.text
        );
        assert!(
            !calls[after.index + 1..end]
                .iter()
                .any(|entry| matches!(entry.call, Call::SendText { .. } | Call::SendKeys { .. }))
        );
    }
    let second_quit = calls
        .iter()
        .rposition(|entry| sent(&entry.call) == Some("/quit"))
        .expect("resumed quit");
    let first_command = calls
        .iter()
        .position(|entry| {
            sent(&entry.call).is_some_and(|line| line.contains("workflow builtins install"))
        })
        .expect("first shell command");
    assert!(second_quit < first_command);
    assert!(
        exec.reads.iter().any(|read| read.index > second_quit
            && read.index < first_command
            && read.pane == support::second_pane().0
            && read.text.contains("$ ")),
        "read the shell before running commands"
    );
    assert_eq!(exec.report.exit_code(), 0);
}

#[test]
fn a_failing_step_stops_the_run_and_cleans_up() {
    let exec = run(&["status-missing"]);
    assert_eq!(exec.report.outcomes.len(), 4);
    assert!(matches!(
        exec.report.outcomes.last(),
        Some(StepOutcome::Fail {
            step: "ultracode-status",
            ..
        })
    ));
    if let Some(StepOutcome::Fail { capture, .. }) = exec.report.outcomes.last() {
        assert!(
            capture
                .iter()
                .any(|pane| pane.visible.contains("status unavailable")
                    || pane.recent.contains("status unavailable")),
            "the differing pane text must be captured"
        );
    }
    assert!(
        !exec
            .term
            .everything_typed()
            .iter()
            .any(|line| line == "/quit" || line.contains("workflow builtins"))
    );
    assert_eq!(exec.close_tabs(), 1);
    assert!(!exec.report.scratch.exists());
    assert_eq!(std::fs::read_dir(&exec.root).unwrap().count(), 0);
}

#[test]
fn deadline_names_the_step() {
    let exec = exec_5(&script_5(&["reply-missing"]), false, false, 1.0, 5);
    assert!(
        exec.lines()
            .contains("FAIL plain-answer (deadline reached)"),
        "{}",
        exec.lines()
    );
    assert_eq!(exec.report.exit_code(), 1);
    assert_eq!(exec.close_tabs(), 1);
}

#[test]
fn a_missing_precondition_is_a_fail_not_a_skip() {
    let exec = run(&["missing-precondition"]);
    assert!(matches!(
        exec.report.outcomes.last(),
        Some(StepOutcome::Fail {
            step: "startup",
            ..
        })
    ));
    assert!(!exec.lines().contains("SKIP"));
    assert_eq!(exec.report.exit_code(), 1);
    assert_eq!(exec.close_tabs(), 1);
}

#[test]
fn live_require_does_not_change_a_phase5_result() {
    for modes in [
        &[][..],
        &["missing-precondition"][..],
        &["workflows-missing"][..],
        &["gateway-incorrect"][..],
    ] {
        let normal = exec_5(&script_5(modes), false, false, 1.0, 600);
        let required = exec_5(&script_5(modes), true, false, 1.0, 600);
        assert_eq!(normal.lines(), required.lines());
        assert_eq!(normal.report.exit_code(), required.report.exit_code());
        assert_ne!(normal.report.exit_code(), 3);
    }
}
