//! Screen scripts for the phase 2b suite: one rule group per step, each with the passing
//! shape and every failing shape the exact reasons need.
//!
//! Submits travel as `run` (the first suite's submit is `pane run`), typed text as
//! `send_text` and keys as `send_keys`. The token prompt matches the prompt shape and answers with the captured
//! word and number, so scripts never know the run's token; the plain-answer rule also
//! remembers `$1 and $2` as `${mem:0}` for the exported file, and the rename rule remembers
//! the run id as `${mem:1}` for the bare rename. Overlays clear or wipe when they close, so
//! the live screen never shows stale text. The headless outputs land in the scratch root,
//! one folder above the tab's working folder, so their writes use `..` paths.

use std::fs;
use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::time::Duration;

use smoke::{ManualClock, SmokeConfig, SmokeReport, StepOutcome, run_smoke_2b};

use super::{Call, FakeTerminal, RuleBuilder, ScriptBuilder, StageBuilder, write_executable};

/// The token prompt tail; `$1` is the word and `$2` the number.
const PROMPT_TAIL: &str =
    r"join these two words with a hyphen and reply with only the result: (\w+) and (\d+)$";
/// The launch flag run, matching plain and environment-prefixed launch lines alike.
const LAUNCH_RUN: &str = "--no-ensure --profile ";
/// The status line with manual approvals in folder `a`.
const STATUS_MANUAL_A: &str =
    " minimax-m3.1-flash \u{2502} approvals: manual \u{2502} \u{2026}/a \u{2502} ctx 34%";
/// The status line with approvals off in folder `a`.
const STATUS_OFF_A: &str = " minimax-m3.1-flash \u{2502} approvals: off (this session) \u{2502} \u{2026}/a \u{2502} ctx 34%";
/// The status line with manual approvals in folder `b`.
const STATUS_MANUAL_B: &str =
    " minimax-m3.1-flash \u{2502} approvals: manual \u{2502} \u{2026}/b (main) \u{2502} ctx 34%";
/// The status line after a model change that must not happen.
const STATUS_CHANGED: &str =
    " no-such-model-xyz \u{2502} approvals: manual \u{2502} \u{2026}/a \u{2502} ctx 34%";
/// The notice a successful first toggle prints.
const OFF_NOTICE: &str = "Approvals off for this session only: Hermes will run commands it would normally ask about. Rules that always block a command still apply. Shift+Tab turns approvals back on.";
/// The open model picker, as the recorded screen shows it.
const PICKER_OPEN: &str = "Switch model \u{00b7} this session\n>\n\u{2605} Ollama Cloud / minimax-m3.1-flash\nOllama Cloud \u{00b7} current\n\u{25cf} minimax-m3.1-flash\n  glm-5\nNot set up (2)\nAnthropic \u{00b7} needs ANTHROPIC_API_KEY \u{00b7} set up with hermes model\neffort: high \u{2192} xhigh \u{00b7} enter switch \u{00b7} esc close";
/// The open picker without its footer line.
const PICKER_NO_FOOTER: &str = "Switch model \u{00b7} this session\n>\n\u{2605} Ollama Cloud / minimax-m3.1-flash\nOllama Cloud \u{00b7} current\n\u{25cf} minimax-m3.1-flash\n  glm-5\nNot set up (2)\nAnthropic \u{00b7} needs ANTHROPIC_API_KEY \u{00b7} set up with hermes model";
/// The refusal of an unknown model.
const MODEL_REFUSAL: &str = "Hermes could not switch to no-such-model-xyz: it does not know that model on Ollama Cloud, or the provider is not set up. Nothing changed.";
/// The rewind list with both prompts, the second selected.
const REWIND_OPEN: &str = "Rewind \u{00b7} choose a prompt to go back to\n \u{203a} #2  5m ago  Now remember the code word lark instead. Reply ok.\n   #1  9m ago  Remember the code word heron. Reply ok.\n\u{2191}\u{2193} choose \u{00b7} enter open \u{00b7} esc close";
/// The rewind list with the first prompt missing.
const REWIND_ONE_ROW: &str = "Rewind \u{00b7} choose a prompt to go back to\n \u{203a} #2  5m ago  Now remember the code word lark instead. Reply ok.\n\u{2191}\u{2193} choose \u{00b7} enter open \u{00b7} esc close";
/// The rewound marker with the restored composer.
const REWOUND: &str = "\u{2500}\u{2500} Rewound to before prompt #2 (\"Now remember the code word lark instead. Reply ok.\"). Hermes removed 2 messages. The lines above from that prompt on are no longer part of the conversation. Files were not changed. \u{2500}\u{2500}\n> Now remember the code word lark instead.";
/// The code word prompts and the check prompt.
const CODE_FIRST: &str = "Remember the code word heron. Reply ok.";
const CODE_SECOND: &str = "Now remember the code word lark instead. Reply ok.";
const CODE_CHECK: &str = "Which code word should you remember? One word.";
/// The measured context header with its rows.
const CONTEXT_BLOCK: &str = "Context \u{00b7} minimax-m3.1-flash \u{00b7} 34,120 / 200,000 tokens (17%)\nEstimated usage by category\n\u{25a0} System prompt       ~3,210 tokens   1.6%\n\u{25a3} Tool definitions   ~11,840 tokens   5.9%\n\u{00b7} Free space        ~165,910 tokens  83.0%\nSource: provider usage; category counts are local estimates.";
/// The measured context header without its source line.
const CONTEXT_NO_SOURCE: &str = "Context \u{00b7} minimax-m3.1-flash \u{00b7} 34,120 / 200,000 tokens (17%)\nEstimated usage by category\n\u{25a0} System prompt       ~3,210 tokens   1.6%\n\u{25a3} Tool definitions   ~11,840 tokens   5.9%\n\u{00b7} Free space        ~165,910 tokens  83.0%";
/// The measured usage block.
const USAGE_BLOCK: &str = "Usage \u{00b7} this session\n  Model        minimax-m3.1-flash (ollama-cloud)\n  Tokens       input 30,210 \u{00b7} output 410 \u{00b7} reasoning 0 \u{00b7} total 30,620\n  API calls    3 \u{00b7} average latency 1.2 s\n  Context      34,120 / 200,000 tokens (17%)\n  Cost         not reported by Hermes for this session";
/// The long queue prompt.
const LONG_PROMPT: &str =
    "Run the shell command sleep 25, then reply with the word first in capital letters.";
/// The no-gateway standard error, as the recorded run shows it.
const NO_GATEWAY_ERR: &str = "talaria: no Hermes gateway is running for this home.\nNext step: start the gateway, then run the command again.\n";

/// The startup shape: the suite starts with manual approvals, or already off.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Startup {
    /// The launch lands on manual approvals.
    #[default]
    Pass,
    /// The launch lands with approvals already off.
    OffAtStart,
}

/// The context-empty shapes.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ContextEmpty {
    /// Both empty texts appear.
    #[default]
    Pass,
    /// `/context` stays silent.
    ContextLineMissing,
    /// `/usage` stays silent.
    UsageLineMissing,
}

/// The permission shapes (the skip is a terminal failure, not a shape).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Permission {
    /// The toggle goes off and back to manual.
    #[default]
    Pass,
    /// The first press changes nothing.
    NoOffSegment,
    /// The notice never appears.
    NoOffNotice,
    /// The second press restores nothing.
    NoRestore,
}

/// The model-picker shapes.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ModelPicker {
    /// The picker opens, filters and closes over two escapes.
    #[default]
    Pass,
    /// `/model` stays silent.
    OpenMissing,
    /// The list lacks its footer line.
    RowsMissing,
    /// Typing `zzzz` shows no match line.
    NoMatchMissing,
    /// The first escape does not restore the list.
    EscSteps,
}

/// The model-refusal shapes.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ModelRefusal {
    /// The refusal appears and the model stays.
    #[default]
    Pass,
    /// The refusal never appears.
    RefusalMissing,
    /// The status line names the refused model.
    ModelChanged,
}

/// The rewind shapes.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Rewind {
    /// Two code words, a rewind to the second prompt and the first word back.
    #[default]
    Pass,
    /// The first code word never gets its `ok`.
    ReplyTimeout,
    /// Two escapes open nothing.
    OverlayMissing,
    /// The list shows only the second prompt.
    ListRows,
    /// The confirm step prints no rewound marker.
    RewoundMissing,
    /// The composer comes back empty.
    ComposerNotRestored,
    /// The check prompt names the rewound word.
    WrongCodeWord,
    /// The overlay text stays in the scrollback.
    OverlayLeftText,
}

/// The plain-answer shape: the token prompt is answered with the joined token.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PlainAnswer {
    /// The reply holds the joined token.
    #[default]
    Pass,
}

/// The steer-idle shapes.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SteerIdle {
    /// The notice and the `IDLE` reply on the first attempt.
    #[default]
    Pass,
    /// The idle notice never appears.
    NoticeMissing,
    /// No `IDLE` reply on either attempt.
    ReplyMissing,
    /// The first attempt misses; the repeat hits.
    MissThenHit,
    /// The first attempt misses while a turn still works; the repeat hits.
    MissWhileWorking,
}

/// The context shapes.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Context {
    /// The header, the rows, the usage block and the two refusals.
    #[default]
    Pass,
    /// `/context` stays silent.
    HeaderMissing,
    /// The source line is missing.
    RowsMissing,
    /// `/usage` stays silent.
    UsageMissing,
    /// The aggressive refusal never appears.
    AggressiveMissing,
    /// The argument refusal never appears.
    ArgumentMissing,
    /// A compressed line appears after the refused commands.
    CompressedAppeared,
}

/// The hermes-commands shapes.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum HermesCommands {
    /// Both refusals appear and no loop line follows.
    #[default]
    Pass,
    /// `/fast` stays silent.
    FastMissing,
    /// `/loop 45m` stays silent.
    LoopMissing,
    /// A loop line appears after the promptless loop.
    LoopLine,
}

/// The rename-export shapes.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum RenameExport {
    /// The rename answers, the export lands with mode 0600 and the bad name is refused.
    #[default]
    Pass,
    /// The rename stays silent.
    RenamedMissing,
    /// The bare rename stays silent.
    TitleMissing,
    /// The export prints nothing.
    ExportedMissing,
    /// The export prints but writes no file.
    FileBad,
    /// The bad name is not refused.
    BadNameMissing,
    /// The bad name writes the file it must refuse.
    BadNameWrote,
}

/// The queue shapes.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Queue {
    /// The indicator, the preview, `FIRST`, the omega user line and `OMEGA` in order.
    #[default]
    Pass,
    /// No indicator on either attempt.
    IndicatorMissing,
    /// The preview shows twice before the prompt runs.
    PreviewTwice,
    /// `FIRST` never appears.
    FirstMissing,
    /// `OMEGA` never appears.
    OmegaMissing,
    /// `OMEGA` arrives before `FIRST`.
    OrderWrong,
    /// The indicator stays after the queue drained.
    IndicatorLeft,
    /// The first attempt misses; the repeat hits.
    MissThenHit,
    /// Both attempts miss the indicator.
    TwoMisses,
    /// The first attempt misses while a turn still works; the repeat hits.
    MissWhileWorking,
}

/// The cd shapes.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Cd {
    /// The move, the bare print and the missing-folder refusal.
    #[default]
    Pass,
    /// The move prints nothing.
    MovedMissing,
    /// The folder segment still names the old folder.
    SegmentWrong,
    /// The bare `/cd` stays silent.
    ShowMissing,
    /// The missing folder is not refused.
    MissingFolder,
}

/// The quit shapes.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Quit {
    /// The exit marker with the token still behind it.
    #[default]
    Pass,
    /// The pane text is wiped on exit.
    TokenMissing,
}

/// The headless shapes.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Headless {
    /// The text run prints 5555 and the no-gateway run prints its JSON line.
    #[default]
    Pass,
    /// The text run exits 7.
    TextRun,
    /// The no-gateway run prints the wrong JSON.
    NoGatewayJson,
    /// The no-gateway run prints the wrong standard error.
    NoGatewayText,
    /// The text command never finishes.
    CommandTimeout,
}

/// The launch rule with the passing screen for one status line.
fn launch_with(status: &str) -> RuleBuilder {
    RuleBuilder::run(LAUNCH_RUN)
        .foreground(&["-bash", "$0"])
        .append(&format!("$0\nTalaria\ntalaria\nidle\n{status}"))
        .get("talaria\nidle")
}

/// The launch and the stable screen.
pub fn startup_rules(shape: Startup) -> Vec<RuleBuilder> {
    match shape {
        Startup::Pass => vec![launch_with(STATUS_MANUAL_A)],
        Startup::OffAtStart => vec![launch_with(STATUS_OFF_A)],
    }
}

/// The two empty texts before any prompt.
pub fn context_empty_rules(shape: ContextEmpty) -> Vec<RuleBuilder> {
    let context = |body: &str| RuleBuilder::run(r"^/context$").append(body);
    let usage = |body: &str| RuleBuilder::run(r"^/usage$").append(body);
    let context_hit = "> /context\nContext \u{00b7} not measured yet\nHermes builds this session's agent on the next prompt; until then there is nothing to break down.";
    let usage_hit = "> /usage\n  No API calls yet in this session.";
    match shape {
        ContextEmpty::Pass => vec![context(context_hit), usage(usage_hit)],
        ContextEmpty::ContextLineMissing => vec![context("> /context")],
        ContextEmpty::UsageLineMissing => vec![context(context_hit), usage("> /usage")],
    }
}

/// The toggle key rules: each name has an off instance and a manual instance, so the
/// second press finds an unfired rule whatever name the suite sent first.
fn toggle_rules(off_body: &str, manual_body: &str) -> Vec<RuleBuilder> {
    vec![
        RuleBuilder::send_keys(r"^shift\+tab$").append(off_body),
        RuleBuilder::send_keys(r"^backtab$").append(off_body),
        RuleBuilder::send_keys(r"^shift\+tab$").append(manual_body),
        RuleBuilder::send_keys(r"^backtab$").append(manual_body),
    ]
}

/// The cleanup line the suite sends after a failed check that follows a working press.
fn cleanup_rule() -> RuleBuilder {
    RuleBuilder::run(r"^/permissions session default$").append(&format!(
        "> /permissions session default\n{STATUS_MANUAL_A}"
    ))
}

/// The approval toggle off and back to manual.
pub fn permission_rules(shape: Permission) -> Vec<RuleBuilder> {
    let off = format!("{STATUS_OFF_A}\n{OFF_NOTICE}");
    match shape {
        Permission::Pass => toggle_rules(&off, STATUS_MANUAL_A),
        Permission::NoOffSegment => toggle_rules(STATUS_MANUAL_A, STATUS_MANUAL_A),
        Permission::NoOffNotice => {
            let mut rules = toggle_rules(STATUS_OFF_A, STATUS_MANUAL_A);
            rules.push(cleanup_rule());
            rules
        }
        Permission::NoRestore => {
            let mut rules = toggle_rules(&off, STATUS_OFF_A);
            rules.push(cleanup_rule());
            rules
        }
    }
}

/// The model picker around its two escapes.
pub fn model_picker_rules(shape: ModelPicker) -> Vec<RuleBuilder> {
    let open = |body: &str| RuleBuilder::run(r"^/model$").append(body);
    let filter = |body: &str| RuleBuilder::send_text(r"^zzzz$").append(body);
    let esc_list = || {
        RuleBuilder::send_keys(r"^esc$")
            .clear()
            .append(&format!("> \n{PICKER_OPEN}"))
    };
    let esc_closed = || {
        RuleBuilder::send_keys(r"^esc$")
            .clear()
            .append(&format!("> \n{STATUS_MANUAL_A}"))
    };
    let no_match = "> zzzz\nNo model matches \"zzzz\".";
    match shape {
        ModelPicker::Pass => vec![
            open(&format!("> /model\n{PICKER_OPEN}")),
            filter(no_match),
            esc_list(),
            esc_closed(),
        ],
        ModelPicker::OpenMissing => vec![open("> /model")],
        ModelPicker::RowsMissing => vec![
            open(&format!("> /model\n{PICKER_NO_FOOTER}")),
            filter(no_match),
            esc_list(),
            esc_closed(),
        ],
        ModelPicker::NoMatchMissing => vec![
            open(&format!("> /model\n{PICKER_OPEN}")),
            filter("> zzzz"),
            esc_list(),
            esc_closed(),
        ],
        ModelPicker::EscSteps => vec![
            open(&format!("> /model\n{PICKER_OPEN}")),
            filter(no_match),
            RuleBuilder::send_keys(r"^esc$").append("> "),
            esc_closed(),
        ],
    }
}

/// The refused model switch.
pub fn model_refusal_rules(shape: ModelRefusal) -> Vec<RuleBuilder> {
    let submit = |body: &str| {
        RuleBuilder::run(r"^/model no-such-model-xyz --provider ollama-cloud$").append(body)
    };
    match shape {
        ModelRefusal::Pass => vec![submit(&format!(
            "> /model no-such-model-xyz --provider ollama-cloud\n{MODEL_REFUSAL}\n{STATUS_MANUAL_A}"
        ))],
        ModelRefusal::RefusalMissing => {
            vec![submit("> /model no-such-model-xyz --provider ollama-cloud")]
        }
        ModelRefusal::ModelChanged => vec![submit(&format!(
            "> /model no-such-model-xyz --provider ollama-cloud\n{MODEL_REFUSAL}\n{STATUS_CHANGED}"
        ))],
    }
}

/// One code word prompt with its bare `ok` reply line.
fn code_word(prompt: &str) -> RuleBuilder {
    RuleBuilder::run(prompt)
        .append(&format!("> {prompt}"))
        .churn(4)
        .then(StageBuilder::new().after_polls(4).append("ok"))
}

/// The rewind of the second prompt and the first code word back.
pub fn rewind_rules(shape: Rewind) -> Vec<RuleBuilder> {
    let first = code_word(CODE_FIRST);
    let second = code_word(CODE_SECOND);
    let esc_empty = RuleBuilder::send_keys(r"^esc$");
    let esc_list = |body: &str| RuleBuilder::send_keys(r"^esc$").append(body);
    let enter_ack = RuleBuilder::send_keys(r"^enter$");
    let typed_c = RuleBuilder::send_text(r"^c$").append("c");
    let confirm = RuleBuilder::send_keys(r"^enter$").wipe().append(REWOUND);
    let clear_draft = RuleBuilder::send_keys(r"^ctrl\+c$").append("> ");
    let check = |reply: &str| {
        RuleBuilder::run(&regex::escape(CODE_CHECK))
            .append(&format!("> {CODE_CHECK}"))
            .churn(4)
            .then(StageBuilder::new().after_polls(4).append(reply))
    };
    match shape {
        Rewind::Pass => vec![
            first,
            second,
            esc_empty,
            esc_list(REWIND_OPEN),
            enter_ack,
            typed_c,
            confirm,
            clear_draft,
            check("agent: heron"),
        ],
        Rewind::ReplyTimeout => vec![
            RuleBuilder::run(CODE_FIRST).append(&format!("> {CODE_FIRST}")),
        ],
        Rewind::OverlayMissing => vec![
            first,
            second,
            esc_empty,
            RuleBuilder::send_keys(r"^esc$").append("> "),
        ],
        Rewind::ListRows => vec![
            first,
            second,
            esc_empty,
            esc_list(REWIND_ONE_ROW),
            enter_ack,
            typed_c,
            confirm,
            clear_draft,
            check("agent: heron"),
        ],
        Rewind::RewoundMissing => vec![
            first,
            second,
            esc_empty,
            esc_list(REWIND_OPEN),
            enter_ack,
            typed_c,
            RuleBuilder::send_keys(r"^enter$")
                .wipe()
                .append("> Now remember the code word lark instead."),
            clear_draft,
            check("agent: heron"),
        ],
        Rewind::ComposerNotRestored => vec![
            first,
            second,
            esc_empty,
            esc_list(REWIND_OPEN),
            enter_ack,
            typed_c,
            RuleBuilder::send_keys(r"^enter$").wipe().append(
                "\u{2500}\u{2500} Rewound to before prompt #2 (\"Now remember the code word lark instead. Reply ok.\"). \u{2500}\u{2500}\n> ",
            ),
            clear_draft,
            check("agent: heron"),
        ],
        Rewind::WrongCodeWord => vec![
            first,
            second,
            esc_empty,
            esc_list(REWIND_OPEN),
            enter_ack,
            typed_c,
            confirm,
            clear_draft,
            check("agent: lark"),
        ],
        Rewind::OverlayLeftText => vec![
            first,
            second,
            esc_empty,
            esc_list(REWIND_OPEN),
            enter_ack,
            typed_c,
            RuleBuilder::send_keys(r"^enter$").append(REWOUND),
            clear_draft,
            check("agent: heron"),
        ],
    }
}

/// The token prompt answered with the joined token; the `word and number` form stays
/// `${mem:0}` for the exported file.
pub fn plain_answer_rules(shape: PlainAnswer) -> Vec<RuleBuilder> {
    match shape {
        PlainAnswer::Pass => vec![
            RuleBuilder::run(PROMPT_TAIL)
                .append("you: $0")
                .remember("$1 and $2")
                .churn(4)
                .then(StageBuilder::new().after_polls(4).append("agent: $1-$2")),
        ],
    }
}

/// The steer submit with its idle notice and the `IDLE` reply.
fn steer_hit() -> RuleBuilder {
    RuleBuilder::run(r"^/steer Reply with the word idle in capital letters\.$")
        .append("> /steer Reply with the word idle in capital letters.\nNo turn is running, so this was sent as a new prompt")
        .churn(4)
        .then(StageBuilder::new().after_polls(4).append("agent: IDLE"))
}

/// The steer submit with the notice but no reply.
fn steer_miss() -> RuleBuilder {
    RuleBuilder::run(r"^/steer Reply with the word idle in capital letters\.$")
        .append("> /steer Reply with the word idle in capital letters.\nNo turn is running, so this was sent as a new prompt")
}

/// The steer of an idle session, answered as a new prompt.
pub fn steer_idle_rules(shape: SteerIdle) -> Vec<RuleBuilder> {
    match shape {
        SteerIdle::Pass => vec![steer_hit()],
        SteerIdle::NoticeMissing => vec![
            RuleBuilder::run(r"^/steer Reply with the word idle in capital letters\.$")
                .append("> /steer Reply with the word idle in capital letters."),
        ],
        SteerIdle::ReplyMissing => vec![steer_miss(), steer_miss()],
        SteerIdle::MissThenHit => vec![steer_miss(), steer_hit()],
        SteerIdle::MissWhileWorking => vec![
            RuleBuilder::run(r"^/steer Reply with the word idle in capital letters\.$")
                .append("> /steer Reply with the word idle in capital letters.\nNo turn is running, so this was sent as a new prompt")
                .churn(400),
            steer_hit(),
        ],
    }
}

/// The measured context, usage and the two refusals.
pub fn context_rules(shape: Context) -> Vec<RuleBuilder> {
    let context = |body: &str| RuleBuilder::run(r"^/context$").append(body);
    let usage = |body: &str| RuleBuilder::run(r"^/usage$").append(body);
    let aggressive = |body: &str| RuleBuilder::run(r"^/compact --aggressive$").append(body);
    let argument = |body: &str| RuleBuilder::run(r"^/context now$").append(body);
    let context_hit = format!("> /context\n{CONTEXT_BLOCK}");
    let usage_hit = format!("> /usage\n{USAGE_BLOCK}");
    let aggressive_hit =
        "> /compact --aggressive\nHermes does not support --aggressive. Nothing was changed.";
    let argument_hit = "> /context now\n/context does not take an argument.";
    match shape {
        Context::Pass => vec![
            context(&context_hit),
            usage(&usage_hit),
            aggressive(aggressive_hit),
            argument(argument_hit),
        ],
        Context::HeaderMissing => vec![context("> /context")],
        Context::RowsMissing => vec![
            context(&format!("> /context\n{CONTEXT_NO_SOURCE}")),
            usage(&usage_hit),
            aggressive(aggressive_hit),
            argument(argument_hit),
        ],
        Context::UsageMissing => vec![context(&context_hit), usage("> /usage")],
        Context::AggressiveMissing => vec![
            context(&context_hit),
            usage(&usage_hit),
            aggressive("> /compact --aggressive"),
        ],
        Context::ArgumentMissing => vec![
            context(&context_hit),
            usage(&usage_hit),
            aggressive(aggressive_hit),
            argument("> /context now"),
        ],
        Context::CompressedAppeared => vec![
            context(&context_hit),
            usage(&usage_hit),
            aggressive(&format!(
                "{aggressive_hit}\n\u{2713} Compressed 12 cells, 34,120 tokens kept."
            )),
            argument(argument_hit),
        ],
    }
}

/// The two Hermes refusals with no loop behind them.
pub fn hermes_commands_rules(shape: HermesCommands) -> Vec<RuleBuilder> {
    let fast = |body: &str| RuleBuilder::run(r"^/fast$").append(body);
    let looping = |body: &str| RuleBuilder::run(r"^/loop 45m$").append(body);
    let fast_hit = "> /fast\n\u{26a1} /fast is only available for OpenAI models that support Priority Processing.";
    let loop_hit = "> /loop 45m\n/loop: missing prompt (usage: /loop [interval] <prompt>)";
    match shape {
        HermesCommands::Pass => vec![fast(fast_hit), looping(loop_hit)],
        HermesCommands::FastMissing => vec![fast("> /fast")],
        HermesCommands::LoopMissing => vec![fast(fast_hit), looping("> /loop 45m")],
        HermesCommands::LoopLine => vec![
            fast(fast_hit),
            looping(&format!("{loop_hit}\n\u{21bb} loop every 45m (no prompt)")),
        ],
    }
}

/// The rename that remembers the run id as `${mem:1}`.
fn rename_hit() -> RuleBuilder {
    RuleBuilder::run(r"^/rename fixture smoke (\S+)$")
        .remember("$1")
        .append("Renamed to \"fixture smoke $1\".")
}

/// The bare rename, answered with the remembered run id.
fn title_hit() -> RuleBuilder {
    RuleBuilder::run(r"^/rename$").append(
        "This session is called \"fixture smoke ${mem:1}\". Type /rename <new title> to change it.",
    )
}

/// The export that writes the remembered prompt line with mode 0600.
fn export_hit() -> RuleBuilder {
    RuleBuilder::run(r"^/export$")
        .append("Exported \"fixture smoke ${mem:1}\" to ./fixture-smoke-export.md (4.2 KiB, Markdown, secrets redacted).")
        .then(
            StageBuilder::new().write_in_cwd_with_mode(
                "fixture-smoke-export.md",
                "${mem:0}\n",
                0o600,
            ),
        )
}

/// The refused bad name, writing nothing.
fn bad_name_hit() -> RuleBuilder {
    RuleBuilder::run(r"^/export \./notes\.txt$")
        .append("> /export ./notes.txt\nUse a file name ending in .md, .json or .html.")
}

/// The rename, the export to folder `a` and the refused bad name.
pub fn rename_export_rules(shape: RenameExport) -> Vec<RuleBuilder> {
    match shape {
        RenameExport::Pass => vec![rename_hit(), title_hit(), export_hit(), bad_name_hit()],
        RenameExport::RenamedMissing => vec![
            RuleBuilder::run(r"^/rename fixture smoke (\S+)$")
                .append("> /rename fixture smoke $1"),
        ],
        RenameExport::TitleMissing => vec![rename_hit(), RuleBuilder::run(r"^/rename$").append("> /rename")],
        RenameExport::ExportedMissing => vec![
            rename_hit(),
            title_hit(),
            RuleBuilder::run(r"^/export$").append("> /export"),
        ],
        RenameExport::FileBad => vec![
            rename_hit(),
            title_hit(),
            RuleBuilder::run(r"^/export$")
                .append("Exported \"fixture smoke ${mem:1}\" to ./fixture-smoke-export.md (4.2 KiB, Markdown, secrets redacted)."),
            bad_name_hit(),
        ],
        RenameExport::BadNameMissing => vec![
            rename_hit(),
            title_hit(),
            export_hit(),
            RuleBuilder::run(r"^/export \./notes\.txt$").append("> /export ./notes.txt"),
        ],
        RenameExport::BadNameWrote => vec![
            rename_hit(),
            title_hit(),
            export_hit(),
            RuleBuilder::run(r"^/export \./notes\.txt$")
                .append("> /export ./notes.txt\nUse a file name ending in .md, .json or .html.")
                .then(StageBuilder::new().write_in_cwd("notes.txt", "the refused file\n")),
        ],
    }
}

/// The long prompt, echoed as a running turn.
fn long_submit() -> RuleBuilder {
    RuleBuilder::run(LONG_PROMPT).append(&format!("> {LONG_PROMPT}"))
}

/// The omega prompt answered with the indicator and the preview, then `FIRST`, the omega
/// user line and `OMEGA` in order, and a drained screen.
fn omega_hit() -> RuleBuilder {
    RuleBuilder::run(r"Reply with the word omega in capital letters\.$")
        .append("queued (1)\nReply with the word omega in capital letters.")
        .then(StageBuilder::new().after_polls(4).append("agent: FIRST"))
        .then(
            StageBuilder::new()
                .after_polls(8)
                .append("> Reply with the word omega in capital letters.\nagent: OMEGA"),
        )
        .then(
            StageBuilder::new()
                .after_polls(12)
                .clear()
                .append(STATUS_MANUAL_A),
        )
}

/// The omega prompt answered at once, with no indicator because the turn already ended.
/// The answers arrive in the wrong order on purpose: the repeat passes only when its
/// checks read past the last echo of its own long prompt.
fn omega_instant() -> RuleBuilder {
    RuleBuilder::run(r"Reply with the word omega in capital letters\.$")
        .append("agent: OMEGA\nagent: FIRST")
}

/// The omega prompt with no answer at all.
fn omega_miss() -> RuleBuilder {
    RuleBuilder::run(r"Reply with the word omega in capital letters\.$")
}

/// The queued prompt behind the long sleep.
pub fn queue_rules(shape: Queue) -> Vec<RuleBuilder> {
    match shape {
        Queue::Pass => vec![long_submit(), omega_hit()],
        Queue::IndicatorMissing => vec![long_submit(), omega_miss(), long_submit(), omega_miss()],
        Queue::PreviewTwice => vec![
            long_submit(),
            RuleBuilder::run(r"Reply with the word omega in capital letters\.$").append(
                "queued (1)\nReply with the word omega in capital letters.\nReply with the word omega in capital letters.",
            ),
        ],
        Queue::FirstMissing => vec![
            long_submit(),
            RuleBuilder::run(r"Reply with the word omega in capital letters\.$")
                .append("queued (1)\nReply with the word omega in capital letters."),
        ],
        Queue::OmegaMissing => vec![
            long_submit(),
            RuleBuilder::run(r"Reply with the word omega in capital letters\.$")
                .append("queued (1)\nReply with the word omega in capital letters.")
                .then(StageBuilder::new().after_polls(4).append("agent: FIRST")),
        ],
        Queue::OrderWrong => vec![
            long_submit(),
            RuleBuilder::run(r"Reply with the word omega in capital letters\.$")
                .append("queued (1)\nReply with the word omega in capital letters.")
                .then(
                    StageBuilder::new()
                        .after_polls(4)
                        .append("> Reply with the word omega in capital letters.\nagent: OMEGA"),
                )
                .then(StageBuilder::new().after_polls(8).append("agent: FIRST")),
        ],
        Queue::IndicatorLeft => vec![
            long_submit(),
            RuleBuilder::run(r"Reply with the word omega in capital letters\.$")
                .append("queued (1)\nReply with the word omega in capital letters.")
                .then(StageBuilder::new().after_polls(4).append("agent: FIRST"))
                .then(
                    StageBuilder::new()
                        .after_polls(8)
                        .append("> Reply with the word omega in capital letters.\nagent: OMEGA"),
                )
                .then(
                    StageBuilder::new()
                        .after_polls(12)
                        .append(&format!("queued (1)\n{STATUS_MANUAL_A}")),
                ),
        ],
        Queue::MissThenHit => vec![long_submit(), omega_instant(), long_submit(), omega_hit()],
        Queue::TwoMisses => vec![long_submit(), omega_miss(), long_submit(), omega_miss()],
        Queue::MissWhileWorking => vec![
            long_submit(),
            RuleBuilder::run(r"Reply with the word omega in capital letters\.$").churn(40),
            long_submit(),
            omega_hit(),
        ],
    }
}

/// The move notice with the remembered folder, then the manual status in `b`.
fn moved_hit() -> RuleBuilder {
    RuleBuilder::run(r"^/cd \.\./b$")
        .append("> /cd ../b\nMoving this session to \u{2026}/b\u{2026}\nThis session now works in \u{2026}/b (main). Its next command runs there; the conversation is kept.")
        .then(
            StageBuilder::new()
                .after_polls(1)
                .clear()
                .append(STATUS_MANUAL_B),
        )
}

/// The working folder move and its printouts.
pub fn cd_rules(shape: Cd) -> Vec<RuleBuilder> {
    let show = |body: &str| RuleBuilder::run(r"^/cd$").append(body);
    let missing = |body: &str| RuleBuilder::run(r"^/cd \./missing-folder-smoke$").append(body);
    let show_hit = "> /cd\nThis session works in \u{2026}/b (main). Use /cd <folder> to move it.";
    let missing_hit = "> /cd ./missing-folder-smoke\nNo folder at \u{2026}/missing-folder-smoke.";
    match shape {
        Cd::Pass => vec![moved_hit(), show(show_hit), missing(missing_hit)],
        Cd::MovedMissing => vec![RuleBuilder::run(r"^/cd \.\./b$").append("> /cd ../b")],
        Cd::SegmentWrong => vec![
            RuleBuilder::run(r"^/cd \.\./b$")
                .append("> /cd ../b\nMoving this session to \u{2026}/b\u{2026}\nThis session now works in \u{2026}/b (main). Its next command runs there; the conversation is kept.")
                .then(
                    StageBuilder::new()
                        .after_polls(1)
                        .clear()
                        .append(STATUS_MANUAL_A),
                ),
            show(show_hit),
            missing(missing_hit),
        ],
        Cd::ShowMissing => vec![moved_hit(), show("> /cd")],
        Cd::MissingFolder => vec![
            moved_hit(),
            show(show_hit),
            missing("> /cd ./missing-folder-smoke"),
        ],
    }
}

/// The exit with the token still in the recent text.
pub fn quit_rules(shape: Quit) -> Vec<RuleBuilder> {
    let quit = || RuleBuilder::run(r"^/quit$");
    match shape {
        Quit::Pass => vec![
            quit()
                .foreground(&[])
                .append("> /quit\nSession detached.\nSMOKE_EXIT:0\n$ "),
        ],
        Quit::TokenMissing => vec![
            quit()
                .wipe()
                .foreground(&[])
                .append("> /quit\nSession detached.\nSMOKE_EXIT:0\n$ "),
        ],
    }
}

/// The plain headless run: the answer file, the empty error file and the zero marker.
fn text_hit() -> RuleBuilder {
    RuleBuilder::run("What is 1234 plus 4321")
        .write_in_cwd("../print.out", "5555\n")
        .then(
            StageBuilder::new()
                .write_in_cwd("../print.err", "")
                .append("SMOKE_EXIT:0"),
        )
}

/// The no-gateway run: one JSON line, the gateway error and the marker 4.
fn nogw_hit() -> RuleBuilder {
    RuleBuilder::run("HERMES_HOME=")
        .write_in_cwd(
            "../nogw.out",
            "{\"status\":\"not_started\",\"exit_code\":4}\n",
        )
        .then(
            StageBuilder::new()
                .write_in_cwd("../nogw.err", NO_GATEWAY_ERR)
                .append("SMOKE_EXIT:4"),
        )
}

/// The two headless runs on the shell that is back.
pub fn headless_rules(shape: Headless) -> Vec<RuleBuilder> {
    match shape {
        Headless::Pass => vec![text_hit(), nogw_hit()],
        Headless::TextRun => vec![
            RuleBuilder::run("What is 1234 plus 4321")
                .write_in_cwd("../print.out", "5555\n")
                .then(
                    StageBuilder::new()
                        .write_in_cwd("../print.err", "")
                        .append("SMOKE_EXIT:7"),
                ),
            nogw_hit(),
        ],
        Headless::NoGatewayJson => vec![
            text_hit(),
            RuleBuilder::run("HERMES_HOME=")
                .write_in_cwd("../nogw.out", "{\"status\":\"error\",\"exit_code\":4}\n")
                .then(
                    StageBuilder::new()
                        .write_in_cwd("../nogw.err", NO_GATEWAY_ERR)
                        .append("SMOKE_EXIT:4"),
                ),
        ],
        Headless::NoGatewayText => vec![
            text_hit(),
            RuleBuilder::run("HERMES_HOME=")
                .write_in_cwd(
                    "../nogw.out",
                    "{\"status\":\"not_started\",\"exit_code\":4}\n",
                )
                .then(
                    StageBuilder::new()
                        .write_in_cwd("../nogw.err", "unexpected failure\n")
                        .append("SMOKE_EXIT:4"),
                ),
        ],
        Headless::CommandTimeout => vec![RuleBuilder::run("What is 1234 plus 4321")],
    }
}

/// The steps of the phase 2b run after startup, in order.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Step {
    ContextEmpty,
    Permission,
    ModelPicker,
    ModelRefusal,
    Rewind,
    PlainAnswer,
    SteerIdle,
    Context,
    HermesCommands,
    RenameExport,
    Queue,
    Cd,
    Quit,
    Headless,
}

/// The shape of every step of a script; all default to the passing shape.
#[derive(Default)]
pub struct Shapes {
    /// The startup shape.
    pub startup: Startup,
    /// The context-empty shape.
    pub context_empty: ContextEmpty,
    /// The permission shape.
    pub permission: Permission,
    /// The model-picker shape.
    pub model_picker: ModelPicker,
    /// The model-refusal shape.
    pub model_refusal: ModelRefusal,
    /// The rewind shape.
    pub rewind: Rewind,
    /// The plain-answer shape.
    pub plain_answer: PlainAnswer,
    /// The steer-idle shape.
    pub steer_idle: SteerIdle,
    /// The context shape.
    pub context: Context,
    /// The hermes-commands shape.
    pub hermes_commands: HermesCommands,
    /// The rename-export shape.
    pub rename_export: RenameExport,
    /// The queue shape.
    pub queue: Queue,
    /// The cd shape.
    pub cd: Cd,
    /// The quit shape.
    pub quit: Quit,
    /// The headless shape.
    pub headless: Headless,
}

/// A script with a passing run-up to `stop` and the shapes of `shapes` at it. Steps
/// after `stop` have no rules, so the run ends at the first of them.
pub fn script_2b(shapes: &Shapes, stop: Step) -> ScriptBuilder {
    let mut script = ScriptBuilder::new();
    let mut first: Vec<RuleBuilder> = Vec::new();
    first.extend(startup_rules(shapes.startup));
    first.extend(context_empty_rules(shapes.context_empty));
    if stop >= Step::Permission {
        first.extend(permission_rules(shapes.permission));
    }
    if stop >= Step::ModelPicker {
        first.extend(model_picker_rules(shapes.model_picker));
    }
    if stop >= Step::ModelRefusal {
        first.extend(model_refusal_rules(shapes.model_refusal));
    }
    if stop >= Step::Rewind {
        first.extend(rewind_rules(shapes.rewind));
    }
    if stop >= Step::PlainAnswer {
        first.extend(plain_answer_rules(shapes.plain_answer));
    }
    if stop >= Step::SteerIdle {
        first.extend(steer_idle_rules(shapes.steer_idle));
    }
    if stop >= Step::Context {
        first.extend(context_rules(shapes.context));
    }
    if stop >= Step::HermesCommands {
        first.extend(hermes_commands_rules(shapes.hermes_commands));
    }
    if stop >= Step::RenameExport {
        first.extend(rename_export_rules(shapes.rename_export));
    }
    if stop >= Step::Queue {
        first.extend(queue_rules(shapes.queue));
    }
    if stop >= Step::Cd {
        first.extend(cd_rules(shapes.cd));
    }
    if stop >= Step::Quit {
        first.extend(quit_rules(shapes.quit));
    }
    if stop >= Step::Headless {
        first.extend(headless_rules(shapes.headless));
    }
    for rule in first {
        script = script.on_first(rule);
    }
    script
}

// ---- running phase 2b scripts in process ----

/// A finished in-process phase 2b run: the report and the terminal.
pub struct Exec2b {
    /// The run's report.
    pub report: SmokeReport,
    /// The terminal the run drove.
    pub term: FakeTerminal<'static>,
    /// The temporary root the run used.
    pub root: PathBuf,
    _temp: tempfile::TempDir,
}

/// Runs `run_smoke_2b` against the script with a manual clock and a seeded config with
/// the phase 2b deadline.
pub fn exec_2b(
    script: &ScriptBuilder,
    tune_cfg: impl FnOnce(&mut SmokeConfig),
    tune_term: impl FnOnce(FakeTerminal<'static>) -> FakeTerminal<'static>,
) -> Exec2b {
    let clock: &'static ManualClock = Box::leak(Box::new(ManualClock::new()));
    let temp = tempfile::tempdir().expect("temporary folder for the run");
    let root = fs::canonicalize(temp.path()).expect("resolve the temporary folder");
    let talaria = root.join("bin").join("talaria");
    fs::create_dir_all(talaria.parent().expect("a parent folder")).expect("create bin folder");
    write_executable(&talaria, "#!/bin/sh\nexit 0\n");
    let mut cfg = SmokeConfig {
        talaria,
        profile: "fixture".to_owned(),
        model: "minimax-m3.1-flash".to_owned(),
        provider: "ollama-cloud".to_owned(),
        deadline: Duration::from_secs(1500),
        step_scale: 1.0,
        report_dir: None,
        require: false,
        temp_root: root.clone(),
        seed: Some(11),
    };
    tune_cfg(&mut cfg);
    let term = tune_term(FakeTerminal::from_builder(script, clock));
    let stop = AtomicBool::new(false);
    let report = run_smoke_2b(&term, clock, &cfg, &stop);
    Exec2b {
        report,
        term,
        root: cfg.temp_root.clone(),
        _temp: temp,
    }
}

impl Exec2b {
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

    /// The reason of the last outcome, if it is a skip or a failure.
    pub fn last_reason(&self) -> Option<String> {
        self.report
            .outcomes
            .last()
            .and_then(|outcome| match outcome {
                StepOutcome::Skip { reason, .. } | StepOutcome::Fail { reason, .. } => {
                    Some(reason.clone())
                }
                StepOutcome::Pass { .. } => None,
            })
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

/// The keys sent are exactly these, in order (passing scripts only; a failing submit may
/// also carry the single `enter` fallback, so failing scripts use [`assert_keys_in_order`]).
pub fn assert_keys_exact(exec: &Exec2b, expected: &[&str]) {
    let actual = exec.sent_keys();
    let expected: Vec<String> = expected.iter().map(ToString::to_string).collect();
    assert_eq!(
        actual,
        expected,
        "the keys sent in order; lines:\n{}",
        exec.lines()
    );
}

/// The keys hold these in order, possibly with fallback keys between them.
pub fn assert_keys_in_order(exec: &Exec2b, expected: &[&str]) {
    let actual = exec.sent_keys();
    let mut rest = actual.as_slice();
    for want in expected {
        let Some(at) = rest.iter().position(|got| got == want) else {
            panic!(
                "key {want} missing in order in {actual:?}; lines:\n{}",
                exec.lines()
            );
        };
        rest = &rest[at + 1..];
    }
}

/// The last outcome fails this step with exactly this reason.
pub fn assert_fails_with(exec: &Exec2b, step: &str, reason: &str) {
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

/// The outcomes hold a pass of this step.
pub fn assert_passes(exec: &Exec2b, step: &str) {
    assert!(
        exec.report.outcomes.iter().any(|outcome| matches!(
            outcome,
            StepOutcome::Pass { step: passed, .. } if *passed == step
        )),
        "{step} must pass; lines:\n{}",
        exec.lines()
    );
}

/// The run closed the tab exactly once.
pub fn assert_one_close_tab(exec: &Exec2b) {
    assert_eq!(
        exec.close_tabs(),
        1,
        "one close_tab; lines:\n{}",
        exec.lines()
    );
}
