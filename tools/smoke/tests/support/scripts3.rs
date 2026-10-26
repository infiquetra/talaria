//! Screen scripts for the phase 3 suite.
//!
//! A shell line is a `run` rule whose `matches` names the output file. The rule writes that
//! file with `write_in_scratch` and appends `SMOKE_EXIT:<n>`. The run itself panics until
//! `run_smoke_3` is written, so every step test fails on that panic.

use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::time::Duration;

use smoke::{ManualClock, SmokeConfig, SmokeReport, StepOutcome, Suite, run_smoke_3};

use super::{Call, FakeTerminal, RuleBuilder, ScriptBuilder, StageBuilder, write_executable};

const PROMPT_PATTERN: &str = concat!(
    r"join these two words with a hyphen and reply with only the result: ",
    r"(\w+) and (\d+)$"
);
const OK_PROMPT: &str = "Reply with only the word ok.";
const PROBE_PROMPT: &str = "Run this exact shell command: rm -rf ./talaria-smoke-probe";
const ID1: &str = "local-a1b2c3d4e5f67890";
const ID2: &str = "local-b2c3d4e5f6789012";
const ID3: &str = "local-c3d4e5f678901234";
const LAUNCHED: &str = concat!(
    "{\"status\":\"launched\",\"session_id\":\"",
    "local-a1b2c3d4e5f67890\"}\n"
);
const BUSY_FLEET: &str = include_str!("../fixtures/screens/phase3/fleet-quiet-busy.txt");
const QUIET_FLEET: &str = include_str!("../fixtures/screens/phase3/fleet-quiet-empty.txt");

/// How far the script answers. Later steps are left as a plain shell.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Step {
    FleetQuiet,
    BgLaunch,
    WorktreeLaunch,
    FleetList,
    Attach,
    Startup,
    PlainAnswer,
    LocalCommands,
    FleetView,
    OpenInPane,
    Quit,
    NeedsYou,
}

/// What `fleet-quiet` prints.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum FleetQuiet {
    #[default]
    Pass,
    Exit(i32),
    Narrow,
    Busy,
}

/// What `bg-launch` prints.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum BgLaunch {
    #[default]
    Pass,
    Exit(i32),
    Output,
    Slow,
    RefusalExit(i32),
    RefusalText,
}

/// What `worktree-launch` prints and which files it leaves.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum WorktreeLaunch {
    #[default]
    Pass,
    Exit(i32),
    Text,
    TreeMissing,
    NotFresh,
    CheckoutDirty,
}

/// What `fleet-list` prints.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum FleetList {
    #[default]
    Pass,
    Exit(i32),
    NeverDone,
    WorktreeMissing,
    Unserved,
    NoGateway,
    NoGatewayStarted,
}

/// What `attach` shows.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Attach {
    #[default]
    Pass,
    NoResumed,
    Ambiguous,
    Exit,
    NoMatch,
}

/// What the local commands show.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum LocalCommands {
    #[default]
    Pass,
    Tasks,
    AgentsOption,
    Processes,
    BgUsage,
}

/// What the fleet view shows.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum FleetView {
    #[default]
    Pass,
    Open,
    NoMatch,
    Close,
}

/// What open-in-pane shows.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum OpenInPane {
    #[default]
    Pass,
    Notice,
    NotInHerdr,
}

/// What `needs-you` shows. `RetryThenFail` answers the first poll with a miss and the
/// second miss too. `SkipUnasked` is the miss whose command already ran.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum NeedsYou {
    #[default]
    Pass,
    Launch,
    NeverListed,
    TooOld,
    Notice,
    Segment,
    Peek,
    NoDeny,
    NotCleared,
    CommandRan,
    SegmentStays,
    SkipUnasked,
    RetryThenFail,
}

/// One knob per step. The default is the passing run.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Shapes {
    pub fleet_quiet: FleetQuiet,
    pub bg_launch: BgLaunch,
    pub worktree_launch: WorktreeLaunch,
    pub fleet_list: FleetList,
    pub attach: Attach,
    pub local_commands: LocalCommands,
    pub fleet_view: FleetView,
    pub open_in_pane: OpenInPane,
    pub needs_you: NeedsYou,
    /// `/fleet` and `/tasks` leave the screen unchanged, so a submit fallback would show.
    pub still_after_slash: bool,
}

/// The script for a run that stops answering after `stop`.
pub fn script_3(shapes: &Shapes, stop: Step) -> ScriptBuilder {
    let mut script = ScriptBuilder::new();
    let mut rules = vec![fleet_quiet_rule(shapes.fleet_quiet)];
    if stop >= Step::BgLaunch {
        rules.extend(bg_launch_rules(shapes.bg_launch));
    }
    if stop >= Step::WorktreeLaunch {
        rules.extend(worktree_rules(shapes.worktree_launch));
    }
    if stop >= Step::FleetList {
        rules.extend(fleet_list_rules(shapes.fleet_list));
    }
    if stop >= Step::Attach {
        rules.extend(attach_rules(shapes.attach));
    }
    if stop >= Step::Startup {
        rules.push(startup_rule());
    }
    if stop >= Step::PlainAnswer {
        rules.push(plain_answer_rule());
    }
    if stop >= Step::LocalCommands {
        rules.extend(local_rules(shapes));
    }
    if stop >= Step::FleetView {
        rules.extend(fleet_view_rules(shapes));
    }
    if stop >= Step::OpenInPane {
        rules.extend(open_rules(shapes.open_in_pane));
    }
    if stop >= Step::Quit {
        rules.push(quit_rule(0));
    }
    if stop >= Step::NeedsYou {
        rules.extend(needs_you_rules(shapes.needs_you));
    }
    for rule in rules {
        script = script.on_first(rule);
    }
    script
}

fn exit_marker(code: i32) -> String {
    format!("SMOKE_EXIT:{code}\n")
}

fn fleet_quiet_rule(shape: FleetQuiet) -> RuleBuilder {
    let (size, listing, code) = match shape {
        FleetQuiet::Pass => ("100 24\n", QUIET_FLEET, 0),
        FleetQuiet::Exit(code) => ("100 24\n", QUIET_FLEET, code),
        FleetQuiet::Narrow => ("80 20\n", QUIET_FLEET, 0),
        FleetQuiet::Busy => ("100 24\n", BUSY_FLEET, 0),
    };
    RuleBuilder::run("size\\.out")
        .write_in_scratch("size.out", size)
        .then(StageBuilder::new().write_in_scratch("q.out", listing))
        .append(&exit_marker(code))
}

fn bg_launch_rules(shape: BgLaunch) -> Vec<RuleBuilder> {
    let (body, code, slow) = match shape {
        BgLaunch::Pass => (LAUNCHED, 0, false),
        BgLaunch::Exit(code) => (LAUNCHED, code, false),
        BgLaunch::Output => ("not a launch line\n", 0, false),
        BgLaunch::Slow => (LAUNCHED, 0, true),
        BgLaunch::RefusalExit(_) | BgLaunch::RefusalText => (LAUNCHED, 0, false),
    };
    let mut launch = RuleBuilder::run("bg1\\.out")
        .write_in_scratch("bg1.out", body)
        .then(StageBuilder::new().write_in_scratch("bg1.err", ""));
    if !slow {
        launch = launch.append(&exit_marker(code));
    }
    let (err, out, refusal_code) = match shape {
        BgLaunch::RefusalExit(code) => ("talaria --bg cannot be combined with -p.\n", "", code),
        BgLaunch::RefusalText => ("", "talaria --bg cannot be combined with -p.\n", 2),
        _ => ("talaria --bg cannot be combined with -p.\n", "", 2),
    };
    let refusal = RuleBuilder::run("bg2\\.err")
        .write_in_scratch("bg2.err", err)
        .then(StageBuilder::new().write_in_scratch("bg2.out", out))
        .append(&exit_marker(refusal_code));
    vec![launch, refusal]
}

fn worktree_rules(shape: WorktreeLaunch) -> Vec<RuleBuilder> {
    let code = match shape {
        WorktreeLaunch::Exit(code) => code,
        _ => 0,
    };
    let receipt = match shape {
        WorktreeLaunch::Text => "launched without the receipt\n",
        _ => concat!(
            "Worktree: branch talaria/$1 from origin/main at abcdef123456\n",
            "Launched session local-f678901234567890 in profile fixture.\n"
        ),
    };
    let mut launch = RuleBuilder::run(r"--worktree --name (smoke-[0-9a-z]+)")
        .write_in_scratch("wt.out", receipt);
    if shape != WorktreeLaunch::TreeMissing {
        launch =
            launch.then(StageBuilder::new().write_in_scratch("a/.worktrees/talaria-$1/KEEP", ""));
    }
    launch = launch.append(&exit_marker(code));
    let subject = match shape {
        WorktreeLaunch::NotFresh => "init\n",
        _ => "two\n",
    };
    let status = match shape {
        WorktreeLaunch::CheckoutDirty => " M README\n",
        _ => "",
    };
    let inspect = RuleBuilder::run("wt\\.st")
        .write_in_scratch("wt.id", &format!("{ID2}\n"))
        .then(StageBuilder::new().write_in_scratch("wt.log", subject))
        .then(StageBuilder::new().write_in_scratch("wt.st", status))
        .append(&exit_marker(0));
    vec![launch, inspect]
}

fn rows_json(id1_state: &str, include_id2: bool) -> String {
    let second = if include_id2 {
        format!(",{{\"session_id\":\"{ID2}\",\"state\":\"idle\"}}")
    } else {
        String::new()
    };
    format!(
        "{{\"rows\":[{{\"session_id\":\"{ID1}\",\"state\":\"{id1_state}\"}}{second}],\"needs_you\":[]}}\n"
    )
}

fn fleet_list_rules(shape: FleetList) -> Vec<RuleBuilder> {
    let (state, include_id2, code) = match shape {
        FleetList::Pass
        | FleetList::Unserved
        | FleetList::NoGateway
        | FleetList::NoGatewayStarted => ("done", true, 0),
        FleetList::Exit(code) => ("done", true, code),
        FleetList::NeverDone => ("working", true, 0),
        FleetList::WorktreeMissing => ("done", false, 0),
    };
    let list = RuleBuilder::run("f\\.json")
        .repeat()
        .write_in_scratch("f.json", &rows_json(state, include_id2))
        .append(&exit_marker(code));
    let unserved_code = if shape == FleetList::Unserved { 0 } else { 2 };
    let unserved_text = if shape == FleetList::Unserved {
        "profile missing\n"
    } else {
        "profile \"nosuch\" is not served by this gateway\n"
    };
    let unserved = RuleBuilder::run("f2\\.out")
        .write_in_scratch("f2.out", unserved_text)
        .append(&exit_marker(unserved_code));
    let (home_code, home_text, wrote) = match shape {
        FleetList::NoGateway => (0, "profile fixture\n", false),
        FleetList::NoGatewayStarted => (
            1,
            "no Hermes gateway is running for this Hermes home\n",
            true,
        ),
        _ => (
            1,
            "no Hermes gateway is running for this Hermes home\n",
            false,
        ),
    };
    let mut home = RuleBuilder::run("f3\\.out")
        .write_in_scratch("f3.out", home_text)
        .append(&exit_marker(home_code));
    if wrote {
        home = home.then(StageBuilder::new().write_in_scratch("empty-home/started", "x\n"));
    }
    vec![list, unserved, home]
}

fn attach_rules(shape: Attach) -> Vec<RuleBuilder> {
    let resumed = match shape {
        Attach::NoResumed => "$0\n",
        Attach::Ambiguous => concat!(
            "talaria: 2 sessions match \"local-a1b2c3d4\":\n",
            "<title>  local-a1b2c3d4e5f67890  fixture\n",
            "<title>  local-a1b2c3d4f6789012  fixture\n",
            "Next step: use more of the id, or add --profile fixture.\n",
            "SMOKE_EXIT:2\n"
        ),
        _ => include_str!("../fixtures/screens/phase3/attach-resumed.screen.txt"),
    };
    let attach = RuleBuilder::run("attach --no-ensure").append(resumed);
    let quit_code = if shape == Attach::Exit { 7 } else { 0 };
    let typed_quit = RuleBuilder::send_text("^/quit$")
        .repeat()
        .append("you: /quit");
    let enter = RuleBuilder::send_keys("^enter$").append(&exit_marker(quit_code));
    let no_match_body = if shape == Attach::NoMatch {
        "talaria: something else\nSMOKE_EXIT:0\n"
    } else {
        concat!(
            "talaria: no session matches \"zz-none-abcd1234\"\n",
            "SMOKE_EXIT:1\n"
        )
    };
    let missing = RuleBuilder::run("zz-none-").append(no_match_body);
    vec![attach, typed_quit, enter, missing]
}

fn startup_rule() -> RuleBuilder {
    // The interactive launch is the one whose provider flag ends the command.
    // A `--bg` line keeps a prompt after the provider, so it does not match.
    RuleBuilder::run(r"--provider ollama-cloud;")
        .foreground(&["-bash", "$0"])
        .append("$0\nTalaria\nConnecting to Hermes")
}

fn plain_answer_rule() -> RuleBuilder {
    RuleBuilder::run(PROMPT_PATTERN)
        .remember("$1-$2")
        .churn(4)
        .then(StageBuilder::new().after_polls(4).append("agent: $1-$2"))
}

fn local_rules(shapes: &Shapes) -> Vec<RuleBuilder> {
    let tasks = if shapes.still_after_slash || shapes.local_commands == LocalCommands::Tasks {
        "$0\n"
    } else {
        include_str!("../fixtures/screens/phase3/local-tasks.screen.txt")
    };
    let agents = if shapes.local_commands == LocalCommands::AgentsOption {
        "unknown option\n"
    } else {
        include_str!("../fixtures/screens/phase3/local-agents-option.screen.txt")
    };
    let processes = if shapes.local_commands == LocalCommands::Processes {
        "$0\n"
    } else {
        include_str!("../fixtures/screens/phase3/local-processes.screen.txt")
    };
    let usage = if shapes.local_commands == LocalCommands::BgUsage {
        "Background task started\n"
    } else {
        include_str!("../fixtures/screens/phase3/local-bg-usage.screen.txt")
    };
    vec![
        RuleBuilder::run("^/tasks$").append(tasks),
        RuleBuilder::run("^/agents frobnicate$").append(agents),
        RuleBuilder::run("^/processes$").append(processes),
        RuleBuilder::run("^/bg$").append(usage),
    ]
}

fn fleet_view_rules(shapes: &Shapes) -> Vec<RuleBuilder> {
    let opened = if shapes.still_after_slash || shapes.fleet_view == FleetView::Open {
        "$0\n"
    } else {
        include_str!("../fixtures/screens/phase3/fleet-view-here.screen.txt")
    };
    let filtered = if shapes.fleet_view == FleetView::NoMatch {
        "$0\n"
    } else {
        include_str!("../fixtures/screens/phase3/fleet-view-no-match.screen.txt")
    };
    let closed = if shapes.fleet_view == FleetView::Close {
        include_str!("../fixtures/screens/phase3/fleet-view-here.screen.txt")
    } else {
        "minimax-m3.1-flash\n"
    };
    vec![
        RuleBuilder::run("^/fleet$").repeat().append(opened),
        RuleBuilder::send_text("^zzz-no-match$").append(filtered),
        RuleBuilder::send_keys("^ctrl\\+c$").append(closed),
    ]
}

fn open_rules(shape: OpenInPane) -> Vec<RuleBuilder> {
    let notice = match shape {
        OpenInPane::Notice => "$0\n",
        OpenInPane::NotInHerdr => {
            include_str!("../fixtures/screens/phase3/open-in-pane-not-in-herdr.screen.txt")
        }
        OpenInPane::Pass => {
            include_str!("../fixtures/screens/phase3/open-in-pane-notice.screen.txt")
        }
    };
    vec![
        RuleBuilder::send_text("^/$"),
        RuleBuilder::send_text("^o$").append(notice),
    ]
}

fn quit_rule(code: i32) -> RuleBuilder {
    RuleBuilder::run("^/quit$")
        .repeat()
        .foreground(&[])
        .append(&format!(
            "you: /quit\nSession detached.\nSMOKE_EXIT:{code}\n$ "
        ))
}

fn needs_you_rules(shape: NeedsYou) -> Vec<RuleBuilder> {
    let (body, code) = match shape {
        NeedsYou::Launch => ("not a launch line\n", 1),
        _ => (
            concat!(
                "{\"status\":\"launched\",\"session_id\":\"",
                "local-c3d4e5f678901234\"}\n"
            ),
            0,
        ),
    };
    let mut launch = RuleBuilder::run("ny\\.out")
        .repeat()
        .write_in_scratch("ny.out", body)
        .then(StageBuilder::new().write_in_scratch("ny.err", ""))
        .append(&exit_marker(code));
    if matches!(shape, NeedsYou::CommandRan | NeedsYou::SkipUnasked) {
        launch = launch.then(StageBuilder::new().remove_in_scratch("probe/talaria-smoke-probe"));
    }
    let listed = !matches!(
        shape,
        NeedsYou::NeverListed | NeedsYou::SkipUnasked | NeedsYou::RetryThenFail
    );
    let queue = if listed {
        format!(
            "{{\"rows\":[{{\"session_id\":\"{ID3}\",\"state\":\"working\"}}],\"needs_you\":[{{\"session_id\":\"{ID3}\"}}]}}\n"
        )
    } else if shape == NeedsYou::SkipUnasked {
        format!("{{\"rows\":[{{\"session_id\":\"{ID3}\",\"state\":\"done\"}}],\"needs_you\":[]}}\n")
    } else {
        format!(
            "{{\"rows\":[{{\"session_id\":\"{ID3}\",\"state\":\"working\"}}],\"needs_you\":[]}}\n"
        )
    };
    let poll = RuleBuilder::run("f4\\.json")
        .repeat()
        .write_in_scratch("f4.json", &queue)
        .append(&exit_marker(0));
    let notice = match shape {
        NeedsYou::Notice => "minimax-m3.1-flash\n",
        NeedsYou::Segment => "1 background session needs you; press alt+a to see it.\n",
        _ => include_str!("../fixtures/screens/phase3/needs-you-notice.screen.txt"),
    };
    let peek = if shape == NeedsYou::Peek {
        "NEEDS YOU (1)\n"
    } else {
        include_str!("../fixtures/screens/phase3/needs-you-peek.screen.txt")
    };
    let card = if shape == NeedsYou::NoDeny {
        include_str!("../fixtures/screens/phase3/needs-you-peek.screen.txt")
    } else {
        include_str!("../fixtures/screens/phase3/needs-you-card.screen.txt")
    };
    // Not-cleared keeps the card. The flag shape closes the list but leaves the mark.
    // Anything else shows the cleared screen.
    let cleared: &str = if shape == NeedsYou::NotCleared {
        include_str!("../fixtures/screens/phase3/needs-you-card.screen.txt")
    } else if shape == NeedsYou::SegmentStays {
        "minimax-m3.1-flash  \u{2691} 1 needs you\n"
    } else {
        include_str!("../fixtures/screens/phase3/needs-you-cleared.screen.txt")
    };
    let mut rules = vec![launch];
    match shape {
        // The first poll lists the request. Later polls (the fresh list after deny) do not,
        // so the failure is the mark that stays, not a request that stays listed.
        NeedsYou::SegmentStays => {
            let unlisted = format!(
                "{{\"rows\":[{{\"session_id\":\"{ID3}\",\"state\":\"done\"}}],\"needs_you\":[]}}\n"
            );
            rules.push(
                RuleBuilder::run("f4\\.json")
                    .write_in_scratch("f4.json", &queue)
                    .append(&exit_marker(0)),
            );
            rules.push(
                RuleBuilder::run("f4\\.json")
                    .repeat()
                    .write_in_scratch("f4.json", &unlisted)
                    .append(&exit_marker(0)),
            );
        }
        // The request stays out of the queue for the first attempt and the quiet wait,
        // then appears. The step test names the age as 151 s.
        NeedsYou::TooOld => {
            let unlisted = format!(
                "{{\"rows\":[{{\"session_id\":\"{ID3}\",\"state\":\"working\"}}],\"needs_you\":[]}}\n"
            );
            for _ in 0..31 {
                rules.push(
                    RuleBuilder::run("f4\\.json")
                        .write_in_scratch("f4.json", &unlisted)
                        .append(&exit_marker(0)),
                );
            }
            rules.push(poll);
        }
        _ => rules.push(poll),
    }
    rules.extend([
        RuleBuilder::run(r"--provider ollama-cloud;").append(notice),
        RuleBuilder::send_keys("^alt\\+a$").append(peek),
        RuleBuilder::send_text("^a$").append(card),
        RuleBuilder::send_text("^[0-9]$").append(cleared),
        RuleBuilder::send_keys("^esc$").append(cleared),
    ]);
    rules
}

/// A finished in-process phase 3 run.
pub struct Exec3 {
    /// The run's report.
    pub report: SmokeReport,
    /// The terminal the run drove.
    pub term: FakeTerminal<'static>,
    /// The temporary root the run used.
    pub root: PathBuf,
    _temp: tempfile::TempDir,
}

/// Runs `run_smoke_3` against the script with a manual clock and a seeded config.
pub fn exec_3(
    script: &ScriptBuilder,
    tune_cfg: impl FnOnce(&mut SmokeConfig),
    tune_term: impl FnOnce(FakeTerminal<'static>) -> FakeTerminal<'static>,
) -> Exec3 {
    let clock: &'static ManualClock = Box::leak(Box::new(ManualClock::new()));
    let temp = tempfile::tempdir().expect("temporary folder for the run");
    let root = std::fs::canonicalize(temp.path()).expect("resolve the temporary folder");
    let talaria = root.join("bin").join("talaria");
    std::fs::create_dir_all(talaria.parent().expect("a parent folder")).expect("create bin folder");
    write_executable(&talaria, "#!/bin/sh\nexit 0\n");
    let mut cfg = SmokeConfig {
        talaria,
        profile: "fixture".to_owned(),
        model: "minimax-m3.1-flash".to_owned(),
        provider: "ollama-cloud".to_owned(),
        deadline: Duration::from_secs(Suite::Phase3.default_deadline_secs()),
        step_scale: 1.0,
        report_dir: None,
        require: false,
        temp_root: root.clone(),
        seed: Some(11),
    };
    tune_cfg(&mut cfg);
    let term = tune_term(FakeTerminal::from_builder(script, clock));
    let stop = AtomicBool::new(false);
    let report = run_smoke_3(&term, clock, &cfg, &stop);
    Exec3 {
        report,
        term,
        root: cfg.temp_root,
        _temp: temp,
    }
}

impl Exec3 {
    /// `step:pass`, `step:skip` or `step:fail`, in order.
    pub fn kinds(&self) -> Vec<String> {
        self.report
            .outcomes
            .iter()
            .map(|outcome| match outcome {
                StepOutcome::Pass { step, .. } => format!("{step}:pass"),
                StepOutcome::Skip { step, .. } => format!("{step}:skip"),
                StepOutcome::Fail { step, .. } => format!("{step}:fail"),
            })
            .collect()
    }

    /// The one line per step.
    pub fn lines(&self) -> String {
        self.report.step_lines()
    }

    /// Every key sent, in order.
    pub fn sent_keys(&self) -> Vec<String> {
        self.term
            .calls()
            .into_iter()
            .flat_map(|recorded| match recorded.call {
                Call::SendKeys { keys, .. } => keys,
                _ => Vec::new(),
            })
            .collect()
    }

    /// Every submitted text, in order.
    pub fn run_texts(&self) -> Vec<String> {
        self.term
            .calls()
            .into_iter()
            .filter_map(|recorded| match recorded.call {
                Call::Run { text, .. } => Some(text),
                _ => None,
            })
            .collect()
    }

    /// Every typed text, in order.
    pub fn typed_texts(&self) -> Vec<String> {
        self.term
            .calls()
            .into_iter()
            .filter_map(|recorded| match recorded.call {
                Call::SendText { text, .. } => Some(text),
                _ => None,
            })
            .collect()
    }

    /// How often the tab was closed.
    pub fn close_tabs(&self) -> usize {
        self.term.calls_of("tab close").len()
    }

    /// The time of the last terminal call.
    pub fn last_at(&self) -> Duration {
        self.term
            .calls()
            .last()
            .map(|recorded| recorded.at)
            .unwrap_or_default()
    }
}

/// The last outcome passes this step.
pub fn assert_passes(exec: &Exec3, step: &str) {
    let passed =
        exec.report.outcomes.iter().any(
            |outcome| matches!(outcome, StepOutcome::Pass { step: name, .. } if *name == step),
        );
    assert!(passed, "{step} passes; lines:\n{}", exec.lines());
}

/// The last outcome fails this step with exactly this reason.
pub fn assert_fails_with(exec: &Exec3, step: &str, reason: &str) {
    match exec.report.outcomes.last() {
        Some(StepOutcome::Fail {
            step: failed,
            reason: why,
            ..
        }) => {
            assert_eq!(*failed, step, "lines:\n{}", exec.lines());
            assert_eq!(why, reason, "lines:\n{}", exec.lines());
        }
        _ => panic!(
            "the last outcome must fail {step}; lines:\n{}",
            exec.lines()
        ),
    }
}

/// The run closed its tab once.
pub fn assert_one_close_tab(exec: &Exec3) {
    assert_eq!(
        exec.close_tabs(),
        1,
        "one close_tab; lines:\n{}",
        exec.lines()
    );
}

/// The background prompt the launch steps send.
pub fn ok_prompt() -> &'static str {
    OK_PROMPT
}

/// The shell command the needs-you launch asks the model to run.
pub fn probe_prompt() -> &'static str {
    PROBE_PROMPT
}
