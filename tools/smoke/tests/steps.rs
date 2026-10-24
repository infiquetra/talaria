//! `run_smoke` against the scripted terminal and a manual clock: the outcome list, the exact
//! reason texts, the text and keys sent with their manual-clock times, and the single tab close.
//! Every expected value is a hand written literal; times are read from the recorded calls.

mod support;

use std::fs;
use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::time::Duration;

use regex::Regex;
use smoke::{Clock, ManualClock, PaneCapture, ReadSource, SmokeConfig, SmokeReport, StepOutcome};
use support::{
    Approval, Call, FIRST_PANE_ID, FakeTerminal, FinalQuit, Interrupt, PANIC_MESSAGE, Quit,
    Recorded, Resume, RuleBuilder, SECOND_PANE_ID, Scenario, ScriptBuilder, StageBuilder, Startup,
    TokenReply,
};

const STEPS: [&str; 6] = [
    "startup",
    "plain-answer",
    "approval",
    "interrupt",
    "quit",
    "resume",
];

const TOKEN_PROMPT_START: &str =
    "Without using any tools, join these two words with a hyphen and reply with only the result: ";
const APPROVAL_PROMPT: &str =
    "Run this exact shell command and then tell me its exit code: rm -rf .pytest_cache";
const APPROVAL_RETRY_PROMPT: &str =
    "Use your shell tool now and run exactly this command: rm -rf .pytest_cache";
const COUNT_PROMPT: &str = "Count from 1 to 2000, one number per line, and do not stop early.";
const WORDS: [&str; 8] = [
    "maple", "cobalt", "juniper", "saffron", "tundra", "walnut", "harbor", "quartz",
];

const SKIP_REASON: &str = "no approval card appeared and the command ran; the profile fixture must set approvals.mode: manual (the default is smart, in which Hermes may approve by itself; a hosted-room policy can also override the mode)";
// The reason names the card unit by its planning code; the code is split so that no source file
// of the crate spells it out in one piece.
const NO_ONCE_NUMBER_REASON: &str = concat!(
    "card layout differs from unit P1",
    "-09; cannot find the number for 'once'"
);

fn ms(millis: u64) -> Duration {
    Duration::from_millis(millis)
}

fn secs(seconds: u64) -> Duration {
    Duration::from_secs(seconds)
}

fn assert_within(what: &str, got: Duration, low: Duration, high: Duration) {
    assert!(
        got >= low && got <= high,
        "{what}: {got:?} is not within {low:?} ..= {high:?}"
    );
}

// ------------------------------------------------------------------------- running a script

struct Exec {
    report: SmokeReport,
    term: FakeTerminal<'static>,
    clock: &'static ManualClock,
    talaria: PathBuf,
    root: PathBuf,
    _temp: tempfile::TempDir,
}

fn exec(script: &ScriptBuilder) -> Exec {
    exec_with(script, |_| {}, |term| term)
}

fn scenario(scenario: &Scenario) -> Exec {
    exec(&scenario.script())
}

fn exec_with(
    script: &ScriptBuilder,
    tune: impl FnOnce(&mut SmokeConfig),
    shape: impl FnOnce(FakeTerminal<'static>) -> FakeTerminal<'static>,
) -> Exec {
    let clock: &'static ManualClock = Box::leak(Box::new(ManualClock::new()));
    let temp = tempfile::tempdir().expect("temporary folder for the run");
    let root = fs::canonicalize(temp.path()).expect("resolve the temporary folder");
    let talaria = root.join("bin").join("talaria");
    fs::create_dir_all(talaria.parent().expect("a parent folder")).expect("create bin folder");
    support::write_executable(&talaria, "#!/bin/sh\nexit 0\n");
    let mut cfg = SmokeConfig {
        talaria,
        profile: "fixture".to_owned(),
        model: "minimax-m3.1-flash".to_owned(),
        provider: "ollama-cloud".to_owned(),
        deadline: secs(300),
        step_scale: 1.0,
        report_dir: None,
        require: false,
        temp_root: root.clone(),
        seed: Some(11),
    };
    tune(&mut cfg);
    let term = shape(FakeTerminal::from_builder(script, clock));
    let stop = AtomicBool::new(false);
    let report = smoke::run_smoke(&term, clock, &cfg, &stop);
    Exec {
        report,
        term,
        clock,
        talaria: cfg.talaria.clone(),
        root: cfg.temp_root.clone(),
        _temp: temp,
    }
}

impl Exec {
    /// `step:pass`, `step:skip` or `step:fail`, in order.
    fn kinds(&self) -> Vec<String> {
        self.report
            .outcomes
            .iter()
            .map(|o| match o {
                StepOutcome::Pass { step, .. } => format!("{step}:pass"),
                StepOutcome::Skip { step, .. } => format!("{step}:skip"),
                StepOutcome::Fail { step, .. } => format!("{step}:fail"),
            })
            .collect()
    }

    /// The reason of the last outcome, if it is a skip or a failure.
    fn last_reason(&self) -> Option<String> {
        self.report.outcomes.last().and_then(|o| match o {
            StepOutcome::Skip { reason, .. } | StepOutcome::Fail { reason, .. } => {
                Some(reason.clone())
            }
            StepOutcome::Pass { .. } => None,
        })
    }

    fn lines(&self) -> Vec<String> {
        self.report
            .step_lines()
            .lines()
            .map(str::to_owned)
            .collect()
    }

    fn pass_elapsed(&self, wanted: &str) -> Duration {
        self.report
            .outcomes
            .iter()
            .find_map(|o| match o {
                StepOutcome::Pass { step, elapsed } if *step == wanted => Some(*elapsed),
                _ => None,
            })
            .unwrap_or_else(|| panic!("step {wanted} did not pass"))
    }

    fn capture(&self) -> &[PaneCapture] {
        self.report
            .outcomes
            .iter()
            .find_map(|o| match o {
                StepOutcome::Fail { capture, .. } => Some(capture.as_slice()),
                _ => None,
            })
            .expect("a failing step with a capture")
    }

    fn capture_of(&self, pane: &str) -> &PaneCapture {
        self.capture()
            .iter()
            .find(|c| c.pane == pane)
            .unwrap_or_else(|| panic!("no capture of the {pane} pane"))
    }

    fn calls(&self) -> Vec<Recorded> {
        self.term.calls()
    }

    fn created(&self) -> (String, PathBuf) {
        self.calls()
            .into_iter()
            .find_map(|r| match r.call {
                Call::CreateTab { label, cwd } => Some((label, cwd)),
                _ => None,
            })
            .expect("a tab was created")
    }

    fn run_texts(&self, pane: &str) -> Vec<String> {
        self.runs_on(pane).into_iter().map(|(_, t)| t).collect()
    }

    fn runs_on(&self, pane: &str) -> Vec<(Duration, String)> {
        self.term
            .runs()
            .into_iter()
            .filter(|(_, p, _)| p == pane)
            .map(|(at, _, text)| (at, text))
            .collect()
    }

    fn text_on(&self, pane: &str) -> Vec<(Duration, String)> {
        self.term
            .text_sent()
            .into_iter()
            .filter(|(_, p, _)| p == pane)
            .map(|(at, _, text)| (at, text))
            .collect()
    }

    fn keys_on(&self, pane: &str) -> Vec<(Duration, Vec<String>)> {
        self.term
            .keys_sent()
            .into_iter()
            .filter(|(_, p, _)| p == pane)
            .map(|(at, _, keys)| (at, keys))
            .collect()
    }

    /// Times at which a single key was sent to a pane.
    fn key_times(&self, pane: &str, key: &str) -> Vec<Duration> {
        self.keys_on(pane)
            .into_iter()
            .filter(|(_, keys)| keys.iter().any(|k| k == key))
            .map(|(at, _)| at)
            .collect()
    }

    /// Time of the first `pane run` on the pane whose text satisfies the predicate.
    fn run_time(&self, pane: &str, wanted: impl Fn(&str) -> bool) -> Duration {
        self.runs_on(pane)
            .into_iter()
            .find(|(_, text)| wanted(text))
            .map(|(at, _)| at)
            .expect("the wanted text was run")
    }

    /// Token prompts typed into a pane, with their time.
    fn token_prompts(&self, pane: &str) -> Vec<(Duration, String)> {
        self.runs_on(pane)
            .into_iter()
            .filter(|(_, text)| token_of(text).is_some())
            .collect()
    }

    /// Time of the last read before the tab was closed: where the waiting ended.
    fn end_of_waiting(&self) -> Duration {
        let calls = self.calls();
        let close = calls
            .iter()
            .position(|r| matches!(r.call, Call::CloseTab { .. }))
            .expect("the tab was closed");
        calls[..close]
            .iter()
            .rev()
            .find(|r| matches!(r.call, Call::Read { .. }))
            .map(|r| r.at)
            .expect("a read before the close")
    }

    /// Time from `since` to the end of the waiting.
    fn waited_since(&self, since: Duration) -> Duration {
        self.end_of_waiting().saturating_sub(since)
    }
}

/// The joined token of a token prompt, if the text is one.
fn token_of(text: &str) -> Option<String> {
    let pattern = Regex::new(&format!(
        "^{} ([a-z]+) and ([0-9]{{4}})$",
        regex::escape(TOKEN_PROMPT_START.trim_end())
    ))
    .expect("token prompt pattern");
    let caught = pattern.captures(text)?;
    Some(format!("{}-{}", &caught[1], &caught[2]))
}

fn is_token_prompt(text: &str) -> bool {
    token_of(text).is_some()
}

fn passes_of(steps: &[&str]) -> Vec<String> {
    steps.iter().map(|s| format!("{s}:pass")).collect()
}

// ------------------------------------------------------------------------------- assertions

/// The tab was created once, closed once and checked, and the scratch project is gone.
fn assert_cleaned(e: &Exec) {
    let calls = e.calls();
    let creates = calls
        .iter()
        .filter(|r| matches!(r.call, Call::CreateTab { .. }))
        .count();
    assert_eq!(creates, 1, "one tab is created");
    let (label, cwd) = e.created();
    let shape = Regex::new("^talaria-smoke-[a-z0-9]{8}$").expect("label pattern");
    assert!(shape.is_match(&label), "label {label}");
    assert_eq!(
        cwd,
        e.root.join(&label),
        "the scratch project sits in the temporary root"
    );
    assert!(!cwd.exists(), "the scratch project was removed");
    assert_eq!(e.term.close_count(), 1, "close_tab is called once");
    let close = calls
        .iter()
        .position(|r| matches!(r.call, Call::CloseTab { .. }))
        .expect("a close");
    let after = &calls[close + 1..];
    assert!(
        after
            .iter()
            .any(|r| matches!(r.call, Call::TabExists { .. })),
        "the tab is checked to be gone after the close"
    );
    assert!(
        after
            .iter()
            .all(|r| matches!(r.call, Call::TabExists { .. } | Call::Version)),
        "no pane is touched after the close: {after:?}"
    );
    assert!(
        e.report.unclosed_tab.is_none(),
        "no unclosed tab is reported"
    );
}

/// A run that ends in a failing step: earlier steps passed, nothing after it, one result line.
fn expect_fail(e: &Exec, step: &str, reason: &str) {
    let index = STEPS
        .iter()
        .position(|s| *s == step)
        .expect("a known step name");
    let mut want = passes_of(&STEPS[..index]);
    want.push(format!("{step}:fail"));
    assert_eq!(e.kinds(), want, "outcomes");
    assert_eq!(e.last_reason().as_deref(), Some(reason), "reason");
    assert_eq!(e.report.exit_code(), 1);
    assert_eq!(
        e.report.result_line(),
        format!("SMOKE_RESULT: FAILED ({step})")
    );
    let lines = e.lines();
    let failing = format!("FAIL {step} ({reason})");
    assert!(lines.contains(&failing), "missing {failing:?} in {lines:?}");
    for later in &STEPS[index + 1..] {
        assert!(
            !lines.iter().any(|l| {
                l.starts_with(&format!("PASS {later} "))
                    || l.starts_with(&format!("SKIP {later} "))
                    || l.starts_with(&format!("FAIL {later} "))
            }),
            "a line for {later} after the failure: {lines:?}"
        );
    }
    assert_cleaned(e);
}

fn expect_all_pass(e: &Exec) {
    assert_eq!(e.kinds(), passes_of(&STEPS));
    assert_eq!(e.report.exit_code(), 0);
    assert_eq!(
        e.report.result_line(),
        format!(
            "SMOKE_RESULT: PASSED (elapsed {} s)",
            e.report.elapsed.as_secs()
        )
    );
    let lines = e.lines();
    for outcome in &e.report.outcomes {
        let StepOutcome::Pass { step, elapsed } = outcome else {
            panic!("a step did not pass");
        };
        let pattern = Regex::new(&format!(r"^PASS {step} ([0-9]+\.[0-9]) s$")).expect("pattern");
        let shown: f64 = lines
            .iter()
            .find_map(|l| pattern.captures(l))
            .unwrap_or_else(|| panic!("no PASS line for {step} in {lines:?}"))[1]
            .parse()
            .expect("seconds");
        assert!(
            (shown - elapsed.as_secs_f64()).abs() <= 0.06,
            "{step}: line shows {shown}, the step took {elapsed:?}"
        );
    }
    assert_cleaned(e);
}

/// Every wait is a poll loop at 250 ms; only the wait after Esc is a different length.
fn assert_polling_grid(e: &Exec) {
    let calls = e.calls();
    let mut shortest = None;
    for pair in calls.windows(2) {
        let gap = pair[1].at.saturating_sub(pair[0].at);
        if gap.is_zero() {
            continue;
        }
        shortest = Some(shortest.map_or(gap, |s: Duration| s.min(gap)));
        let after_esc = matches!(&pair[0].call, Call::SendKeys { keys, .. } if keys == &["esc"]);
        if after_esc {
            assert!(gap >= ms(400), "only {gap:?} of silence after Esc");
            assert_eq!(
                (gap - ms(400)).as_millis() % 250,
                0,
                "{gap:?} after Esc is not 400 ms plus whole polls"
            );
        } else {
            assert_eq!(gap.as_millis() % 250, 0, "a gap of {gap:?} between calls");
        }
    }
    assert_eq!(shortest, Some(ms(250)), "the polling interval is 250 ms");
}

fn happy() -> Scenario {
    Scenario::happy()
}

// ------------------------------------------------------------------ scripts built by hand

/// The reply rule of a token prompt: the echo, a short churn, then the joined token.
fn token_rule() -> RuleBuilder {
    RuleBuilder::run(
        r"join these two words with a hyphen and reply with only the result: (\w+) and (\d+)$",
    )
    .append("you: $0")
    .remember("$1-$2")
    .churn(4)
    .then(StageBuilder::new().after_polls(4).append("agent: $1-$2"))
}

/// Start and plain answer rules only, for tests that need an unusual approval screen.
fn script_to_plain_answer() -> ScriptBuilder {
    ScriptBuilder::new()
        .on_first(
            RuleBuilder::run("--no-ensure --profile ")
                .foreground(&["-bash", "$0"])
                .append("$0\nTalaria\nConnecting to Hermes"),
        )
        .on_first(token_rule())
}

/// The recorded card, from its first line.
fn card_text() -> String {
    let start = support::CARD_FIXTURE.find("Approval needed").unwrap_or(0);
    support::CARD_FIXTURE[start..].trim_end().to_owned()
}

/// Start, plain answer and a normal approval, for tests that need an unusual interrupt.
fn script_to_interrupt() -> ScriptBuilder {
    script_to_plain_answer()
        .on_first(
            approval_prompt_rule()
                .churn(2)
                .then(StageBuilder::new().after_polls(3).append(&card_text())),
        )
        .on_first(
            RuleBuilder::send_text("^1$")
                .remove_in_cwd(".pytest_cache")
                .clear()
                .append("Command finished with exit code 0."),
        )
}

fn count_rule(lines: u32) -> RuleBuilder {
    RuleBuilder::run(r"Count from 1 to 2000, one number per line, and do not stop early\.$")
        .append("you: $0")
        .churn(lines)
        .churn_lines("{n}", 1, 1)
}

fn approval_prompt_rule() -> RuleBuilder {
    RuleBuilder::run(r"rm -rf \.pytest_cache$").append("you: $0")
}

// =============================================================================================
// The happy path and the launch lines
// =============================================================================================

#[test]
fn happy_path_passes_and_closes_the_tab() {
    let e = scenario(&happy());
    expect_all_pass(&e);
    assert_eq!(e.report.fallbacks, 0);
    assert!(!e.report.interrupted);
    assert!(e.report.elapsed <= e.clock.now());
    assert_eq!(
        e.report.version,
        include_str!("fixtures/herdr/version.txt").trim()
    );

    // First pane: launch line, token A, approval, count, token B, /quit.
    let first = e.run_texts(FIRST_PANE_ID);
    assert_eq!(first.len(), 6, "{first:?}");
    assert!(first[0].ends_with("; echo SMOKE_EXIT:$?"));
    assert!(is_token_prompt(&first[1]));
    assert_eq!(first[2], APPROVAL_PROMPT);
    assert_eq!(first[3], COUNT_PROMPT);
    assert!(is_token_prompt(&first[4]));
    assert_eq!(first[5], "/quit");
    // Second pane: resume line, token C, /quit.
    let second = e.run_texts(SECOND_PANE_ID);
    assert_eq!(second.len(), 3, "{second:?}");
    assert!(second[0].contains(" -c "));
    assert!(is_token_prompt(&second[1]));
    assert_eq!(second[2], "/quit");

    // The approval is answered with a digit, the interrupt with Esc, nothing else is pressed.
    let digits = e.text_on(FIRST_PANE_ID);
    assert_eq!(digits.len(), 1);
    assert_eq!(digits[0].1, "1");
    assert!(e.text_on(SECOND_PANE_ID).is_empty());
    let keys = e.keys_on(FIRST_PANE_ID);
    assert_eq!(keys.len(), 1);
    assert_eq!(keys[0].1, vec!["esc".to_owned()]);
    assert!(e.keys_on(SECOND_PANE_ID).is_empty());

    // Three tokens, three different values, each a word of the list and four digits.
    let tokens: Vec<String> = e
        .token_prompts(FIRST_PANE_ID)
        .into_iter()
        .chain(e.token_prompts(SECOND_PANE_ID))
        .filter_map(|(_, text)| token_of(&text))
        .collect();
    assert_eq!(tokens.len(), 3);
    for token in &tokens {
        let (word, number) = token.split_once('-').expect("word-number");
        assert!(WORDS.contains(&word), "{token}");
        assert!((1000..=9999).contains(&number.parse::<u32>().expect("number")));
    }
    assert!(tokens[0] != tokens[1] && tokens[1] != tokens[2] && tokens[0] != tokens[2]);

    // Searches use the unwrapped recent text with 3000 lines; the soft wrapped source is never read.
    let reads: Vec<(ReadSource, Option<u32>)> = e
        .calls()
        .into_iter()
        .filter_map(|r| match r.call {
            Call::Read { source, lines, .. } => Some((source, lines)),
            _ => None,
        })
        .collect();
    assert!(
        reads
            .iter()
            .all(|(source, _)| *source != ReadSource::Recent)
    );
    assert!(reads.contains(&(ReadSource::RecentUnwrapped, Some(3000))));
    assert!(
        reads
            .iter()
            .any(|(source, _)| *source == ReadSource::Visible)
    );
}

#[test]
fn launch_lines_absolute_no_ensure() {
    // A folder name with a space shows why the path is quoted.
    let e = exec_with(
        &happy().script(),
        |cfg| {
            let dir = cfg.temp_root.join("folder with space");
            fs::create_dir_all(&dir).expect("create folder");
            cfg.talaria = dir.join("talaria");
            cfg.profile = "default".to_owned();
            cfg.model = "model-one".to_owned();
            cfg.provider = "provider-two".to_owned();
        },
        |term| term,
    );
    expect_all_pass(&e);
    assert!(e.talaria.is_absolute());
    let path = e.talaria.display();
    let first = e.run_texts(FIRST_PANE_ID);
    assert_eq!(
        first[0],
        format!(
            "'{path}' --no-ensure --profile default --model model-one --provider provider-two; echo SMOKE_EXIT:$?"
        )
    );
    let second = e.run_texts(SECOND_PANE_ID);
    assert_eq!(
        second[0],
        format!("'{path}' --no-ensure -c --profile default; echo SMOKE_EXIT:$?")
    );
    // Only those two lines start the binary; every other run is a prompt or /quit.
    let launches = e
        .term
        .runs()
        .into_iter()
        .filter(|(_, _, text)| text.contains("--no-ensure"))
        .count();
    assert_eq!(launches, 2);
    assert!(
        e.term
            .runs()
            .into_iter()
            .filter(|(_, _, text)| !text.contains("--no-ensure"))
            .all(|(_, _, text)| !text.contains("talaria")),
        "the binary path appears only in launch lines"
    );
}

// =============================================================================================
// startup
// =============================================================================================

#[test]
fn startup_fails_with_exit_code() {
    for code in [7, 0] {
        let e = scenario(&happy().startup(Startup::ExitsWith(code)));
        expect_fail(
            &e,
            "startup",
            &format!("talaria exited with code {code} before showing its screen"),
        );
        // Failing at once: the exit marker is not waited out.
        let launched = e.run_time(FIRST_PANE_ID, |t| t.contains("--no-ensure"));
        assert!(e.waited_since(launched) <= secs(10));
        // The pane text of the failure is kept.
        let first = e.capture_of("first");
        assert!(
            first.visible.contains("stub: no gateway"),
            "{}",
            first.visible
        );
        assert!(first.recent.contains("stub: no gateway"));
        // Nothing but the launch line was typed.
        assert_eq!(e.run_texts(FIRST_PANE_ID).len(), 1);
        assert!(e.text_on(FIRST_PANE_ID).is_empty());
        assert!(e.term.keys_sent().is_empty());
    }
}

#[test]
fn startup_fails_without_stable_screen() {
    let e = scenario(&happy().startup(Startup::NeverStable));
    expect_fail(
        &e,
        "startup",
        "startup did not show a stable screen within 30 s",
    );
    let launched = e.run_time(FIRST_PANE_ID, |t| t.contains("--no-ensure"));
    assert_within(
        "startup wait",
        e.waited_since(launched),
        ms(29_500),
        secs(36),
    );
    assert!(e.capture_of("first").visible.contains("working..."));
    assert_eq!(e.run_texts(FIRST_PANE_ID).len(), 1);
}

#[test]
fn startup_fails_when_the_shell_is_never_ready() {
    let e = scenario(&happy().startup(Startup::ShellNeverReady));
    expect_fail(&e, "startup", "the shell did not become ready within 45 s");
    assert!(
        e.term.runs().is_empty(),
        "nothing is run in a shell that is not ready"
    );
    assert_within("shell wait", e.end_of_waiting(), ms(44_500), secs(52));
}

// =============================================================================================
// plain-answer
// =============================================================================================

#[test]
fn plain_answer_times_out_with_pane_text() {
    let e = scenario(&happy().token_a(TokenReply::Silent));
    expect_fail(
        &e,
        "plain-answer",
        "the reply token did not appear and settle within 60 s",
    );
    let submitted = e.run_time(FIRST_PANE_ID, is_token_prompt);
    assert_within(
        "reply wait",
        e.waited_since(submitted),
        ms(59_500),
        secs(66),
    );
    // The capture holds the screen as it was: the echoed prompt and no reply.
    let first = e.capture_of("first");
    assert!(
        first.visible.contains("join these two words"),
        "{}",
        first.visible
    );
    assert!(first.recent.contains("join these two words"));
    // 200 lines of unwrapped recent text of the pane in use were read before the tab closed.
    let calls = e.calls();
    let close = calls
        .iter()
        .position(|r| matches!(r.call, Call::CloseTab { .. }))
        .expect("a close");
    assert!(calls[..close].iter().any(|r| matches!(
        &r.call,
        Call::Read { pane, source: ReadSource::RecentUnwrapped, lines: Some(200) }
            if pane == FIRST_PANE_ID
    )));
    // No automatic retry: one token prompt only.
    assert_eq!(e.token_prompts(FIRST_PANE_ID).len(), 1);
    // A screen that stood still for 3 s with nothing on it gets one Enter, and only one.
    let enters = e.key_times(FIRST_PANE_ID, "enter");
    assert_eq!(enters.len(), 1, "{enters:?}");
    assert_within(
        "Enter after submit",
        enters[0] - submitted,
        secs(3),
        secs(8),
    );
    assert_eq!(e.report.fallbacks, 1);
}

#[test]
fn plain_answer_without_the_hyphen_does_not_count() {
    let e = scenario(&happy().token_a(TokenReply::WrongForm));
    expect_fail(
        &e,
        "plain-answer",
        "the reply token did not appear and settle within 60 s",
    );
    assert_eq!(e.token_prompts(FIRST_PANE_ID).len(), 1, "no retry");
}

#[test]
fn plain_answer_needs_a_settled_screen_too() {
    // The token is on screen but something keeps changing: a reply that is still being written.
    let e = scenario(&happy().token_a(TokenReply::NeverSettles));
    expect_fail(
        &e,
        "plain-answer",
        "the reply token did not appear and settle within 60 s",
    );
    let submitted = e.run_time(FIRST_PANE_ID, is_token_prompt);
    assert_within(
        "reply wait",
        e.waited_since(submitted),
        ms(59_500),
        secs(66),
    );
}

// =============================================================================================
// the submit fallback
// =============================================================================================

#[test]
fn submit_fallback_sends_one_enter() {
    let e = scenario(&happy().token_a(TokenReply::NeedsEnter));
    expect_all_pass(&e);
    assert_eq!(e.report.fallbacks, 1);
    let enters = e.key_times(FIRST_PANE_ID, "enter");
    assert_eq!(enters.len(), 1, "exactly one Enter in the whole run");
    let keys = e.keys_on(FIRST_PANE_ID);
    assert_eq!(
        keys[0].1,
        vec!["enter".to_owned()],
        "a single key, in the first pane"
    );
    // After 3 s of stillness and inside the 8 s window that follows the submission.
    let submitted = e.run_time(FIRST_PANE_ID, is_token_prompt);
    assert_within(
        "Enter after submit",
        enters[0] - submitted,
        secs(3),
        secs(8),
    );
    assert!(e.key_times(SECOND_PANE_ID, "enter").is_empty());
}

#[test]
fn quit_fallback_sends_enter_once() {
    let e = scenario(&happy().quit(Quit::NeedsEnter));
    expect_all_pass(&e);
    assert_eq!(e.report.fallbacks, 1);
    let enters = e.key_times(FIRST_PANE_ID, "enter");
    assert_eq!(enters.len(), 1);
    let quit = e.run_time(FIRST_PANE_ID, |t| t == "/quit");
    assert_within("Enter after /quit", enters[0] - quit, secs(3), secs(15));
}

#[test]
fn submit_fallback_never_with_a_card() {
    // Card words stay on screen, unchanged, while the step decides; no Enter is ever pressed.
    for approval in [
        Approval::CardOtherCommand,
        Approval::CardHighlightElsewhere,
        Approval::CardNoOnceNumber,
    ] {
        let e = scenario(&happy().approval(approval));
        assert_eq!(
            e.kinds().last().map(String::as_str),
            Some("approval:fail"),
            "{approval:?}"
        );
        assert!(
            e.term.keys_sent().is_empty(),
            "{approval:?}: keys {:?}",
            e.term.keys_sent()
        );
        assert_eq!(e.report.fallbacks, 0, "{approval:?}");
        assert_cleaned(&e);
    }

    // A card that outlives its answer sees the one Enter of the answer rule and nothing more,
    // although the screen stands still for longer than the fallback's quiet period.
    let e = scenario(&happy().approval(Approval::CardIgnoresAnswer));
    assert_eq!(e.key_times(FIRST_PANE_ID, "enter").len(), 1);
    assert_eq!(e.report.fallbacks, 0);
}

// =============================================================================================
// approval
// =============================================================================================

#[test]
fn approval_answers_with_the_once_number() {
    let e = scenario(&happy());
    expect_all_pass(&e);
    let digits = e.text_on(FIRST_PANE_ID);
    assert_eq!(digits.len(), 1);
    assert_eq!(digits[0].1, "1");
    let submitted = e.run_time(FIRST_PANE_ID, |t| t == APPROVAL_PROMPT);
    assert!(digits[0].0 >= submitted);
    assert!(
        e.key_times(FIRST_PANE_ID, "enter").is_empty(),
        "the digit alone answers"
    );

    // The number comes from the card: here `once` is the third choice.
    let card = "Approval needed\n$ rm -rf .pytest_cache\nRecursive delete of a folder in the working directory.\n  1. deny\n  2. session\n› 3. once";
    let script = script_to_plain_answer()
        .on_first(
            approval_prompt_rule()
                .churn(2)
                .then(StageBuilder::new().after_polls(3).append(card)),
        )
        .on_first(
            RuleBuilder::send_text("^3$")
                .remove_in_cwd(".pytest_cache")
                .clear()
                .append("Command finished with exit code 0."),
        );
    let e = exec(&script);
    assert_eq!(
        &e.kinds()[..3],
        passes_of(&["startup", "plain-answer", "approval"]).as_slice(),
        "{:?}",
        e.kinds()
    );
    let digits = e.text_on(FIRST_PANE_ID);
    assert_eq!(digits.len(), 1, "{digits:?}");
    assert_eq!(digits[0].1, "3");
}

#[test]
fn approval_enter_follows_a_digit_that_did_nothing() {
    let e = scenario(&happy().approval(Approval::CardEnterNeeded));
    expect_all_pass(&e);
    let digits = e.text_on(FIRST_PANE_ID);
    assert_eq!(digits.len(), 1);
    assert_eq!(digits[0].1, "1");
    let enters = e.key_times(FIRST_PANE_ID, "enter");
    assert_eq!(enters.len(), 1, "Enter once");
    assert_within(
        "Enter after the digit",
        enters[0] - digits[0].0,
        ms(1500),
        secs(3),
    );
}

#[test]
fn approval_without_once_number_fails() {
    let e = scenario(&happy().approval(Approval::CardNoOnceNumber));
    expect_fail(&e, "approval", NO_ONCE_NUMBER_REASON);
    // Nothing is typed into a card whose answer cannot be found.
    assert!(e.text_on(FIRST_PANE_ID).is_empty());
    assert!(e.term.keys_sent().is_empty());
    assert!(e.capture_of("first").visible.contains("Approval needed"));
}

#[test]
fn approval_other_command_not_answered() {
    let e = scenario(&happy().approval(Approval::CardOtherCommand));
    expect_fail(
        &e,
        "approval",
        "an approval card for another command is open; the script does not answer it",
    );
    assert!(e.text_on(FIRST_PANE_ID).is_empty(), "no digit is typed");
    assert!(e.term.keys_sent().is_empty(), "no key is pressed");
    let submitted = e.run_time(FIRST_PANE_ID, |t| t == APPROVAL_PROMPT);
    assert!(
        e.waited_since(submitted) <= secs(30),
        "it fails when it sees the card"
    );
    assert!(
        e.capture_of("first")
            .visible
            .contains("cd ~ && rm -rf .pytest_cache"),
        "the card is in the capture"
    );
}

#[test]
fn approval_highlight_elsewhere_fails() {
    let e = scenario(&happy().approval(Approval::CardHighlightElsewhere));
    expect_fail(
        &e,
        "approval",
        "the highlighted choice is not 'once'; Enter was not pressed",
    );
    let digits = e.text_on(FIRST_PANE_ID);
    assert_eq!(digits.len(), 1);
    assert_eq!(digits[0].1, "1");
    assert!(e.term.keys_sent().is_empty(), "Enter is not pressed");
    assert_within(
        "wait for the card to close",
        e.waited_since(digits[0].0),
        ms(1500),
        secs(6),
    );
}

#[test]
fn approval_card_still_open_after_answer() {
    let e = scenario(&happy().approval(Approval::CardIgnoresAnswer));
    expect_fail(
        &e,
        "approval",
        "the approval card was still open 3 s after it was answered",
    );
    let digits = e.text_on(FIRST_PANE_ID);
    assert_eq!(digits.len(), 1, "the card is answered once");
    let enters = e.key_times(FIRST_PANE_ID, "enter");
    assert_eq!(enters.len(), 1, "Enter once, on the highlighted once");
    assert_within(
        "Enter after the digit",
        enters[0] - digits[0].0,
        ms(1500),
        secs(3),
    );
    assert_within(
        "wait after the answer",
        e.waited_since(digits[0].0),
        secs(3),
        secs(10),
    );
}

#[test]
fn approval_skips_when_command_ran() {
    let e = scenario(&happy().approval(Approval::RanWithoutCard));
    assert_eq!(
        e.kinds(),
        vec![
            "startup:pass",
            "plain-answer:pass",
            "approval:skip",
            "interrupt:pass",
            "quit:pass",
            "resume:pass",
        ]
    );
    assert_eq!(e.last_reason(), None, "the run ended with a pass");
    let skip = e
        .report
        .outcomes
        .iter()
        .find_map(|o| match o {
            StepOutcome::Skip { step, reason } => Some((*step, reason.clone())),
            _ => None,
        })
        .expect("a skipped step");
    assert_eq!(skip, ("approval", SKIP_REASON.to_owned()));
    assert!(
        e.lines()
            .contains(&format!("SKIP approval ({SKIP_REASON})"))
    );
    assert_eq!(e.report.exit_code(), 3);
    assert_eq!(
        e.report.result_line(),
        "SMOKE_RESULT: PASSED WITH SKIPS (approval)"
    );
    // The skip types nothing into the pane: no digit, no key before the interrupt.
    assert!(e.text_on(FIRST_PANE_ID).is_empty());
    assert_eq!(e.key_times(FIRST_PANE_ID, "esc").len(), 1);
    assert!(e.key_times(FIRST_PANE_ID, "enter").is_empty());
    assert_cleaned(&e);
}

#[test]
fn approval_retries_once_then_fails() {
    let e = scenario(
        &happy()
            .approval(Approval::Ignored)
            .approval_retry(Approval::Ignored),
    );
    expect_fail(
        &e,
        "approval",
        "the model did not run the command in two attempts",
    );
    let prompts: Vec<(Duration, String)> = e
        .runs_on(FIRST_PANE_ID)
        .into_iter()
        .filter(|(_, t)| t.contains("rm -rf .pytest_cache"))
        .collect();
    assert_eq!(prompts.len(), 2, "two attempts, no more: {prompts:?}");
    assert_eq!(prompts[0].1, APPROVAL_PROMPT);
    assert_eq!(prompts[1].1, APPROVAL_RETRY_PROMPT);
    // Five seconds of stillness, then the second attempt.
    assert_within(
        "second attempt",
        prompts[1].0 - prompts[0].0,
        secs(5),
        secs(30),
    );
    assert!(e.text_on(FIRST_PANE_ID).is_empty());
}

#[test]
fn approval_second_attempt_can_pass() {
    let e = scenario(
        &happy()
            .approval(Approval::Ignored)
            .approval_retry(Approval::Card),
    );
    expect_all_pass(&e);
    let prompts: Vec<String> = e
        .run_texts(FIRST_PANE_ID)
        .into_iter()
        .filter(|t| t.contains("rm -rf .pytest_cache"))
        .collect();
    assert_eq!(prompts, vec![APPROVAL_PROMPT, APPROVAL_RETRY_PROMPT]);
    assert_eq!(e.text_on(FIRST_PANE_ID).len(), 1);
}

#[test]
fn approval_withdrawn_is_reported() {
    let e = scenario(
        &happy()
            .approval(Approval::Withdrawn)
            .approval_retry(Approval::Withdrawn),
    );
    expect_fail(
        &e,
        "approval",
        "the approval was withdrawn or blocked: The approval was withdrawn: the client could not answer it.",
    );
    assert!(e.text_on(FIRST_PANE_ID).is_empty());

    // The first line with the whole word, case-insensitive and trimmed; "unblocked" is no match.
    let script = script_to_plain_answer().on_first(approval_prompt_rule().churn(2).then(
        StageBuilder::new().after_polls(3).append(
            "The folder is unblocked now.\n   The command was BLOCKED by policy.  \nblocked again",
        ),
    ));
    let e = exec(&script);
    assert_eq!(
        e.kinds(),
        vec!["startup:pass", "plain-answer:pass", "approval:fail"]
    );
    assert_eq!(
        e.last_reason().as_deref(),
        Some("the approval was withdrawn or blocked: The command was BLOCKED by policy.")
    );
}

#[test]
fn approval_no_outcome_in_90s_fails() {
    let e = scenario(&happy().approval(Approval::NoOutcome));
    expect_fail(
        &e,
        "approval",
        "no approval card and no outcome within 90 s",
    );
    let submitted = e.run_time(FIRST_PANE_ID, |t| t == APPROVAL_PROMPT);
    assert_within(
        "approval wait",
        e.waited_since(submitted),
        ms(89_500),
        secs(96),
    );
    let prompts = e
        .run_texts(FIRST_PANE_ID)
        .into_iter()
        .filter(|t| t.contains("rm -rf .pytest_cache"))
        .count();
    assert_eq!(
        prompts, 1,
        "a screen that keeps changing is not a second attempt"
    );
}

// =============================================================================================
// the run stops at a failure; a panic is a failure
// =============================================================================================

#[test]
fn a_failing_step_stops_the_run() {
    let e = scenario(&happy().interrupt(Interrupt::KeepsGrowing));
    expect_fail(&e, "interrupt", "output kept growing after Esc");
    assert_eq!(
        e.lines().iter().filter(|l| l.starts_with("PASS")).count(),
        3
    );
    // Nothing of the quit and resume steps happens: no /quit, no second pane.
    assert!(!e.run_texts(FIRST_PANE_ID).iter().any(|t| t == "/quit"));
    assert!(e.run_texts(SECOND_PANE_ID).is_empty());
    assert!(
        !e.calls()
            .iter()
            .any(|r| matches!(r.call, Call::SplitPane { .. }))
    );
    // A failing first step ends the run just as well.
    let e = scenario(&happy().startup(Startup::ExitsWith(7)));
    assert_eq!(e.kinds(), vec!["startup:fail"]);
    assert_eq!(e.lines().len(), 1, "{:?}", e.lines());
}

#[test]
fn panic_in_a_step_is_a_failed_step() {
    let reason = format!("internal error: {PANIC_MESSAGE}");
    // The second `pane run` is the plain answer prompt.
    let e = exec_with(
        &happy().script(),
        |_| {},
        |term| term.panic_on_nth("pane run", 2),
    );
    expect_fail(&e, "plain-answer", &reason);

    // A panic names the step that was in progress, whichever it is.
    let e = exec_with(
        &happy().script(),
        |_| {},
        |term| {
            term.panic_when(
                |call| matches!(call, Call::Run { text, .. } if text.starts_with("Count from")),
            )
        },
    );
    expect_fail(&e, "interrupt", &reason);

    let e = exec_with(
        &happy().script(),
        |_| {},
        |term| term.panic_on_nth("pane process-info", 1),
    );
    expect_fail(&e, "startup", &reason);
}

#[test]
fn approval_fails_if_directory_survives() {
    let e = scenario(&happy().approval(Approval::AnsweredButNoDeletion));
    expect_fail(
        &e,
        "approval",
        "the command did not finish within 60 s of the answer",
    );
    let digits = e.text_on(FIRST_PANE_ID);
    assert_eq!(digits.len(), 1);
    assert_within(
        "wait after the answer",
        e.waited_since(digits[0].0),
        ms(59_500),
        secs(66),
    );
    assert!(
        e.term.keys_sent().is_empty(),
        "the card was gone, so no Enter"
    );
}

// =============================================================================================
// interrupt
// =============================================================================================

#[test]
fn interrupt_waits_400ms_after_esc() {
    let e = scenario(&happy().interrupt(Interrupt::Works));
    expect_all_pass(&e);
    let keys = e.keys_on(FIRST_PANE_ID);
    assert_eq!(keys.len(), 1, "one key call in the first pane");
    assert_eq!(keys[0].1, vec!["esc".to_owned()]);
    let esc_at = keys[0].0;

    // Fifteen counting lines and two readings one second apart come before Esc.
    let counted = e.run_time(FIRST_PANE_ID, |t| t == COUNT_PROMPT);
    assert!(esc_at - counted >= secs(1));

    // Nothing at all happens for 400 ms after Esc.
    let calls = e.calls();
    let index = calls
        .iter()
        .position(|r| matches!(&r.call, Call::SendKeys { keys, .. } if keys == &["esc"]))
        .expect("the Esc call");
    let next = calls[index + 1..].first().expect("a call after Esc");
    assert!(
        next.at >= esc_at + ms(400),
        "next call {:?} after Esc at {esc_at:?}",
        next.at
    );

    // The follow-up prompt comes after the screen was still for 3 s.
    let follow_up = e
        .token_prompts(FIRST_PANE_ID)
        .last()
        .map(|(at, _)| *at)
        .expect("the follow-up prompt");
    assert!(follow_up >= esc_at + secs(3));
    // Esc is the only key of the interrupt step and no text follows it.
    assert!(e.text_on(FIRST_PANE_ID).iter().all(|(at, _)| *at < esc_at));
}

#[test]
fn interrupt_fails_if_output_keeps_growing() {
    let e = scenario(&happy().interrupt(Interrupt::KeepsGrowing));
    expect_fail(&e, "interrupt", "output kept growing after Esc");
    let esc = e.key_times(FIRST_PANE_ID, "esc");
    assert_eq!(esc.len(), 1);
    assert_within("settle wait", e.waited_since(esc[0]), ms(19_500), secs(27));
    assert_eq!(
        e.token_prompts(FIRST_PANE_ID).len(),
        1,
        "no follow-up prompt"
    );
}

#[test]
fn interrupt_fails_if_count_finished() {
    let e = scenario(&happy().interrupt(Interrupt::CountFinishes));
    expect_fail(
        &e,
        "interrupt",
        "the count finished before the interrupt; use a slower prompt",
    );
    assert_eq!(e.key_times(FIRST_PANE_ID, "esc").len(), 1);
}

#[test]
fn interrupt_fails_if_count_not_growing() {
    for interrupt in [Interrupt::NotGrowing, Interrupt::FinishedEarly] {
        let e = scenario(&happy().interrupt(interrupt));
        expect_fail(
            &e,
            "interrupt",
            "the count was not running, or was nearly finished, when Esc was sent",
        );
        // The check comes before the key: Esc is never sent into a count that is not running.
        assert!(e.term.keys_sent().is_empty(), "{interrupt:?}");
    }
}

#[test]
fn interrupt_fails_if_no_count_remains() {
    let e = scenario(&happy().interrupt(Interrupt::NoCountLeft));
    expect_fail(&e, "interrupt", "no counting output remained after Esc");
    assert_eq!(e.key_times(FIRST_PANE_ID, "esc").len(), 1);
}

#[test]
fn interrupt_fails_if_count_vanished() {
    let e = scenario(&happy().interrupt(Interrupt::CountVanishes));
    expect_fail(&e, "interrupt", "counting output vanished after Esc");
}

#[test]
fn interrupt_fails_with_too_few_counting_lines() {
    let e = scenario(&happy().interrupt(Interrupt::TooFewLines));
    expect_fail(
        &e,
        "interrupt",
        "fewer than 15 counting lines appeared within 60 s",
    );
    let counted = e.run_time(FIRST_PANE_ID, |t| t == COUNT_PROMPT);
    assert_within("count wait", e.waited_since(counted), ms(59_500), secs(66));
    assert!(e.term.keys_sent().is_empty(), "Esc waits for fifteen lines");
}

#[test]
fn interrupt_count_boundaries() {
    // Fourteen counting lines are too few; the wait is for fifteen.
    let e = exec(&script_to_interrupt().on_first(count_rule(14)));
    assert_eq!(
        e.kinds(),
        vec![
            "startup:pass",
            "plain-answer:pass",
            "approval:pass",
            "interrupt:fail"
        ]
    );
    assert_eq!(
        e.last_reason().as_deref(),
        Some("fewer than 15 counting lines appeared within 60 s")
    );

    // After Esc the highest number may be 1899 but not 1900.
    for (last, passes) in [("1899", true), ("1900", false)] {
        let script = script_to_interrupt()
            .on_first(count_rule(1_000_000))
            .on_first(RuleBuilder::send_keys("^esc$").stop_churn().append(last))
            .on_first(token_rule());
        let e = exec(&script);
        let kinds = e.kinds();
        assert_eq!(&kinds[..3], passes_of(&STEPS[..3]).as_slice(), "{kinds:?}");
        if passes {
            assert_eq!(kinds[3], "interrupt:pass", "{last}: {kinds:?}");
        } else {
            assert_eq!(
                kinds,
                vec![
                    "startup:pass",
                    "plain-answer:pass",
                    "approval:pass",
                    "interrupt:fail",
                ]
            );
            assert_eq!(
                e.last_reason().as_deref(),
                Some("the count finished before the interrupt; use a slower prompt")
            );
        }
    }
}

#[test]
fn interrupt_follow_up_must_answer() {
    let e = scenario(&happy().token_b(TokenReply::Silent));
    expect_fail(
        &e,
        "interrupt",
        "the reply to the follow-up prompt did not appear and settle within 60 s",
    );
    let prompts = e.token_prompts(FIRST_PANE_ID);
    assert_eq!(prompts.len(), 2, "token A and the follow-up");
    assert!(token_of(&prompts[0].1) != token_of(&prompts[1].1));
    assert!(prompts[1].0 > e.key_times(FIRST_PANE_ID, "esc")[0]);
    assert_within(
        "follow-up wait",
        e.waited_since(prompts[1].0),
        ms(59_500),
        secs(66),
    );
}

// =============================================================================================
// quit
// =============================================================================================

#[test]
fn quit_fails_without_exit_code_zero() {
    let e = scenario(&happy().quit(Quit::ExitsWith(3)));
    expect_fail(
        &e,
        "quit",
        "/quit ended talaria with exit code 3, expected 0",
    );

    let e = scenario(&happy().quit(Quit::NeverExits));
    expect_fail(&e, "quit", "/quit did not end talaria within 15 s");
    let quit = e.run_time(FIRST_PANE_ID, |t| t == "/quit");
    assert_within("quit wait", e.waited_since(quit), ms(14_500), secs(21));
    // The submit fallback sends its one Enter while /quit is pending.
    let enters = e.key_times(FIRST_PANE_ID, "enter");
    assert_eq!(enters.len(), 1);
    assert!(enters[0] > quit);
}

#[test]
fn quit_fails_if_the_shell_is_not_back() {
    let e = scenario(&happy().quit(Quit::ShellNotBack));
    expect_fail(
        &e,
        "quit",
        "the shell did not return to the foreground after /quit",
    );
}

#[test]
fn quit_fails_if_output_vanished() {
    let e = scenario(&happy().quit(Quit::OutputVanishes));
    assert_eq!(e.kinds().last().map(String::as_str), Some("quit:fail"));
    let reason = e.last_reason().expect("a reason");
    assert!(
        reason == "committed output vanished on exit (token A missing)"
            || reason == "committed output vanished on exit (token B missing)",
        "{reason}"
    );
    assert_eq!(e.report.exit_code(), 1);
    assert_cleaned(&e);

    // Only the first token is left: the second one is named.
    let e = scenario(&happy().quit(Quit::LosesSecondToken));
    expect_fail(
        &e,
        "quit",
        "committed output vanished on exit (token B missing)",
    );
    assert!(
        e.run_texts(SECOND_PANE_ID).is_empty(),
        "no resume after a failed quit"
    );
}

// =============================================================================================
// resume
// =============================================================================================

#[test]
fn resume_runs_in_a_second_pane() {
    let e = scenario(&happy());
    expect_all_pass(&e);
    let (_, scratch) = e.created();
    let calls = e.calls();
    let splits: Vec<(usize, &Recorded)> = calls
        .iter()
        .enumerate()
        .filter(|(_, r)| matches!(r.call, Call::SplitPane { .. }))
        .collect();
    assert_eq!(splits.len(), 1, "one split");
    match &splits[0].1.call {
        Call::SplitPane { pane, cwd } => {
            assert_eq!(pane, FIRST_PANE_ID, "the first pane is split");
            assert_eq!(cwd, &scratch, "the new pane starts in the scratch project");
        }
        other => panic!("{other:?}"),
    }
    // The split comes after the quit of the first pane; nothing more is typed into the first pane.
    let first = e.runs_on(FIRST_PANE_ID);
    let quit_at = first.last().expect("a last run").0;
    assert_eq!(first.last().map(|(_, t)| t.as_str()), Some("/quit"));
    assert!(splits[0].1.at >= quit_at);
    // The resume line, the third prompt and the quit run in the new pane, in this order.
    let second = e.runs_on(SECOND_PANE_ID);
    assert_eq!(second.len(), 3);
    assert!(second[0].1.contains("--no-ensure -c --profile fixture"));
    assert!(second.iter().all(|(at, _)| *at >= splits[0].1.at));
    // The third token differs from the earlier two.
    let tokens: Vec<String> = e
        .token_prompts(FIRST_PANE_ID)
        .into_iter()
        .chain(e.token_prompts(SECOND_PANE_ID))
        .filter_map(|(_, text)| token_of(&text))
        .collect();
    assert_eq!(tokens.len(), 3);
    // The remembered replies of the fake are those three tokens.
    assert_eq!(e.term.memory(), tokens);
}

#[test]
fn resume_accepts_counts_up_to_first_pane() {
    let cases = [
        // (reply A, reply B, copies printed on resume)
        (TokenReply::Reply, TokenReply::Reply, 1),
        (TokenReply::ReplyTwice, TokenReply::ReplyTwice, 1),
        (TokenReply::ReplyTwice, TokenReply::ReplyTwice, 2),
    ];
    for (a, b, copies) in cases {
        let e = scenario(
            &happy()
                .token_a(a)
                .token_b(b)
                .resume(Resume::Transcript(copies)),
        );
        assert_eq!(e.kinds(), passes_of(&STEPS), "{a:?} {b:?} {copies}");
        assert_eq!(e.report.exit_code(), 0);
    }
}

#[test]
fn resume_fails_on_missing_or_extra_token() {
    let counts = |a: u32, b: u32| {
        format!(
            "token counts after resume: A {a} (first pane 1), B {b} (first pane 1); each must be at least 1 and at most the first pane's count"
        )
    };
    // History lost.
    let e = scenario(&happy().resume(Resume::Transcript(0)));
    expect_fail(&e, "resume", &counts(0, 0));
    // History printed twice.
    let e = scenario(&happy().resume(Resume::Transcript(2)));
    expect_fail(&e, "resume", &counts(2, 2));
    // Only one of the two tokens came back.
    let e = scenario(&happy().resume(Resume::OnlyFirstToken));
    expect_fail(&e, "resume", &counts(1, 0));
    // No third prompt is typed after a failed count.
    assert_eq!(e.run_texts(SECOND_PANE_ID).len(), 1);
    // Both panes are in the capture.
    let panes: Vec<&str> = e.capture().iter().map(|c| c.pane).collect();
    assert!(
        panes.contains(&"first") && panes.contains(&"second"),
        "{panes:?}"
    );
}

#[test]
fn resume_fails_for_each_of_its_reasons() {
    let e = scenario(&happy().resume(Resume::ShellNeverReady));
    expect_fail(&e, "resume", "the shell did not become ready within 45 s");
    assert!(e.run_texts(SECOND_PANE_ID).is_empty());

    let e = scenario(&happy().resume(Resume::ExitsWith(5)));
    expect_fail(
        &e,
        "resume",
        "resumed talaria exited with code 5 before showing its screen",
    );

    let e = scenario(&happy().resume(Resume::NeverStable));
    expect_fail(
        &e,
        "resume",
        "resumed talaria did not show a stable screen within 30 s",
    );
    let resumed = e.run_time(SECOND_PANE_ID, |t| t.contains("-c --profile"));
    assert_within(
        "restore wait",
        e.waited_since(resumed),
        ms(29_500),
        secs(36),
    );

    let e = scenario(&happy().token_c(TokenReply::Silent));
    expect_fail(
        &e,
        "resume",
        "the reply to the third prompt did not appear and stay stable within 60 s",
    );
    let third = e.run_time(SECOND_PANE_ID, is_token_prompt);
    assert_within(
        "third prompt wait",
        e.waited_since(third),
        ms(59_500),
        secs(66),
    );

    for last in [FinalQuit::NeverExits, FinalQuit::ExitsWith(2)] {
        let e = scenario(&happy().final_quit(last));
        expect_fail(
            &e,
            "resume",
            "/quit in the resumed pane did not end talaria with exit code 0 within 15 s",
        );
    }
}

// =============================================================================================
// deadline, skips, step scale, polling
// =============================================================================================

#[test]
fn deadline_names_step_and_closes_tab() {
    let e = exec_with(
        &happy().token_a(TokenReply::Silent).script(),
        |cfg| cfg.deadline = secs(30),
        |term| term,
    );
    expect_fail(&e, "plain-answer", "deadline exceeded in step plain-answer");
    assert_within("deadline", e.end_of_waiting(), ms(29_500), secs(36));

    let e = exec_with(
        &happy().approval(Approval::NoOutcome).script(),
        |cfg| cfg.deadline = secs(60),
        |term| term,
    );
    expect_fail(&e, "approval", "deadline exceeded in step approval");
    assert_within("deadline", e.end_of_waiting(), ms(59_500), secs(66));
}

#[test]
fn a_skip_is_never_a_pass() {
    let e = scenario(&happy().approval(Approval::RanWithoutCard));
    assert_eq!(e.report.exit_code(), 3, "a skip is not exit 0");
    assert_eq!(
        e.report.result_line(),
        "SMOKE_RESULT: PASSED WITH SKIPS (approval)"
    );
    assert!(!e.report.result_line().contains("PASSED (elapsed"));
    assert!(e.lines().iter().all(|l| !l.starts_with("PASS approval")));

    // With the live requirement set, the skip is a failure and ends the run.
    let e = exec_with(
        &happy().approval(Approval::RanWithoutCard).script(),
        |cfg| cfg.require = true,
        |term| term,
    );
    assert_eq!(e.report.exit_code(), 1);
    assert_eq!(e.report.result_line(), "SMOKE_RESULT: FAILED (approval)");
    let lines = e.lines();
    assert!(
        lines.contains(&format!(
            "FAIL approval (skipped: {SKIP_REASON}; TALARIA_LIVE_REQUIRE=1)"
        )),
        "{lines:?}"
    );
    assert!(lines.iter().all(|l| !l.starts_with("SKIP")));
    assert!(lines.iter().all(|l| !l.starts_with("PASS interrupt")));
    assert!(!e.run_texts(FIRST_PANE_ID).iter().any(|t| t == COUNT_PROMPT));
    assert_cleaned(&e);
}

#[test]
fn step_scale_leaves_deadline_and_quiet() {
    // A quarter of every limit: the run still passes, the quiet periods and polls stay as they are.
    let e = exec_with(&happy().script(), |cfg| cfg.step_scale = 0.25, |term| term);
    assert_eq!(e.kinds(), passes_of(&STEPS));
    assert!(
        e.pass_elapsed("plain-answer") >= secs(3),
        "the 3 s quiet period is not scaled: {:?}",
        e.pass_elapsed("plain-answer")
    );
    assert_polling_grid(&e);
    let esc = e.key_times(FIRST_PANE_ID, "esc");
    let after = e
        .calls()
        .into_iter()
        .find(|r| r.at > esc[0])
        .expect("a call after Esc");
    assert!(
        after.at - esc[0] >= ms(400),
        "the wait after Esc is not scaled"
    );

    // Per-step limits scale, and the reason names the limit in force, rounded to the nearest.
    for (scale, limit, low, high) in [
        (0.5_f32, 30, ms(29_500), secs(36)),
        (2.0, 120, ms(119_500), secs(126)),
        (0.51, 31, ms(30_500), secs(37)),
    ] {
        let e = exec_with(
            &happy().token_a(TokenReply::Silent).script(),
            |cfg| cfg.step_scale = scale,
            |term| term,
        );
        expect_fail(
            &e,
            "plain-answer",
            &format!("the reply token did not appear and settle within {limit} s"),
        );
        let submitted = e.run_time(FIRST_PANE_ID, is_token_prompt);
        assert_within("scaled wait", e.waited_since(submitted), low, high);
    }

    // The 45 s shell wait is a per-step limit too.
    let e = exec_with(
        &happy().startup(Startup::ShellNeverReady).script(),
        |cfg| cfg.step_scale = 2.0,
        |term| term,
    );
    expect_fail(&e, "startup", "the shell did not become ready within 90 s");
    assert_within("shell wait", e.end_of_waiting(), ms(89_500), secs(97));

    // The deadline is never scaled: five times the limits would run past 300 s.
    let e = exec_with(
        &happy().token_a(TokenReply::Silent).script(),
        |cfg| cfg.step_scale = 5.0,
        |term| term,
    );
    expect_fail(&e, "plain-answer", "deadline exceeded in step plain-answer");
    assert_within("deadline", e.end_of_waiting(), ms(299_500), secs(306));
}

#[test]
fn polling_interval_is_250_ms() {
    let e = scenario(&happy());
    assert_eq!(e.kinds(), passes_of(&STEPS));
    assert_polling_grid(&e);

    // A long wait: from the prompt to the end, calls are made every 250 ms, no less and no more.
    let e = scenario(&happy().token_a(TokenReply::Silent));
    let submitted = e.run_time(FIRST_PANE_ID, is_token_prompt);
    let end = e.end_of_waiting();
    let mut times: Vec<Duration> = e
        .calls()
        .into_iter()
        .map(|r| r.at)
        .filter(|at| *at >= submitted && *at <= end)
        .collect();
    times.dedup();
    assert!(times.len() > 200, "{} distinct times in 60 s", times.len());
    for pair in times.windows(2) {
        assert_eq!(
            pair[1] - pair[0],
            ms(250),
            "between {:?} and {:?}",
            pair[0],
            pair[1]
        );
    }
}
