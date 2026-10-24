//! What the smoke run types into a pane. It must never type the word that would switch Hermes
//! into approving dangerous commands by itself, and the only slash command it types is `/quit`.
//! Both are checked over the happy path and over every failure and skip path of the scenarios.

mod support;

use std::sync::OnceLock;
use std::sync::atomic::AtomicBool;
use std::time::Duration;

use smoke::{ManualClock, SmokeConfig};
use support::{
    Approval, Call, FakeTerminal, FinalQuit, Interrupt, Quit, Resume, Scenario, Startup, TokenReply,
};

/// The Hermes prompt field that this unit must never cause to be sent.
const FORBIDDEN: &str = "unattended";

/// Everything one run sent to the terminal.
struct Typed {
    /// Text of `pane run`.
    runs: Vec<String>,
    /// Text of `pane send-text`.
    texts: Vec<String>,
    /// Key names of `pane send-keys`.
    keys: Vec<String>,
    /// The label of the tab.
    label: String,
}

impl Typed {
    /// Every string that reached a pane or the tab list.
    fn everything(&self) -> impl Iterator<Item = &String> {
        self.runs
            .iter()
            .chain(&self.texts)
            .chain(&self.keys)
            .chain(std::iter::once(&self.label))
    }
}

fn play(scenario: &Scenario, require: bool) -> Typed {
    let clock: &'static ManualClock = Box::leak(Box::new(ManualClock::new()));
    let temp = tempfile::tempdir().expect("temporary folder for the run");
    let root = std::fs::canonicalize(temp.path()).expect("resolve the temporary folder");
    let talaria = root.join("talaria");
    support::write_executable(&talaria, "#!/bin/sh\nexit 0\n");
    let cfg = SmokeConfig {
        talaria,
        profile: "fixture".to_owned(),
        model: "minimax-m3.1-flash".to_owned(),
        provider: "ollama-cloud".to_owned(),
        deadline: Duration::from_secs(300),
        step_scale: 1.0,
        report_dir: None,
        require,
        temp_root: root,
        seed: Some(3),
    };
    let term = FakeTerminal::new(scenario.build(), clock);
    let stop = AtomicBool::new(false);
    let _report = smoke::run_smoke(&term, clock, &cfg, &stop);

    let mut typed = Typed {
        runs: Vec::new(),
        texts: Vec::new(),
        keys: Vec::new(),
        label: String::new(),
    };
    for recorded in term.calls() {
        match recorded.call {
            Call::Run { text, .. } => typed.runs.push(text),
            Call::SendText { text, .. } => typed.texts.push(text),
            Call::SendKeys { keys, .. } => typed.keys.extend(keys),
            Call::CreateTab { label, .. } => typed.label = label,
            _ => {}
        }
    }
    typed
}

fn every_approval() -> Vec<Approval> {
    vec![
        Approval::Card,
        Approval::LateCard(8),
        Approval::CardEnterNeeded,
        Approval::CardOtherCommand,
        Approval::CardNoOnceNumber,
        Approval::CardHighlightElsewhere,
        Approval::CardIgnoresAnswer,
        Approval::AnsweredButNoDeletion,
        Approval::RanWithoutCard,
        Approval::Ignored,
        Approval::Withdrawn,
        Approval::NoOutcome,
    ]
}

fn every_token_reply() -> Vec<TokenReply> {
    vec![
        TokenReply::Reply,
        TokenReply::ReplyTwice,
        TokenReply::Silent,
        TokenReply::WrongForm,
        TokenReply::NeverSettles,
        TokenReply::NeedsEnter,
    ]
}

/// The happy path and one scenario for every value of every knob, with the live requirement set
/// or not where that matters.
fn every_path() -> Vec<(String, Scenario, bool)> {
    let happy = Scenario::happy;
    let mut paths: Vec<(String, Scenario, bool)> = vec![("happy".to_owned(), happy(), false)];
    let mut add = |name: String, scenario: Scenario, require: bool| {
        paths.push((name, scenario, require));
    };

    for startup in [
        Startup::Ok,
        Startup::ExitsWith(7),
        Startup::NeverStable,
        Startup::ShellNeverReady,
    ] {
        add(format!("{startup:?}"), happy().startup(startup), false);
    }
    for reply in every_token_reply() {
        add(format!("token A {reply:?}"), happy().token_a(reply), false);
        add(format!("token B {reply:?}"), happy().token_b(reply), false);
        add(format!("token C {reply:?}"), happy().token_c(reply), false);
    }
    for approval in every_approval() {
        add(
            format!("approval {approval:?}"),
            happy().approval(approval),
            false,
        );
        add(
            format!("approval {approval:?}, live requirement"),
            happy().approval(approval),
            true,
        );
        add(
            format!("approval Ignored then {approval:?}"),
            happy().approval(Approval::Ignored).approval_retry(approval),
            false,
        );
    }
    for interrupt in [
        Interrupt::Works,
        Interrupt::TooFewLines,
        Interrupt::NotGrowing,
        Interrupt::FinishedEarly,
        Interrupt::KeepsGrowing,
        Interrupt::CountFinishes,
        Interrupt::NoCountLeft,
        Interrupt::CountVanishes,
    ] {
        add(
            format!("{interrupt:?}"),
            happy().interrupt(interrupt),
            false,
        );
    }
    for quit in [
        Quit::Zero,
        Quit::ExitsWith(3),
        Quit::NeverExits,
        Quit::ShellNotBack,
        Quit::OutputVanishes,
        Quit::LosesSecondToken,
        Quit::NeedsEnter,
    ] {
        add(format!("{quit:?}"), happy().quit(quit), false);
    }
    for resume in [
        Resume::Transcript(0),
        Resume::Transcript(1),
        Resume::Transcript(2),
        Resume::OnlyFirstToken,
        Resume::ShellNeverReady,
        Resume::ExitsWith(5),
        Resume::NeverStable,
    ] {
        add(format!("{resume:?}"), happy().resume(resume), false);
    }
    for last in [
        FinalQuit::Zero,
        FinalQuit::ExitsWith(2),
        FinalQuit::NeverExits,
    ] {
        add(format!("{last:?}"), happy().final_quit(last), false);
    }
    paths
}

/// Every path played once; both tests read the same runs.
fn played() -> &'static [(String, Typed)] {
    static PLAYED: OnceLock<Vec<(String, Typed)>> = OnceLock::new();
    PLAYED.get_or_init(|| {
        every_path()
            .iter()
            .map(|(name, scenario, require)| (name.clone(), play(scenario, *require)))
            .collect()
    })
}

#[test]
fn nothing_typed_contains_unattended() {
    let paths = played();
    assert!(paths.len() >= 60, "{} paths", paths.len());
    let mut typed_in_total = 0;
    let mut typed_on_happy_path = 0;
    for (name, typed) in paths {
        if name == "happy" {
            typed_on_happy_path = typed.everything().count();
        }
        for text in typed.everything() {
            typed_in_total += 1;
            assert!(
                !text.to_lowercase().contains(FORBIDDEN),
                "{name}: {text:?} contains the forbidden word"
            );
        }
    }
    // The paths did type: a run that stopped before typing anything would make this vacuous.
    assert!(
        typed_on_happy_path >= 12,
        "the happy path typed {typed_on_happy_path} strings"
    );
    assert!(
        typed_in_total >= 200,
        "only {typed_in_total} strings were typed"
    );
}

#[test]
fn the_only_slash_command_is_quit() {
    let mut quits_on_happy_path = 0;
    for (name, typed) in played() {
        for text in typed.runs.iter().chain(&typed.texts) {
            assert!(!text.contains('\n'), "{name}: {text:?} holds a newline");
            if text.trim_start().starts_with('/') {
                assert_eq!(text, "/quit", "{name}: a slash command other than /quit");
            }
        }
        assert!(
            typed.keys.iter().all(|key| !key.starts_with('/')),
            "{name}: a key that types a slash: {:?}",
            typed.keys
        );
        if name == "happy" {
            quits_on_happy_path = typed.runs.iter().filter(|t| *t == "/quit").count();
        }
    }
    // One /quit per pane on a full run, and no other slash line at all.
    assert_eq!(quits_on_happy_path, 2);
}
