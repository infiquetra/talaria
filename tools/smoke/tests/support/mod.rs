//! Test support for the smoke crate: a scripted in-process terminal, a stateful fake `herdr`
//! executable, a builder for the screen scripts that drive both, and helpers that run the built
//! `smoke` binary against the fake.
//!
//! The pieces:
//!
//! * [`script`]: the screen script format and the machine that plays it.
//! * [`ScriptBuilder`], [`RuleBuilder`], [`StageBuilder`], [`FailBuilder`]: compose a script
//!   without writing JSON.
//! * [`Scenario`]: a complete run (every step) with one knob per way a step can go right, wrong
//!   or be skipped; the default is the happy path.
//! * [`FakeTerminal`]: the same machine behind `smoke::Terminal`, recording every call.
//! * [`FakeHerdr`], [`Harness`], [`SmokeCmd`]: the fake executable, a scratch environment around
//!   it and the command that starts the built `smoke` binary.

#![allow(dead_code)]

use std::cell::Cell;
use std::fs;
use std::io::Read as _;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::{Mutex, MutexGuard};
use std::time::{Duration, Instant};

use serde_json::{Map, Value, json};
use smoke::{Clock, NewTab, PaneId, ReadSource, TabId, TermError, Terminal};

#[path = "script.rs"]
pub mod script;

#[path = "paster2a.rs"]
pub mod paster2a;

pub use paster2a::{FakePaster, PasterCall, PasterRecorded};

#[path = "scratch2b.rs"]
pub mod scratch2b;
#[path = "scripts2a.rs"]
pub mod scripts2a;
#[path = "scripts2b.rs"]
pub mod scripts2b;

#[path = "tmux_fake.rs"]
pub mod tmux_fake;

#[allow(unused_imports)]
pub use script::{
    FIRST, FIRST_PANE_ID, Failure, Input, Machine, SECOND, SECOND_PANE_ID, Script, TAB_ID,
    VISIBLE_ROWS, WORKSPACE_ID,
};

/// The text a planned panic carries.
pub const PANIC_MESSAGE: &str = "fake terminal: planned panic";

/// The recorded layout of the approval card (a placeholder until a live capture replaces it).
pub const CARD_FIXTURE: &str = include_str!("../fixtures/screens/approval_card.txt");

/// The command the approval step asks for.
pub const APPROVAL_COMMAND: &str = "rm -rf .pytest_cache";

const PROMPT_PATTERN: &str =
    r"join these two words with a hyphen and reply with only the result: (\w+) and (\d+)$";
const APPROVAL_PATTERN: &str = r"rm -rf \.pytest_cache$";
const COUNT_PATTERN: &str = r"Count from 1 to 2000, one number per line, and do not stop early\.$";
const START_PATTERN: &str = r"--no-ensure --profile ";
const RESUME_PATTERN: &str = r"--no-ensure -c --profile ";
const FOREVER: u32 = 1_000_000;

// ---------------------------------------------------------------------------------------------
// Script builder
// ---------------------------------------------------------------------------------------------

/// The effects of one stage of a rule (see the script format).
#[derive(Clone, Debug, Default)]
pub struct StageBuilder {
    map: Map<String, Value>,
}

impl StageBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    fn set(mut self, key: &str, value: Value) -> Self {
        self.map.insert(key.to_owned(), value);
        self
    }

    pub fn after_polls(self, reads: u32) -> Self {
        self.set("after_polls", json!(reads))
    }

    pub fn append(self, text: &str) -> Self {
        self.set("append", json!(text))
    }

    /// Churn lines (a turn that is still running) on this many reads after the rule fired.
    pub fn churn(self, reads: u32) -> Self {
        self.set("churn_polls", json!(reads))
    }

    /// Churn lines built from a template; `{n}` becomes `start + step * index`.
    pub fn churn_lines(self, template: &str, start: i64, step: i64) -> Self {
        self.set("churn_line", json!(template))
            .set("churn_start", json!(start))
            .set("churn_step", json!(step))
    }

    /// The foreground command lines; an empty list is the shell alone.
    pub fn foreground(self, lines: &[&str]) -> Self {
        self.set("foreground", json!(lines))
    }

    pub fn remove_in_cwd(self, path: &str) -> Self {
        self.set("remove_in_cwd", json!(path))
    }

    /// Writes the file under the pane's working folder (`{path, text}`).
    pub fn write_in_cwd(self, path: &str, text: &str) -> Self {
        self.set("write_in_cwd", json!({"path": path, "text": text}))
    }

    /// Writes the file under the pane's working folder and sets its mode (`{path, text, mode}`).
    pub fn write_in_cwd_with_mode(self, path: &str, text: &str, mode: u32) -> Self {
        self.set(
            "write_in_cwd",
            json!({"path": path, "text": text, "mode": mode}),
        )
    }

    /// The text `pane get` reports from now on.
    pub fn get(self, text: &str) -> Self {
        self.set("get", json!(text))
    }

    pub fn clear(self) -> Self {
        self.set("clear", json!(true))
    }

    /// Drops the whole text of the pane, the recent text included.
    pub fn wipe(self) -> Self {
        self.set("wipe", json!(true))
    }

    /// Stops every running churn of the pane.
    pub fn stop_churn(self) -> Self {
        self.set("stop_churn", json!(true))
    }
}

/// One rule of a pane.
#[derive(Clone, Debug)]
pub struct RuleBuilder {
    on: &'static str,
    matches: String,
    once: bool,
    remember: Option<String>,
    stage: StageBuilder,
    then: Vec<StageBuilder>,
}

impl RuleBuilder {
    fn new(on: &'static str, matches: &str) -> Self {
        RuleBuilder {
            on,
            matches: matches.to_owned(),
            once: true,
            remember: None,
            stage: StageBuilder::new(),
            then: Vec::new(),
        }
    }

    /// Fires on a `pane run` whose text matches the regular expression.
    pub fn run(matches: &str) -> Self {
        Self::new("run", matches)
    }

    /// Fires on a `pane send-text` whose text matches.
    pub fn send_text(matches: &str) -> Self {
        Self::new("send_text", matches)
    }

    /// Fires on a `pane send-keys` key whose name matches.
    pub fn send_keys(matches: &str) -> Self {
        Self::new("send_keys", matches)
    }

    /// Fires on pasted text that matches; the fake paster feeds the same machine.
    pub fn paste(matches: &str) -> Self {
        Self::new("paste", matches)
    }

    /// May fire again and again (default: once).
    pub fn repeat(mut self) -> Self {
        self.once = false;
        self
    }

    /// Pushes a template to the list shared by all panes (`${mem}` in later templates).
    pub fn remember(mut self, template: &str) -> Self {
        self.remember = Some(template.to_owned());
        self
    }

    pub fn after_polls(mut self, reads: u32) -> Self {
        self.stage = self.stage.after_polls(reads);
        self
    }

    pub fn append(mut self, text: &str) -> Self {
        self.stage = self.stage.append(text);
        self
    }

    pub fn churn(mut self, reads: u32) -> Self {
        self.stage = self.stage.churn(reads);
        self
    }

    pub fn churn_lines(mut self, template: &str, start: i64, step: i64) -> Self {
        self.stage = self.stage.churn_lines(template, start, step);
        self
    }

    pub fn foreground(mut self, lines: &[&str]) -> Self {
        self.stage = self.stage.foreground(lines);
        self
    }

    pub fn remove_in_cwd(mut self, path: &str) -> Self {
        self.stage = self.stage.remove_in_cwd(path);
        self
    }

    pub fn write_in_cwd(mut self, path: &str, text: &str) -> Self {
        self.stage = self.stage.write_in_cwd(path, text);
        self
    }

    pub fn write_in_cwd_with_mode(mut self, path: &str, text: &str, mode: u32) -> Self {
        self.stage = self.stage.write_in_cwd_with_mode(path, text, mode);
        self
    }

    pub fn get(mut self, text: &str) -> Self {
        self.stage = self.stage.get(text);
        self
    }

    pub fn clear(mut self) -> Self {
        self.stage = self.stage.clear();
        self
    }

    pub fn wipe(mut self) -> Self {
        self.stage = self.stage.wipe();
        self
    }

    pub fn stop_churn(mut self) -> Self {
        self.stage = self.stage.stop_churn();
        self
    }

    /// A further stage, counted from the moment the rule fired.
    pub fn then(mut self, stage: StageBuilder) -> Self {
        self.then.push(stage);
        self
    }

    pub fn to_value(&self) -> Value {
        let mut map = self.stage.map.clone();
        map.insert("on".to_owned(), json!(self.on));
        map.insert("matches".to_owned(), json!(self.matches));
        map.insert("once".to_owned(), json!(self.once));
        if let Some(template) = &self.remember {
            map.insert("remember".to_owned(), json!(template));
        }
        if !self.then.is_empty() {
            let stages: Vec<Value> = self
                .then
                .iter()
                .map(|s| Value::Object(s.map.clone()))
                .collect();
            map.insert("then".to_owned(), Value::Array(stages));
        }
        Value::Object(map)
    }
}

/// A planned error answer for one kind of call.
#[derive(Clone, Debug)]
pub struct FailBuilder {
    map: Map<String, Value>,
}

impl FailBuilder {
    /// `call` is `tab create`, `tab list`, `tab close`, `pane split`, `pane run`,
    /// `pane send-text`, `pane send-keys`, `pane read` or `pane process-info`.
    pub fn call(call: &str, code: &str) -> Self {
        let mut map = Map::new();
        map.insert("call".to_owned(), json!(call));
        map.insert("code".to_owned(), json!(code));
        FailBuilder { map }
    }

    pub fn message(mut self, message: &str) -> Self {
        self.map.insert("message".to_owned(), json!(message));
        self
    }

    /// Only for calls on this pane (`first` or `second`).
    pub fn pane(mut self, pane: &str) -> Self {
        self.map.insert("pane".to_owned(), json!(pane));
        self
    }

    /// Let this many matching calls through before failing.
    pub fn after(mut self, calls: u32) -> Self {
        self.map.insert("after".to_owned(), json!(calls));
        self
    }

    /// Stop failing after this many failures.
    pub fn times(mut self, failures: u32) -> Self {
        self.map.insert("times".to_owned(), json!(failures));
        self
    }
}

#[derive(Clone, Debug)]
struct PaneBuilder {
    initial: String,
    foreground: Vec<String>,
    rules: Vec<RuleBuilder>,
}

impl PaneBuilder {
    fn shell() -> Self {
        PaneBuilder {
            initial: "$ ".to_owned(),
            foreground: vec!["-bash".to_owned()],
            rules: Vec::new(),
        }
    }

    fn to_value(&self) -> Value {
        json!({
            "initial": self.initial,
            "foreground": self.foreground,
            "rules": self.rules.iter().map(RuleBuilder::to_value).collect::<Vec<_>>(),
        })
    }
}

/// Composes a screen script: two shell panes, their rules and the planned failures.
#[derive(Clone, Debug)]
pub struct ScriptBuilder {
    first: PaneBuilder,
    second: PaneBuilder,
    fail: Vec<FailBuilder>,
}

impl Default for ScriptBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl ScriptBuilder {
    /// Two plain shell panes (initial text `$ `, foreground `-bash`) and no rules.
    pub fn new() -> Self {
        ScriptBuilder {
            first: PaneBuilder::shell(),
            second: PaneBuilder::shell(),
            fail: Vec::new(),
        }
    }

    pub fn first_initial(mut self, text: &str) -> Self {
        self.first.initial = text.to_owned();
        self
    }

    pub fn first_foreground(mut self, lines: &[&str]) -> Self {
        self.first.foreground = lines.iter().map(|s| (*s).to_owned()).collect();
        self
    }

    pub fn second_initial(mut self, text: &str) -> Self {
        self.second.initial = text.to_owned();
        self
    }

    pub fn second_foreground(mut self, lines: &[&str]) -> Self {
        self.second.foreground = lines.iter().map(|s| (*s).to_owned()).collect();
        self
    }

    pub fn on_first(mut self, rule: RuleBuilder) -> Self {
        self.first.rules.push(rule);
        self
    }

    pub fn on_second(mut self, rule: RuleBuilder) -> Self {
        self.second.rules.push(rule);
        self
    }

    pub fn fail(mut self, entry: FailBuilder) -> Self {
        self.fail.push(entry);
        self
    }

    pub fn to_value(&self) -> Value {
        json!({
            "panes": {"first": self.first.to_value(), "second": self.second.to_value()},
            "fail": self.fail.iter().map(|f| Value::Object(f.map.clone())).collect::<Vec<_>>(),
        })
    }

    pub fn to_json(&self) -> String {
        self.to_value().to_string()
    }

    /// The parsed script (panics on a builder mistake, such as a bad regular expression).
    pub fn build(&self) -> Script {
        Script::from_value(&self.to_value()).expect("the script builder made a bad script")
    }
}

// ---------------------------------------------------------------------------------------------
// Scenarios: every step of a run, with a knob per way it can go
// ---------------------------------------------------------------------------------------------

/// How `talaria` starts in the first pane.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Startup {
    /// A stable screen and `talaria` in the foreground.
    Ok,
    /// The process ends at once with this exit code and a one line message.
    ExitsWith(i32),
    /// The screen never stops changing.
    NeverStable,
    /// The shell never becomes ready (blank screen, a command still running).
    ShellNeverReady,
}

/// How the model answers a token prompt.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TokenReply {
    /// The joined token appears once after a short wait, then the screen stays still.
    Reply,
    /// The token appears twice (an answer and a reasoning cell).
    ReplyTwice,
    /// The prompt is shown and nothing more happens.
    Silent,
    /// The answer is `word number` without the hyphen.
    WrongForm,
    /// The token appears but the screen never stops changing.
    NeverSettles,
    /// Nothing happens until an Enter key arrives (the prompt was read as a paste).
    NeedsEnter,
}

/// What the approval step meets in one attempt.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Approval {
    /// A card after three reads; the digit `1` answers it and deletes the folder.
    Card,
    /// The same, after this many reads.
    LateCard(u32),
    /// A card that only an Enter key (not the digit) answers.
    CardEnterNeeded,
    /// A card for `cd ~ && rm -rf .pytest_cache`.
    CardOtherCommand,
    /// A card whose choices carry no numbers.
    CardNoOnceNumber,
    /// A card whose highlight sits on the second choice.
    CardHighlightElsewhere,
    /// A card that nothing answers; it stays open.
    CardIgnoresAnswer,
    /// The digit closes the card but the folder is never deleted.
    AnsweredButNoDeletion,
    /// The folder is deleted with no card at all.
    RanWithoutCard,
    /// The model declines; the folder stays and the screen settles.
    Ignored,
    /// Like `Ignored`, with a line saying the approval was withdrawn.
    Withdrawn,
    /// The screen keeps changing, with no card and no outcome.
    NoOutcome,
}

/// How the counting prompt and the Esc key behave.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Interrupt {
    /// One more number on every read until Esc; Esc stops it.
    Works,
    /// Only ten counting lines ever appear.
    TooFewLines,
    /// Exactly fifteen counting lines, then nothing changes.
    NotGrowing,
    /// The count starts above 1500.
    FinishedEarly,
    /// Esc is ignored and the count goes on.
    KeepsGrowing,
    /// Esc stops the count with a visible 1950.
    CountFinishes,
    /// Esc clears the screen; no counting line is left.
    NoCountLeft,
    /// Esc clears the screen and leaves a smaller number than before.
    CountVanishes,
}

/// What `/quit` does in the first pane.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Quit {
    Zero,
    ExitsWith(i32),
    /// Nothing happens.
    NeverExits,
    /// `SMOKE_EXIT:0` appears but the foreground list keeps the launch line.
    ShellNotBack,
    /// The exit marker appears but all earlier text is gone.
    OutputVanishes,
    /// The exit marker appears and only the first token is left.
    LosesSecondToken,
    /// Nothing happens until an Enter key arrives.
    NeedsEnter,
}

/// What the resumed `talaria` does in the second pane.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Resume {
    /// The earlier tokens are printed this many times (0 loses them all).
    Transcript(u32),
    /// Only the first token is printed.
    OnlyFirstToken,
    /// The second pane's shell never becomes ready.
    ShellNeverReady,
    /// The process ends at once with this exit code.
    ExitsWith(i32),
    /// The screen never stops changing.
    NeverStable,
}

/// What `/quit` does in the second pane.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FinalQuit {
    Zero,
    ExitsWith(i32),
    NeverExits,
}

/// A whole run. [`Scenario::happy`] is the happy path; each method changes one step.
#[derive(Clone, Debug)]
pub struct Scenario {
    pub startup: Startup,
    pub token_a: TokenReply,
    pub approval: Approval,
    pub approval_retry: Approval,
    pub interrupt: Interrupt,
    pub token_b: TokenReply,
    pub quit: Quit,
    pub resume: Resume,
    pub token_c: TokenReply,
    pub final_quit: FinalQuit,
}

impl Scenario {
    pub fn happy() -> Self {
        Scenario {
            startup: Startup::Ok,
            token_a: TokenReply::Reply,
            approval: Approval::Card,
            approval_retry: Approval::Ignored,
            interrupt: Interrupt::Works,
            token_b: TokenReply::Reply,
            quit: Quit::Zero,
            resume: Resume::Transcript(1),
            token_c: TokenReply::Reply,
            final_quit: FinalQuit::Zero,
        }
    }

    pub fn startup(mut self, v: Startup) -> Self {
        self.startup = v;
        self
    }

    pub fn token_a(mut self, v: TokenReply) -> Self {
        self.token_a = v;
        self
    }

    /// The first approval attempt.
    pub fn approval(mut self, v: Approval) -> Self {
        self.approval = v;
        self
    }

    /// The second attempt, used only if the first leaves the folder in place.
    pub fn approval_retry(mut self, v: Approval) -> Self {
        self.approval_retry = v;
        self
    }

    pub fn interrupt(mut self, v: Interrupt) -> Self {
        self.interrupt = v;
        self
    }

    /// The follow-up token prompt after the interrupt.
    pub fn token_b(mut self, v: TokenReply) -> Self {
        self.token_b = v;
        self
    }

    pub fn quit(mut self, v: Quit) -> Self {
        self.quit = v;
        self
    }

    pub fn resume(mut self, v: Resume) -> Self {
        self.resume = v;
        self
    }

    /// The third token prompt, in the resumed pane.
    pub fn token_c(mut self, v: TokenReply) -> Self {
        self.token_c = v;
        self
    }

    pub fn final_quit(mut self, v: FinalQuit) -> Self {
        self.final_quit = v;
        self
    }

    /// The script, still open to more rules and failures.
    pub fn script(&self) -> ScriptBuilder {
        let mut script = ScriptBuilder::new();
        let mut first: Vec<RuleBuilder> = Vec::new();
        let mut second: Vec<RuleBuilder> = Vec::new();

        match self.startup {
            Startup::ShellNeverReady => {
                script = script
                    .first_initial("")
                    .first_foreground(&["-bash", "busy"]);
            }
            other => first.push(start_rule(START_PATTERN, other_startup(other))),
        }
        first.extend(token_rules(self.token_a));
        first.extend(approval_rules(&[self.approval, self.approval_retry]));
        first.extend(interrupt_rules(self.interrupt));
        first.extend(token_rules(self.token_b));
        first.extend(quit_rules(self.quit));

        match self.resume {
            Resume::ShellNeverReady => {
                script = script
                    .second_initial("")
                    .second_foreground(&["-bash", "busy"]);
            }
            other => second.push(resume_rule(other)),
        }
        second.extend(token_rules(self.token_c));
        second.extend(final_quit_rules(self.final_quit));

        for rule in first {
            script = script.on_first(rule);
        }
        for rule in second {
            script = script.on_second(rule);
        }
        script
    }

    pub fn build(&self) -> Script {
        self.script().build()
    }
}

/// How a start line ends up; the same shapes serve the resume line.
enum Launch {
    Stable,
    Exits(i32),
    NeverStable,
}

fn other_startup(startup: Startup) -> Launch {
    match startup {
        Startup::Ok | Startup::ShellNeverReady => Launch::Stable,
        Startup::ExitsWith(n) => Launch::Exits(n),
        Startup::NeverStable => Launch::NeverStable,
    }
}

fn start_rule(pattern: &str, launch: Launch) -> RuleBuilder {
    let rule = RuleBuilder::run(pattern);
    match launch {
        Launch::Stable => rule
            .foreground(&["-bash", "$0"])
            .append("$0\nTalaria\nConnecting to Hermes"),
        Launch::Exits(n) => rule.append(&format!("$0\nstub: no gateway\nSMOKE_EXIT:{n}\n$ ")),
        Launch::NeverStable => rule
            .foreground(&["-bash", "$0"])
            .append("$0\nTalaria")
            .churn(FOREVER),
    }
}

fn resume_rule(resume: Resume) -> RuleBuilder {
    let transcript = |copies: u32| {
        let mut text = "$0\nresumed session".to_owned();
        for _ in 0..copies {
            text.push_str("\n${mem}");
        }
        text
    };
    match resume {
        Resume::Transcript(copies) => RuleBuilder::run(RESUME_PATTERN)
            .foreground(&["-bash", "$0"])
            .append(&transcript(copies)),
        Resume::OnlyFirstToken => RuleBuilder::run(RESUME_PATTERN)
            .foreground(&["-bash", "$0"])
            .append("$0\nresumed session\n${mem:0}"),
        Resume::ExitsWith(n) => start_rule(RESUME_PATTERN, Launch::Exits(n)),
        Resume::NeverStable => start_rule(RESUME_PATTERN, Launch::NeverStable),
        Resume::ShellNeverReady => start_rule(RESUME_PATTERN, Launch::Stable),
    }
}

/// The rules for one token prompt (and the Enter key it may wait for).
fn token_rules(reply: TokenReply) -> Vec<RuleBuilder> {
    let prompt = || RuleBuilder::run(PROMPT_PATTERN).append("you: $0");
    let enter = || RuleBuilder::send_keys("^enter$");
    match reply {
        TokenReply::Reply => vec![
            prompt()
                .remember("$1-$2")
                .churn(4)
                .then(StageBuilder::new().after_polls(4).append("agent: $1-$2")),
        ],
        TokenReply::ReplyTwice => vec![
            prompt().remember("$1-$2").churn(4).then(
                StageBuilder::new()
                    .after_polls(4)
                    .append("agent: $1-$2\nagent (reasoning): $1-$2"),
            ),
        ],
        TokenReply::Silent => vec![prompt()],
        TokenReply::WrongForm => vec![
            prompt()
                .churn(4)
                .then(StageBuilder::new().after_polls(4).append("agent: $1 $2")),
        ],
        TokenReply::NeverSettles => vec![
            prompt()
                .remember("$1-$2")
                .churn(FOREVER)
                .then(StageBuilder::new().after_polls(2).append("agent: $1-$2")),
        ],
        TokenReply::NeedsEnter => vec![
            prompt().remember("$1-$2"),
            enter().churn(4).then(
                StageBuilder::new()
                    .after_polls(4)
                    .append("agent: ${mem:last}"),
            ),
        ],
    }
}

fn card_text(attempt: Approval) -> String {
    let start = CARD_FIXTURE.find("Approval needed").unwrap_or(0);
    let card = CARD_FIXTURE[start..].trim_end().to_owned();
    match attempt {
        Approval::CardOtherCommand => card.replace(
            &format!("$ {APPROVAL_COMMAND}"),
            &format!("$ cd ~ && {APPROVAL_COMMAND}"),
        ),
        Approval::CardNoOnceNumber => card
            .replace("› 1. once", "› once")
            .replace("  2. session", "  session")
            .replace("  3. always", "  always")
            .replace("  4. deny", "  deny"),
        Approval::CardHighlightElsewhere => card
            .replace("› 1. once", "  1. once")
            .replace("  2. session", "› 2. session"),
        _ => card,
    }
}

fn approval_rules(attempts: &[Approval]) -> Vec<RuleBuilder> {
    let mut rules = Vec::new();
    let mut digit = false;
    let mut hangs = false;
    let mut enter = false;
    for attempt in attempts {
        let prompt = || RuleBuilder::run(APPROVAL_PATTERN).append("you: $0");
        let card_after = |reads: u32| {
            prompt().churn(2).then(
                StageBuilder::new()
                    .after_polls(reads)
                    .append(&card_text(*attempt)),
            )
        };
        match attempt {
            Approval::Card => {
                digit = true;
                rules.push(card_after(3));
            }
            Approval::LateCard(reads) => {
                digit = true;
                rules.push(card_after(*reads));
            }
            Approval::CardEnterNeeded => {
                enter = true;
                rules.push(card_after(3));
            }
            Approval::CardOtherCommand
            | Approval::CardNoOnceNumber
            | Approval::CardHighlightElsewhere
            | Approval::CardIgnoresAnswer => rules.push(card_after(3)),
            Approval::AnsweredButNoDeletion => {
                hangs = true;
                rules.push(card_after(3));
            }
            Approval::RanWithoutCard => rules.push(
                prompt().churn(2).then(
                    StageBuilder::new()
                        .after_polls(3)
                        .remove_in_cwd(".pytest_cache")
                        .append("ran: rm -rf .pytest_cache, exit code 0"),
                ),
            ),
            Approval::Ignored => rules.push(
                prompt().churn(2).then(
                    StageBuilder::new()
                        .after_polls(3)
                        .append("agent: I cannot run shell commands here."),
                ),
            ),
            Approval::Withdrawn => rules.push(
                prompt().churn(2).then(
                    StageBuilder::new()
                        .after_polls(3)
                        .append("The approval was withdrawn: the client could not answer it."),
                ),
            ),
            Approval::NoOutcome => rules.push(prompt().churn(FOREVER)),
        }
    }
    let finished = "Command finished with exit code 0.";
    if digit {
        rules.push(
            RuleBuilder::send_text("^1$")
                .remove_in_cwd(".pytest_cache")
                .clear()
                .append(finished),
        );
    }
    if enter {
        rules.push(
            RuleBuilder::send_keys("^enter$")
                .remove_in_cwd(".pytest_cache")
                .clear()
                .append(finished),
        );
    }
    if hangs {
        rules.push(
            RuleBuilder::send_text("^1$")
                .clear()
                .append("Waiting for the command..."),
        );
    }
    rules
}

fn interrupt_rules(interrupt: Interrupt) -> Vec<RuleBuilder> {
    let counting = |reads: u32, start: i64| {
        RuleBuilder::run(COUNT_PATTERN)
            .append("you: $0")
            .churn(reads)
            .churn_lines("{n}", start, 1)
    };
    let esc = || RuleBuilder::send_keys("^esc$").stop_churn();
    match interrupt {
        Interrupt::Works => vec![counting(FOREVER, 1), esc().append("interrupted by user")],
        Interrupt::TooFewLines => vec![counting(10, 1)],
        Interrupt::NotGrowing => vec![counting(15, 1), esc().append("interrupted by user")],
        Interrupt::FinishedEarly => vec![counting(300, 1600), esc().append("interrupted by user")],
        Interrupt::KeepsGrowing => vec![counting(FOREVER, 1)],
        Interrupt::CountFinishes => vec![counting(FOREVER, 1), esc().append("1950")],
        Interrupt::NoCountLeft => vec![
            counting(FOREVER, 1),
            esc().clear().append("interrupted by user"),
        ],
        Interrupt::CountVanishes => vec![counting(FOREVER, 1), esc().clear().append("3")],
    }
}

fn exit_text(code: i32) -> String {
    format!("you: /quit\nSession detached.\nSMOKE_EXIT:{code}\n$ ")
}

fn quit_rules(quit: Quit) -> Vec<RuleBuilder> {
    let quit_rule = || RuleBuilder::run("^/quit$");
    match quit {
        Quit::Zero => vec![quit_rule().foreground(&[]).append(&exit_text(0))],
        Quit::ExitsWith(n) => vec![quit_rule().foreground(&[]).append(&exit_text(n))],
        Quit::NeverExits => vec![quit_rule().append("you: /quit")],
        Quit::ShellNotBack => vec![quit_rule().append(&exit_text(0))],
        Quit::OutputVanishes => vec![quit_rule().wipe().foreground(&[]).append(&exit_text(0))],
        Quit::LosesSecondToken => vec![
            quit_rule()
                .wipe()
                .foreground(&[])
                .append(&format!("${{mem:0}}\n{}", exit_text(0))),
        ],
        Quit::NeedsEnter => vec![
            quit_rule().append("you: /quit"),
            RuleBuilder::send_keys("^enter$")
                .foreground(&[])
                .append("Session detached.\nSMOKE_EXIT:0\n$ "),
        ],
    }
}

fn final_quit_rules(quit: FinalQuit) -> Vec<RuleBuilder> {
    let quit_rule = || RuleBuilder::run("^/quit$");
    match quit {
        FinalQuit::Zero => vec![quit_rule().foreground(&[]).append(&exit_text(0))],
        FinalQuit::ExitsWith(n) => vec![quit_rule().foreground(&[]).append(&exit_text(n))],
        FinalQuit::NeverExits => vec![quit_rule().append("you: /quit")],
    }
}

// ---------------------------------------------------------------------------------------------
// Local clock (only for tests that need a Clock without the product's ManualClock)
// ---------------------------------------------------------------------------------------------

/// A clock whose `sleep` advances time without waiting.
#[derive(Default)]
pub struct LocalClock {
    now: Cell<Duration>,
}

impl LocalClock {
    pub fn new() -> Self {
        Self::default()
    }
}

impl Clock for LocalClock {
    fn now(&self) -> Duration {
        self.now.get()
    }

    fn sleep(&self, d: Duration) {
        self.now.set(self.now.get() + d);
    }
}

// ---------------------------------------------------------------------------------------------
// FakeTerminal
// ---------------------------------------------------------------------------------------------

/// The ids that [`FakeTerminal`] hands out (the ids of the recorded fixtures).
pub fn tab_id() -> TabId {
    TabId(TAB_ID.to_owned())
}

pub fn first_pane() -> PaneId {
    PaneId(FIRST_PANE_ID.to_owned())
}

pub fn second_pane() -> PaneId {
    PaneId(SECOND_PANE_ID.to_owned())
}

/// One call made through the `Terminal` trait. Panes are the pane ids.
#[derive(Clone, Debug, PartialEq)]
pub enum Call {
    CreateTab {
        label: String,
        cwd: PathBuf,
    },
    SplitPane {
        pane: String,
        cwd: PathBuf,
    },
    Run {
        pane: String,
        text: String,
    },
    SendText {
        pane: String,
        text: String,
    },
    SendKeys {
        pane: String,
        keys: Vec<String>,
    },
    Read {
        pane: String,
        source: ReadSource,
        lines: Option<u32>,
    },
    Foreground {
        pane: String,
    },
    PaneGet {
        pane: String,
    },
    TabExists {
        tab: String,
    },
    CloseTab {
        tab: String,
    },
    Version,
}

impl Call {
    /// The call name used by `fail` entries and [`FakeTerminal::panic_on_nth`].
    pub fn kind(&self) -> &'static str {
        match self {
            Call::CreateTab { .. } => "tab create",
            Call::SplitPane { .. } => "pane split",
            Call::Run { .. } => "pane run",
            Call::SendText { .. } => "pane send-text",
            Call::SendKeys { .. } => "pane send-keys",
            Call::Read { .. } => "pane read",
            Call::Foreground { .. } => "pane process-info",
            Call::PaneGet { .. } => "pane get",
            Call::TabExists { .. } => "tab list",
            Call::CloseTab { .. } => "tab close",
            Call::Version => "version",
        }
    }
}

/// A call with the (manual) clock time at which it was made.
#[derive(Clone, Debug, PartialEq)]
pub struct Recorded {
    pub at: Duration,
    pub call: Call,
}

type PanicPredicate = Box<dyn Fn(&Call) -> bool + Send>;

struct Inner {
    machine: Machine,
    calls: Vec<Recorded>,
    tab_open: bool,
    keep_tab: bool,
    panic_when: Option<PanicPredicate>,
}

/// The screen script machine behind `smoke::Terminal`, in process.
pub struct FakeTerminal<'a> {
    clock: &'a dyn Clock,
    inner: Mutex<Inner>,
}

fn error_message(code: &str) -> String {
    match code {
        "invalid_key" => "unsupported key".to_owned(),
        "tab_not_found" => "tab not found".to_owned(),
        "pane_not_found" => "pane not found".to_owned(),
        other => other.to_owned(),
    }
}

fn herdr_error(code: &str, message: Option<String>) -> TermError {
    TermError::Herdr {
        code: code.to_owned(),
        message: message.unwrap_or_else(|| error_message(code)),
    }
}

fn key_is_known(key: &str) -> bool {
    const NAMES: &[&str] = &[
        "enter",
        "esc",
        "escape",
        "tab",
        "space",
        "backspace",
        "delete",
        "up",
        "down",
        "left",
        "right",
        "home",
        "end",
        "pageup",
        "pagedown",
    ];
    if NAMES.contains(&key) || key.chars().count() == 1 {
        return true;
    }
    if let Some((modifier, rest)) = key.split_once('+') {
        return matches!(modifier, "ctrl" | "alt" | "shift") && key_is_known(rest);
    }
    key.strip_prefix('f')
        .and_then(|n| n.parse::<u8>().ok())
        .is_some_and(|n| (1..=12).contains(&n))
}

impl<'a> FakeTerminal<'a> {
    /// A terminal that plays `script`, stamping calls with the time of `clock`.
    pub fn new(script: Script, clock: &'a dyn Clock) -> Self {
        FakeTerminal {
            clock,
            inner: Mutex::new(Inner {
                machine: Machine::new(script, true),
                calls: Vec::new(),
                tab_open: false,
                keep_tab: false,
                panic_when: None,
            }),
        }
    }

    pub fn from_builder(script: &ScriptBuilder, clock: &'a dyn Clock) -> Self {
        Self::new(script.build(), clock)
    }

    /// Feeds pasted text to the machine as a `paste` input on the named pane; the fake paster
    /// shares the machine through this. Returns whether a rule fired.
    pub fn paste_into(&self, pane: &str, text: &str) -> bool {
        self.lock().machine.typed(pane, Input::Paste, text)
    }

    /// Every typed text that fired a rule on the named pane, oldest first.
    pub fn typed_texts(&self, pane: &str) -> Vec<String> {
        self.lock().machine.typed_texts(pane)
    }

    /// Pushes a value to the machine's remembered list.
    pub fn note_remembered(&self, value: String) {
        self.lock().machine.note(value);
    }

    /// Panics (with [`PANIC_MESSAGE`]) on the first call for which the predicate holds. The
    /// panic happens after the call is recorded.
    pub fn panic_when(self, predicate: impl Fn(&Call) -> bool + Send + 'static) -> Self {
        self.lock().panic_when = Some(Box::new(predicate));
        self
    }

    /// Panics on the `nth` (1 is the first) call of this kind, for example `pane read`.
    pub fn panic_on_nth(self, kind: &'static str, nth: usize) -> Self {
        let seen = std::sync::atomic::AtomicUsize::new(0);
        self.panic_when(move |call| {
            call.kind() == kind && seen.fetch_add(1, std::sync::atomic::Ordering::SeqCst) + 1 == nth
        })
    }

    /// `close_tab` succeeds but the tab stays: `tab_exists` keeps answering true.
    pub fn keep_tab_after_close(self) -> Self {
        self.lock().keep_tab = true;
        self
    }

    fn lock(&self) -> MutexGuard<'_, Inner> {
        self.inner.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Records the call, panics if planned, and returns the locked state.
    fn begin(&self, call: Call, pane: Option<&str>) -> Result<MutexGuard<'_, Inner>, TermError> {
        let mut inner = self.lock();
        let planned = inner.panic_when.as_ref().is_some_and(|p| p(&call));
        let name = pane.and_then(script::pane_name);
        let kind = call.kind();
        inner.calls.push(Recorded {
            at: self.clock.now(),
            call,
        });
        if planned {
            drop(inner);
            panic!("{PANIC_MESSAGE}");
        }
        if let Some(Failure { code, message }) = inner.machine.call(kind, name) {
            return Err(herdr_error(&code, message));
        }
        Ok(inner)
    }

    /// The pane name for an id, which must exist.
    fn pane_of(inner: &Inner, pane: &PaneId) -> Result<&'static str, TermError> {
        match script::pane_name(&pane.0) {
            Some(name) if inner.machine.is_open(name) => Ok(name),
            _ => Err(herdr_error(
                "pane_not_found",
                Some(format!("pane {} not found", pane.0)),
            )),
        }
    }

    // ---- what the tests read back ----

    pub fn calls(&self) -> Vec<Recorded> {
        self.lock().calls.clone()
    }

    /// Calls of one kind (`pane run`, `tab close`, ...), in order.
    pub fn calls_of(&self, kind: &str) -> Vec<Recorded> {
        self.calls()
            .into_iter()
            .filter(|r| r.call.kind() == kind)
            .collect()
    }

    /// Every `pane run` text with its time and pane id.
    pub fn runs(&self) -> Vec<(Duration, String, String)> {
        self.calls()
            .into_iter()
            .filter_map(|r| match r.call {
                Call::Run { pane, text } => Some((r.at, pane, text)),
                _ => None,
            })
            .collect()
    }

    /// Every `pane send-text` text with its time and pane id.
    pub fn text_sent(&self) -> Vec<(Duration, String, String)> {
        self.calls()
            .into_iter()
            .filter_map(|r| match r.call {
                Call::SendText { pane, text } => Some((r.at, pane, text)),
                _ => None,
            })
            .collect()
    }

    /// Every `pane send-keys` call with its time, pane id and keys.
    pub fn keys_sent(&self) -> Vec<(Duration, String, Vec<String>)> {
        self.calls()
            .into_iter()
            .filter_map(|r| match r.call {
                Call::SendKeys { pane, keys } => Some((r.at, pane, keys)),
                _ => None,
            })
            .collect()
    }

    /// All text and key names sent to any pane, in order (run, send-text and send-keys).
    pub fn everything_typed(&self) -> Vec<String> {
        self.calls()
            .into_iter()
            .flat_map(|r| match r.call {
                Call::Run { text, .. } | Call::SendText { text, .. } => vec![text],
                Call::SendKeys { keys, .. } => keys,
                _ => Vec::new(),
            })
            .collect()
    }

    /// How many times `close_tab` was called.
    pub fn close_count(&self) -> usize {
        self.calls_of("tab close").len()
    }

    /// The values remembered by rules so far (the tokens that were answered).
    pub fn memory(&self) -> Vec<String> {
        self.lock().machine.memory().to_vec()
    }

    /// The pane's full text (what `recent` returns).
    pub fn pane_text(&self, pane: &PaneId) -> String {
        let inner = self.lock();
        script::pane_name(&pane.0)
            .map(|n| inner.machine.recent(n, None))
            .unwrap_or_default()
    }
}

impl Terminal for FakeTerminal<'_> {
    fn create_tab(&self, label: &str, cwd: &Path) -> Result<NewTab, TermError> {
        let call = Call::CreateTab {
            label: label.to_owned(),
            cwd: cwd.to_path_buf(),
        };
        let mut inner = self.begin(call, None)?;
        inner.machine.open_pane(FIRST, cwd);
        inner.tab_open = true;
        Ok(NewTab {
            tab: tab_id(),
            pane: first_pane(),
        })
    }

    fn split_pane(&self, pane: &PaneId, cwd: &Path) -> Result<PaneId, TermError> {
        let call = Call::SplitPane {
            pane: pane.0.clone(),
            cwd: cwd.to_path_buf(),
        };
        let mut inner = self.begin(call, Some(&pane.0))?;
        Self::pane_of(&inner, pane)?;
        inner.machine.open_pane(SECOND, cwd);
        Ok(second_pane())
    }

    fn run(&self, pane: &PaneId, text: &str) -> Result<(), TermError> {
        let call = Call::Run {
            pane: pane.0.clone(),
            text: text.to_owned(),
        };
        let mut inner = self.begin(call, Some(&pane.0))?;
        let name = Self::pane_of(&inner, pane)?;
        inner.machine.typed(name, Input::Run, text);
        Ok(())
    }

    fn send_text(&self, pane: &PaneId, text: &str) -> Result<(), TermError> {
        let call = Call::SendText {
            pane: pane.0.clone(),
            text: text.to_owned(),
        };
        let mut inner = self.begin(call, Some(&pane.0))?;
        let name = Self::pane_of(&inner, pane)?;
        inner.machine.typed(name, Input::SendText, text);
        Ok(())
    }

    fn send_keys(&self, pane: &PaneId, keys: &[&str]) -> Result<(), TermError> {
        let call = Call::SendKeys {
            pane: pane.0.clone(),
            keys: keys.iter().map(|k| (*k).to_owned()).collect(),
        };
        let mut inner = self.begin(call, Some(&pane.0))?;
        let name = Self::pane_of(&inner, pane)?;
        if let Some(bad) = keys.iter().find(|k| !key_is_known(k)) {
            return Err(herdr_error(
                "invalid_key",
                Some(format!("unsupported key {bad}")),
            ));
        }
        for key in keys {
            inner.machine.typed(name, Input::SendKeys, key);
        }
        Ok(())
    }

    fn read(
        &self,
        pane: &PaneId,
        source: ReadSource,
        lines: Option<u32>,
    ) -> Result<String, TermError> {
        let call = Call::Read {
            pane: pane.0.clone(),
            source,
            lines,
        };
        let mut inner = self.begin(call, Some(&pane.0))?;
        let name = Self::pane_of(&inner, pane)?;
        inner.machine.tick_read(name);
        let lines = lines.map(|n| n as usize);
        Ok(match source {
            ReadSource::Visible => inner.machine.visible(name, lines),
            ReadSource::Recent | ReadSource::RecentUnwrapped => inner.machine.recent(name, lines),
        })
    }

    fn foreground(&self, pane: &PaneId) -> Result<Vec<String>, TermError> {
        let call = Call::Foreground {
            pane: pane.0.clone(),
        };
        let inner = self.begin(call, Some(&pane.0))?;
        let name = Self::pane_of(&inner, pane)?;
        Ok(inner.machine.foreground(name))
    }

    fn pane_get(&self, pane: &PaneId) -> Result<String, TermError> {
        let call = Call::PaneGet {
            pane: pane.0.clone(),
        };
        let inner = self.begin(call, Some(&pane.0))?;
        let name = Self::pane_of(&inner, pane)?;
        Ok(inner.machine.pane_get_text(name))
    }

    fn tab_exists(&self, tab: &TabId) -> Result<bool, TermError> {
        let call = Call::TabExists { tab: tab.0.clone() };
        let inner = self.begin(call, None)?;
        Ok(inner.tab_open)
    }

    fn close_tab(&self, tab: &TabId) -> Result<(), TermError> {
        let call = Call::CloseTab { tab: tab.0.clone() };
        let mut inner = self.begin(call, None)?;
        if tab.0 != TAB_ID || !inner.tab_open {
            return Err(herdr_error(
                "tab_not_found",
                Some(format!("tab {} not found", tab.0)),
            ));
        }
        if !inner.keep_tab {
            inner.tab_open = false;
            inner.machine.close_all();
        }
        Ok(())
    }

    fn version(&self) -> String {
        let call = Call::Version;
        let mut inner = self.lock();
        inner.calls.push(Recorded {
            at: self.clock.now(),
            call,
        });
        include_str!("../fixtures/herdr/version.txt")
            .trim()
            .to_owned()
    }
}

// ---------------------------------------------------------------------------------------------
// The fake herdr executable and the smoke binary
// ---------------------------------------------------------------------------------------------

/// The built `fake-herdr` executable.
pub fn fake_herdr_exe() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_fake-herdr"))
}

/// The built `smoke` executable.
pub fn smoke_exe() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_smoke"))
}

/// Single quotes for a POSIX shell.
fn shell_quote(text: &str) -> String {
    format!("'{}'", text.replace('\'', "'\\''"))
}

/// Writes an executable file.
pub fn write_executable(path: &Path, body: &str) {
    fs::write(path, body).expect("write executable");
    let mut permissions = fs::metadata(path).expect("stat executable").permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(path, permissions).expect("chmod executable");
}

/// A finished or captured process.
#[derive(Clone, Debug)]
pub struct Outcome {
    /// None when a signal ended the process.
    pub code: Option<i32>,
    pub stdout: String,
    pub stderr: String,
}

impl Outcome {
    fn from_output(output: std::process::Output) -> Self {
        Outcome {
            code: output.status.code(),
            stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
        }
    }

    pub fn lines(&self) -> Vec<&str> {
        self.stdout.lines().collect()
    }

    pub fn last_line(&self) -> Option<&str> {
        self.stdout.lines().last()
    }

    /// The line that starts with `SMOKE_RESULT:`, if any.
    pub fn result_line(&self) -> Option<&str> {
        self.stdout.lines().find(|l| l.starts_with("SMOKE_RESULT:"))
    }

    pub fn has_line_starting(&self, prefix: &str) -> bool {
        self.stdout.lines().any(|l| l.starts_with(prefix))
    }
}

/// What the fake wrote at the last `pane run`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CwdAtRun {
    /// Octal permission bits of the tab's working folder, for example `700`.
    pub mode: String,
    /// Every entry under it, relative and sorted; a folder ends with a slash.
    pub entries: Vec<String>,
}

impl CwdAtRun {
    /// The entries that are files.
    pub fn files(&self) -> Vec<&str> {
        self.entries
            .iter()
            .map(String::as_str)
            .filter(|e| !e.ends_with('/'))
            .collect()
    }
}

/// A `fake-herdr` with its own state folder. [`FakeHerdr::executable`] is a small wrapper script
/// that sets `FAKE_HERDR_DIR`, so it can be handed to `HerdrCli` or `smoke --herdr` as is.
pub struct FakeHerdr {
    dir: tempfile::TempDir,
    wrapper: PathBuf,
}

impl FakeHerdr {
    pub fn new(script: &ScriptBuilder) -> Self {
        Self::from_json(&script.to_json())
    }

    pub fn from_scenario(scenario: &Scenario) -> Self {
        Self::new(&scenario.script())
    }

    /// Two plain shell panes and no rules.
    pub fn plain() -> Self {
        Self::new(&ScriptBuilder::new())
    }

    pub fn from_json(json: &str) -> Self {
        let dir = tempfile::tempdir().expect("temporary folder for the fake");
        fs::write(dir.path().join("script.json"), json).expect("write script.json");
        let wrapper = dir.path().join("herdr");
        let body = format!(
            "#!/bin/sh\nFAKE_HERDR_DIR={} exec {} \"$@\"\n",
            shell_quote(&dir.path().to_string_lossy()),
            shell_quote(&fake_herdr_exe().to_string_lossy()),
        );
        write_executable(&wrapper, &body);
        FakeHerdr { dir, wrapper }
    }

    /// The state folder (`log`, `events`, `script.json`, `cwd_at_run.txt`).
    pub fn dir(&self) -> &Path {
        self.dir.path()
    }

    /// An executable that behaves as `herdr` against this fake.
    pub fn executable(&self) -> &Path {
        &self.wrapper
    }

    /// Runs the fake once with these arguments.
    pub fn call(&self, args: &[&str]) -> Outcome {
        let output = Command::new(&self.wrapper)
            .args(args)
            .env_clear()
            .output()
            .expect("run the fake herdr");
        Outcome::from_output(output)
    }

    /// Every call so far, as argument lists, in order.
    pub fn log(&self) -> Vec<Vec<String>> {
        let Ok(text) = fs::read_to_string(self.dir.path().join("log")) else {
            return Vec::new();
        };
        text.lines()
            .filter_map(|l| serde_json::from_str::<Vec<String>>(l).ok())
            .collect()
    }

    /// Calls whose first arguments equal `prefix`, for example `["pane", "run"]`.
    pub fn calls_starting(&self, prefix: &[&str]) -> Vec<Vec<String>> {
        self.log()
            .into_iter()
            .filter(|args| {
                args.len() >= prefix.len() && args.iter().zip(prefix).all(|(a, p)| a == p)
            })
            .collect()
    }

    /// True once a call with these first arguments is logged.
    pub fn saw(&self, prefix: &[&str]) -> bool {
        !self.calls_starting(prefix).is_empty()
    }

    /// Waits (real time) until a call with these first arguments is logged.
    pub fn wait_for_call(&self, prefix: &[&str], timeout: Duration) -> bool {
        let deadline = Instant::now() + timeout;
        loop {
            if self.saw(prefix) {
                return true;
            }
            if Instant::now() >= deadline {
                return false;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
    }

    pub fn cwd_at_run(&self) -> Option<CwdAtRun> {
        let text = fs::read_to_string(self.dir.path().join("cwd_at_run.txt")).ok()?;
        let mut lines = text.lines();
        let mode = lines.next()?.to_owned();
        Some(CwdAtRun {
            mode,
            entries: lines.map(str::to_owned).collect(),
        })
    }
}

/// A fake herdr plus a private environment for the `smoke` binary: its own temporary folder
/// (`TMPDIR`, where the scratch project appears), home folder, a generated login name and an
/// executable stand-in for `talaria`.
pub struct Harness {
    pub fake: FakeHerdr,
    root: tempfile::TempDir,
    user: String,
    stub: PathBuf,
}

impl Harness {
    pub fn new(script: &ScriptBuilder) -> Self {
        Self::with_fake(FakeHerdr::new(script))
    }

    pub fn from_scenario(scenario: &Scenario) -> Self {
        Self::with_fake(FakeHerdr::from_scenario(scenario))
    }

    /// Plain shell panes: nothing ever starts.
    pub fn plain() -> Self {
        Self::with_fake(FakeHerdr::plain())
    }

    fn with_fake(fake: FakeHerdr) -> Self {
        let root = tempfile::tempdir().expect("temporary folder for the harness");
        for sub in ["tmp", "home", "bin"] {
            fs::create_dir(root.path().join(sub)).expect("create harness folder");
        }
        let stub = root.path().join("bin").join("talaria");
        write_executable(&stub, "#!/bin/sh\nexit 0\n");
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        Harness {
            fake,
            root,
            user: format!("smk{:08x}", nanos as u32),
            stub,
        }
    }

    /// The folder the smoke process starts in (it holds `tmp`, `home` and `bin/talaria`).
    pub fn root(&self) -> &Path {
        self.root.path()
    }

    /// The stand-in `talaria` (an executable that exits 0 at once).
    pub fn talaria(&self) -> &Path {
        &self.stub
    }

    /// `TMPDIR` of the smoke process; the scratch project is created here.
    pub fn tmp_dir(&self) -> PathBuf {
        self.root.path().join("tmp")
    }

    pub fn home_dir(&self) -> PathBuf {
        self.root.path().join("home")
    }

    /// The generated login name given to the smoke process (`USER` and `LOGNAME`).
    pub fn user(&self) -> &str {
        &self.user
    }

    /// The scratch projects (`talaria-smoke-*`) that exist in `TMPDIR` now.
    pub fn scratch_dirs(&self) -> Vec<PathBuf> {
        let Ok(read) = fs::read_dir(self.tmp_dir()) else {
            return Vec::new();
        };
        read.flatten()
            .map(|e| e.path())
            .filter(|p| {
                p.file_name()
                    .is_some_and(|n| n.to_string_lossy().starts_with("talaria-smoke-"))
            })
            .collect()
    }

    /// `smoke run --herdr <fake> --talaria <stand-in>` inside a Herdr pane.
    pub fn cmd(&self) -> SmokeCmd {
        self.cmd_without_talaria().arg("--talaria").arg(&self.stub)
    }

    /// `smoke run --herdr <fake>`: for tests that pass `--talaria` or set the variable themselves.
    pub fn cmd_without_talaria(&self) -> SmokeCmd {
        self.bare_cmd()
            .arg("run")
            .arg("--herdr")
            .arg(self.fake.executable())
    }

    /// `smoke` with the private environment and no arguments at all.
    pub fn bare_cmd(&self) -> SmokeCmd {
        let mut cmd = Command::new(smoke_exe());
        cmd.env_clear()
            .env("PATH", "/usr/bin:/bin")
            .env("HOME", self.home_dir())
            .env("USER", &self.user)
            .env("LOGNAME", &self.user)
            .env("TMPDIR", self.tmp_dir())
            .env("HERDR_ENV", "1")
            .env("HERDR_WORKSPACE_ID", WORKSPACE_ID)
            .current_dir(self.root.path());
        SmokeCmd { cmd }
    }
}

/// The command that starts the built `smoke` binary. The environment starts empty (nothing is
/// inherited from the pane the tests run in); [`Harness`] sets `PATH`, `HOME`, `USER`,
/// `LOGNAME`, `TMPDIR`, `HERDR_ENV=1` and `HERDR_WORKSPACE_ID=w1`.
pub struct SmokeCmd {
    cmd: Command,
}

impl SmokeCmd {
    pub fn arg(mut self, arg: impl AsRef<std::ffi::OsStr>) -> Self {
        self.cmd.arg(arg);
        self
    }

    pub fn args<I, S>(mut self, args: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: AsRef<std::ffi::OsStr>,
    {
        self.cmd.args(args);
        self
    }

    pub fn env(mut self, key: &str, value: impl AsRef<std::ffi::OsStr>) -> Self {
        self.cmd.env(key, value);
        self
    }

    pub fn env_remove(mut self, key: &str) -> Self {
        self.cmd.env_remove(key);
        self
    }

    pub fn current_dir(mut self, dir: &Path) -> Self {
        self.cmd.current_dir(dir);
        self
    }

    /// Outside a Herdr pane: neither `HERDR_ENV` nor `HERDR_WORKSPACE_ID` is set.
    pub fn outside_herdr(self) -> Self {
        self.env_remove("HERDR_ENV")
            .env_remove("HERDR_WORKSPACE_ID")
    }

    pub fn user(self, name: &str) -> Self {
        self.env("USER", name).env("LOGNAME", name)
    }

    /// Neither `USER` nor `LOGNAME`.
    pub fn no_user(self) -> Self {
        self.env_remove("USER").env_remove("LOGNAME")
    }

    pub fn live_require(self) -> Self {
        self.env("TALARIA_LIVE_REQUIRE", "1")
    }

    pub fn step_scale(self, scale: f32) -> Self {
        self.arg("--step-scale").arg(scale.to_string())
    }

    pub fn deadline_secs(self, secs: u64) -> Self {
        self.arg("--deadline-secs").arg(secs.to_string())
    }

    /// Runs to the end and captures the output.
    pub fn run(mut self) -> Outcome {
        let output = self
            .cmd
            .stdin(Stdio::null())
            .output()
            .expect("run the smoke binary");
        Outcome::from_output(output)
    }

    /// Starts the process; use [`Running::signal`] and [`Running::finish`].
    pub fn spawn(mut self) -> Running {
        let child = self
            .cmd
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("start the smoke binary");
        Running { child }
    }
}

/// A started `smoke` process.
pub struct Running {
    child: Child,
}

impl Running {
    pub fn pid(&self) -> u32 {
        self.child.id()
    }

    /// Sends a signal with the `kill` command: `INT` or `TERM`.
    pub fn signal(&self, name: &str) {
        let status = Command::new("kill")
            .arg(format!("-{name}"))
            .arg(self.child.id().to_string())
            .status()
            .expect("run kill");
        assert!(status.success(), "kill -{name} failed");
    }

    /// Kills the process (for a test that gave up waiting).
    pub fn kill(&mut self) {
        let _ = self.child.kill();
    }

    /// Waits for the process to end and returns what it printed.
    pub fn finish(mut self) -> Outcome {
        let mut stdout = String::new();
        let mut stderr = String::new();
        if let Some(mut pipe) = self.child.stdout.take() {
            let _ = pipe.read_to_string(&mut stdout);
        }
        if let Some(mut pipe) = self.child.stderr.take() {
            let _ = pipe.read_to_string(&mut stderr);
        }
        let status = self.child.wait().expect("wait for the smoke binary");
        Outcome {
            code: status.code(),
            stdout,
            stderr,
        }
    }
}
