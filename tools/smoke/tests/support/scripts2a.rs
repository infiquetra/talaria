//! Screen scripts for the phase 2a suite: one rule group per step, each with the passing
//! shape and every failing shape the exact reasons need.
//!
//! Token prompts match the prompt shape and answer with the captured word and number, so scripts
//! never know the run's tokens. The first prompt's joined token stays `${mem:0}` for the side
//! answer; every later reply uses its own captures or the last remembered value. The split pane
//! carries the resumed session first and the tmux session after (the fake reopens it on the
//! second split, which resets the used flags: the resumed pager rules may fire once more, so
//! every tmux-section reply is appended by its question rule and bare `enter` rules absorb the
//! keys); paste rules live there too, fed by the fake paster.

use std::fs;
use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::time::Duration;

use smoke::{ManualClock, Phase2aConfig, SmokeConfig, SmokeReport, StepOutcome, run_smoke_2a};

use super::{
    Call, FakePaster, FakeTerminal, PasterRecorded, RuleBuilder, ScriptBuilder, StageBuilder,
    write_executable,
};

/// The prompt tail shared with the first suite's prompt pattern.
const PROMPT_TAIL: &str =
    r"join these two words with a hyphen and reply with only the result: (\w+) and (\d+)$";
/// The count prompt both suites submit.
const COUNT_PATTERN: &str = r"Count from 1 to 2000, one number per line, and do not stop early\.$";
/// The checklist plan prompt.
const PLAN_PATTERN: &str = "todo_list tool to write a plan";
/// The launch flag run, matching plain and environment-prefixed launch lines alike.
const LAUNCH_RUN: &str = "--no-ensure --profile ";
/// The resumed launch flag run.
const RESUME_RUN: &str = "--no-ensure -c --profile ";

/// The passing tool-shell screen; mirrors `tool_shell_failed.txt` with the run's token.
pub const TOOL_SHELL_OK: &str = concat!(
    "⏺ Shell(ls /talaria-no-such-dir)  ✗ exit 1 · 0.0s\n",
    "No such file or directory\n",
    "agent: $1-$2",
);

/// The startup launch on the first pane.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Startup {
    /// The launch reports `talaria` idle.
    #[default]
    Happy,
    /// The launch reports nothing (the pane-state step skips).
    NoAgent,
}

/// The tool-shell shapes.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ToolShell {
    /// The failed `ls` cell, its output row and the reply.
    #[default]
    Happy,
    /// The cell without the reply token.
    NoToken,
    /// No tool header.
    NoHeader,
    /// The header without the output row.
    NoOutput,
    /// A header for `ls /other` instead.
    OtherCmd,
}

/// The tool-edit shapes.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ToolEdit {
    /// The diff screen and the rewritten file.
    #[default]
    Happy,
    /// Debris rows beside the diff.
    Debris,
    /// The header without the stat fragment.
    NoStat,
    /// The header without numbered rows.
    NoDiffRows,
    /// The screen is right but the file still holds `beta`.
    FileUnchanged,
    /// The first attempt uses the shell; the retry passes.
    RetryShell,
    /// Both attempts use the shell.
    NoEditToolTwice,
    /// The screen without the reply token.
    NoToken,
}

/// The palette shapes.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Palette {
    /// The version list, the block, the group and the close.
    #[default]
    Happy,
    /// `/vers` lists no `/version` row.
    NotOpen,
    /// Enter prints no version block.
    NoBlock,
    /// The group lacks `/skin` and `/theme`.
    GroupLacks,
    /// Esc leaves the footer on screen.
    NotClosed,
}

/// The mention shapes.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Mention {
    /// The list, the insert and the reply word.
    #[default]
    Happy,
    /// The list without the row.
    NoRow,
    /// Tab inserts nothing.
    TabNothing,
    /// The reply misses the word.
    ReplyMisses,
}

/// The shell shapes.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Shell {
    /// The echo cell and the refusal.
    #[default]
    Happy,
    /// The echo header without its output row.
    NoOutput,
    /// No refusal row.
    NoRefusal,
    /// The refused command removed the folder.
    FolderRemoved,
}

/// The side-answer shapes.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Btw {
    /// The header, the answer with the first token's word, the footer.
    #[default]
    Happy,
    /// No side cell.
    NoCell,
    /// The header and footer with an empty answer.
    Empty,
    /// The answer failed.
    Failed,
    /// The answer names only the second token's word.
    LacksWord,
}

/// The pager shapes.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Pager {
    /// Open, following, both searches, close, counts equal.
    #[default]
    Happy,
    /// Ctrl+O opens nothing.
    NotOpen,
    /// The footer never says `following`.
    NotFollowing,
    /// The token search finds no match.
    TokenNoMatch,
    /// The tool search finds no match.
    ToolNoMatch,
    /// `q` leaves the pager open.
    QStuck,
    /// Closing interrupts the agent.
    Interrupt,
}

/// The history shapes.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum History {
    /// The box, the load and the clear.
    #[default]
    Happy,
    /// The box without the prompt.
    EmptyBox,
    /// Enter loads nothing.
    LoadFail,
    /// Ctrl+C clears nothing.
    ClearFail,
}

/// The editor shapes.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Editor {
    /// The edited draft, unsent.
    #[default]
    Happy,
    /// The draft never changes.
    DraftUnchanged,
    /// A reply starts without Enter.
    Sent,
}

/// The theme shapes.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Theme {
    /// The report and both usage lines, no refusal.
    #[default]
    Happy,
    /// The report misses keys.
    ReportMissing,
    /// `/skin ../etc` prints no usage line.
    SkinUsage,
    /// `/theme purple` prints no usage line.
    ArgUsage,
    /// A bad argument reaches Hermes.
    Refused,
}

/// The pane-state shapes.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PaneState {
    /// Fifteen counting lines, working, then idle.
    #[default]
    Happy,
    /// Fewer than fifteen counting lines.
    Counting,
    /// The report never says `working`.
    Working,
    /// The report never says `idle` again.
    Idle,
}

/// The checklist shapes.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Checklist {
    /// The panel, both toggles, one record.
    #[default]
    Happy,
    /// No panel in two attempts.
    SkipTwice,
    /// The first Ctrl+T does not collapse.
    CollapseFail,
    /// The second Ctrl+T does not expand.
    ExpandFail,
    /// The turn never ends.
    EndFail,
    /// The panel outlives the turn.
    PanelLeft,
    /// Two records.
    TwoRecords,
}

/// The resumed-pane shapes.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Resume {
    /// No local cells, the pager check, one resumed record.
    #[default]
    Happy,
    /// A side cell is printed again.
    LocalCell,
    /// The resumed record is printed twice.
    RecordTwice,
    /// The resumed pager finds nothing.
    PagerFail,
}

/// The tmux-pane shapes.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum TmuxPane {
    /// The session starts and the screen settles.
    #[default]
    Happy,
    /// The session never appears (the test forces the paster answer).
    StartTimeout,
    /// Talaria exits before showing its screen (the test forces the exit code).
    ExitEarly,
}

/// The paste-collapse shapes.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PasteCollapse {
    /// The label, the hidden text, the marker reply.
    #[default]
    Happy,
    /// No label.
    NoLabel,
    /// The pasted text stays visible.
    Leak,
    /// No marker reply.
    NoReply,
}

/// The file-drop shapes.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum FileDrop {
    /// The file label and the secret-word reply.
    #[default]
    Happy,
    /// No file label.
    NoLabel,
    /// No secret-word reply.
    NoReply,
}

/// The paste-editor shapes.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PasteEditor {
    /// The second label, the edited label, the marker reply.
    #[default]
    Happy,
    /// No second label.
    NoLabel,
    /// The label is lost to the editor.
    LabelLost,
    /// No marker reply.
    NoReply,
}

/// The tmux-quit shapes.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum TmuxQuit {
    /// Exit zero and the session gone.
    #[default]
    Happy,
    /// Exit seven.
    ExitWrong,
    /// The session outlives the quit (the test forces the paster answers).
    SessionStuck,
}

/// The startup launch on the first pane.
pub fn startup_rules(startup: Startup) -> Vec<RuleBuilder> {
    let rule = RuleBuilder::run(LAUNCH_RUN)
        .foreground(&["-bash", "$0"])
        .append("$0\nTalaria\nConnecting to Hermes");
    match startup {
        Startup::Happy => vec![rule.get("talaria\nidle")],
        Startup::NoAgent => vec![rule],
    }
}

/// The plain-answer prompt (token A) on the first pane.
pub fn answer_a_rules() -> Vec<RuleBuilder> {
    vec![
        RuleBuilder::run(PROMPT_TAIL)
            .append("you: $0")
            .remember("$1-$2")
            .churn(4)
            .then(StageBuilder::new().after_polls(4).append("agent: $1-$2")),
    ]
}

/// The tool-shell prompt (token B) on the first pane. The token is remembered second, so
/// the side answer can name it for the negative case.
pub fn tool_shell_rules(shape: ToolShell) -> Vec<RuleBuilder> {
    let prompt = || {
        RuleBuilder::run(&format!("Run the shell command.*{PROMPT_TAIL}"))
            .append("you: $0")
            .remember("$1-$2")
            .churn(4)
    };
    let reply = |screen: &str| prompt().then(StageBuilder::new().after_polls(4).append(screen));
    match shape {
        ToolShell::Happy => vec![reply(TOOL_SHELL_OK)],
        ToolShell::NoToken => {
            vec![reply(
                "⏺ Shell(ls /talaria-no-such-dir)  ✗ exit 1 · 0.0s\nNo such file or directory",
            )]
        }
        ToolShell::NoHeader => vec![reply("agent: $1-$2")],
        ToolShell::NoOutput => {
            vec![reply(
                "⏺ Shell(ls /talaria-no-such-dir)  ✗ exit 1 · 0.0s\nagent: $1-$2",
            )]
        }
        ToolShell::OtherCmd => vec![reply(
            "⏺ Shell(ls /other)  ✗ exit 2 · 0.0s\nls: /other: No such file or directory\nagent: $1-$2",
        )],
    }
}

/// The tool-edit prompts (token C, then a new token on retry) on the first pane.
pub fn tool_edit_rules(shape: ToolEdit) -> Vec<RuleBuilder> {
    let first = || {
        RuleBuilder::run(&format!(
            "beta to gamma\\. Use your file editing tool, then {PROMPT_TAIL}"
        ))
        .append("you: $0")
        .churn(4)
    };
    let retry = || {
        RuleBuilder::run(&format!("Use your file editing tool now.*{PROMPT_TAIL}"))
            .append("you: $0")
            .churn(4)
    };
    let screen = "⏺ Edit notes.txt  ✓ +1 -1 · 0.1s\n    1   alpha\n    2 - beta\n    2 + gamma\nagent: $1-$2";
    let write_gamma = || {
        StageBuilder::new()
            .after_polls(4)
            .append(screen)
            .write_in_cwd("notes.txt", "alpha\ngamma\n")
    };
    match shape {
        ToolEdit::Happy => vec![first().then(write_gamma())],
        ToolEdit::Debris => vec![first().then(
            StageBuilder::new()
                .after_polls(4)
                .append("⏺ Edit notes.txt  ✓ +1 -1 · 0.1s\n    2 - beta ┊ debris\n    2 + gamma\nagent: $1-$2")
                .write_in_cwd("notes.txt", "alpha\ngamma\n"),
        )],
        ToolEdit::NoStat => vec![first().then(
            StageBuilder::new()
                .after_polls(4)
                .append("⏺ Edit notes.txt · 0.1s\n    2 - beta\n    2 + gamma\nagent: $1-$2")
                .write_in_cwd("notes.txt", "alpha\ngamma\n"),
        )],
        ToolEdit::NoDiffRows => vec![first().then(
            StageBuilder::new()
                .after_polls(4)
                .append("⏺ Edit notes.txt  ✓ +1 -1 · 0.1s\nagent: $1-$2")
                .write_in_cwd("notes.txt", "alpha\ngamma\n"),
        )],
        ToolEdit::FileUnchanged => vec![first().then(
            StageBuilder::new()
                .after_polls(4)
                .append(screen)
                .write_in_cwd("notes.txt", "alpha\nbeta\n"),
        )],
        ToolEdit::RetryShell => vec![
            first().then(
                StageBuilder::new()
                    .after_polls(4)
                    .append("⏺ Shell(sed -i s/beta/gamma/ notes.txt)  ✓ · 0.0s\nagent: $1-$2")
                    .write_in_cwd("notes.txt", "alpha\ngamma\n"),
            ),
            retry().then(write_gamma()),
        ],
        ToolEdit::NoEditToolTwice => vec![
            first().then(
                StageBuilder::new().after_polls(4).append(
                    "⏺ Shell(sed -i s/beta/gamma/ notes.txt)  ✓ · 0.0s\nagent: $1-$2",
                ),
            ),
            retry().then(
                StageBuilder::new().after_polls(4).append(
                    "⏺ Shell(sed -i s/beta/gamma/ notes.txt)  ✓ · 0.0s\nagent: $1-$2",
                ),
            ),
        ],
        ToolEdit::NoToken => vec![first().then(
            StageBuilder::new()
                .after_polls(4)
                .append("⏺ Edit notes.txt  ✓ +1 -1 · 0.1s\n    2 - beta\n    2 + gamma\nagent: done")
                .write_in_cwd("notes.txt", "alpha\ngamma\n"),
        )],
    }
}

/// The palette keys on the first pane. The trailing Ctrl+C rule absorbs the close keys so
/// they never reach a later step's rules.
pub fn palette_rules(shape: Palette) -> Vec<RuleBuilder> {
    let vers = |list: &str| RuleBuilder::send_text("^/vers$").append(list);
    let enter = |block: &str| RuleBuilder::send_keys("^enter$").append(block);
    let browse = |list: &str| RuleBuilder::send_text("^/$").append(list);
    let esc = RuleBuilder::send_keys("^esc$").clear().append(">");
    let ctrl_c = || RuleBuilder::send_keys("^ctrl\\+c$");
    let group = "Talaria\n/quit  x  talaria\n/btw  x  talaria\n/skin  x  talaria\n/theme  x  talaria\n↑↓ choose · Tab complete · Enter run · Esc close  50 of 50";
    match shape {
        Palette::Happy => vec![
            vers(
                "/version  Show version information  talaria\n↑↓ choose · Tab complete · Enter run · Esc close  1 of 1",
            ),
            enter("/version\ntalaria version test"),
            browse(
                "Talaria\n/quit  Leave Talaria; the session keeps running in Hermes  talaria\n/btw  Ask a side question  talaria\n/skin  Use a Hermes skin  talaria\n/theme  Show the theme report  talaria\n↑↓ choose · Tab complete · Enter run · Esc close  50 of 50",
            ),
            esc,
            ctrl_c(),
        ],
        Palette::NotOpen => vec![
            vers("↑↓ choose · Tab complete · Enter run · Esc close  0 of 50"),
            enter("/version\ntalaria version test"),
            browse(group),
            esc.clone(),
            ctrl_c(),
        ],
        Palette::NoBlock => vec![
            vers(
                "/version  Show version information  talaria\n↑↓ choose · Tab complete · Enter run · Esc close  1 of 1",
            ),
            RuleBuilder::send_keys("^enter$"),
            browse(group),
            esc.clone(),
            ctrl_c(),
        ],
        Palette::GroupLacks => vec![
            vers(
                "/version  Show version information  talaria\n↑↓ choose · Tab complete · Enter run · Esc close  1 of 1",
            ),
            enter("/version\ntalaria version test"),
            browse(
                "Talaria\n/quit  x  talaria\n/btw  x  talaria\n↑↓ choose · Tab complete · Enter run · Esc close  50 of 50",
            ),
            esc.clone(),
            ctrl_c(),
        ],
        Palette::NotClosed => vec![
            vers(
                "/version  Show version information  talaria\n↑↓ choose · Tab complete · Enter run · Esc close  1 of 1",
            ),
            enter("/version\ntalaria version test"),
            browse(
                "Talaria\n/quit  x  talaria\n/btw  x  talaria\n/skin  x  talaria\n/theme  x  talaria\n↑↓ choose · Tab complete · Enter run · Esc close  50 of 50",
            ),
            RuleBuilder::send_keys("^esc$"),
            ctrl_c(),
        ],
    }
}

/// The mention keys on the first pane. The reply arrives after Enter, as in the scenario.
pub fn mention_rules(shape: Mention) -> Vec<RuleBuilder> {
    // The step types the query without its last character, then that character alone.
    let list = |rows: &str| RuleBuilder::send_text("^c$").append(rows);
    let tab = |composer: &str| RuleBuilder::send_keys("^tab$").append(composer);
    let question = || {
        RuleBuilder::send_text("^ in one sentence$")
            .append("> Summarize @file:src/calc.py  in one sentence")
    };
    let enter = |reply: &str| {
        RuleBuilder::send_keys("^enter$")
            .churn(4)
            .then(StageBuilder::new().after_polls(4).append(reply))
    };
    let happy_list = "> Summarize @calc\ncalc.py  src\ncalculator.py  src";
    let happy_tab = "> Summarize @file:src/calc.py ";
    let happy_reply = "agent: The function computes the average of the values.";
    let mut rules = match shape {
        Mention::Happy => vec![
            list(happy_list),
            tab(happy_tab),
            question(),
            enter(happy_reply),
        ],
        Mention::NoRow => vec![
            list("> Summarize @calc\ncalculator.py  src"),
            tab(happy_tab),
            question(),
            enter(happy_reply),
        ],
        Mention::TabNothing => vec![
            list(happy_list),
            RuleBuilder::send_keys("^tab$"),
            question(),
            enter(happy_reply),
        ],
        Mention::ReplyMisses => vec![
            list(happy_list),
            tab(happy_tab),
            question(),
            enter("agent: Done."),
        ],
    };
    rules.insert(0, RuleBuilder::send_text("^Summarize @cal$"));
    rules
}

/// The shell lines on the first pane. The cells appear on the typed text and bare `enter`
/// rules absorb the confirming keys.
pub fn shell_rules(shape: Shell) -> Vec<RuleBuilder> {
    let echo = |cell: &str| RuleBuilder::send_text("^!echo (.+)$").append(cell);
    let rm = |row: &str| RuleBuilder::send_text("^!rm -r \\./keepme$").append(row);
    let enter = || RuleBuilder::send_keys("^enter$");
    let refusal = "Hermes refused to run this: it looks dangerous";
    match shape {
        Shell::Happy => vec![echo("! echo $1\n$1"), enter(), rm(refusal), enter()],
        Shell::NoOutput => vec![echo("! echo $1"), enter(), rm(refusal), enter()],
        Shell::NoRefusal => vec![
            echo("! echo $1\n$1"),
            enter(),
            RuleBuilder::send_text("^!rm -r \\./keepme$"),
            enter(),
        ],
        Shell::FolderRemoved => vec![
            echo("! echo $1\n$1"),
            enter(),
            RuleBuilder::send_text("^!rm -r \\./keepme$")
                .append("Hermes refused to run this: it looks dangerous")
                .remove_in_cwd("keepme"),
            enter(),
        ],
    }
}

/// The side question on the first pane.
pub fn btw_rules(shape: Btw) -> Vec<RuleBuilder> {
    let ask = |screen: &str| {
        RuleBuilder::run("^/btw which two words")
            .append("you: $0")
            .churn(2)
            .then(StageBuilder::new().after_polls(2).append(screen))
    };
    match shape {
        Btw::Happy => vec![ask(
            "btw · which two words did I ask you to join in my first message?\nThe words are ${mem:0}.\nside answer · not part of the conversation",
        )],
        Btw::NoCell => vec![ask("agent: I cannot reach the side channel.")],
        Btw::Empty => vec![ask(
            "btw · which two words did I ask you to join in my first message?\n(no answer)\nside answer · not part of the conversation",
        )],
        Btw::Failed => vec![ask(
            "btw · which two words did I ask you to join in my first message?\nThe side question failed: model busy",
        )],
        Btw::LacksWord => vec![ask(
            "btw · which two words did I ask you to join in my first message?\nThe words are ${mem:1}.\nside answer · not part of the conversation",
        )],
    }
}

/// The pager keys on the first pane. Opening clears to the pager frame; `q` clears back to
/// the inline view, so the history step starts with no prompt line in sight.
pub fn pager_rules(shape: Pager) -> Vec<RuleBuilder> {
    let body = "› Reply with the word heron in capital letters and nothing else.\nHERON\nagent: ${mem:0}\nNo such file or directory\nready 1\nready 2";
    let open = |screen: &str| RuleBuilder::send_keys("^ctrl\\+o$").clear().append(screen);
    let slash = || RuleBuilder::send_text("^/$").append("/");
    let token_text = || RuleBuilder::send_text("^(\\w+)-(\\d+)$").append("search: $0");
    let tool_text = || RuleBuilder::send_text("^No such file$").append("search: $0");
    let search = |footer: &str| RuleBuilder::send_keys("^enter$").append(footer);
    let quit = |screen: &str| RuleBuilder::send_keys("^q$").clear().append(screen);
    let frame = format!(" Transcript · fixture · 12 cells  idle\n{body}\ncell 12/12 · following");
    let token_footer = "\"${mem:0}\" 1 of 1 · n N · Esc clears";
    let tool_footer = "\"No such file\" 1 of 1 · n N · Esc clears";
    match shape {
        Pager::Happy => vec![
            open(&frame),
            slash(),
            token_text(),
            search(token_footer),
            slash(),
            tool_text(),
            search(tool_footer),
            quit("> "),
        ],
        Pager::NotOpen => vec![
            RuleBuilder::send_keys("^ctrl\\+o$"),
            slash(),
            token_text(),
            search(token_footer),
            slash(),
            tool_text(),
            search(tool_footer),
            quit("> "),
        ],
        Pager::NotFollowing => vec![
            open(" Transcript · fixture · 12 cells  idle\ncell 12/12"),
            slash(),
            token_text(),
            search(token_footer),
            slash(),
            tool_text(),
            search(tool_footer),
            quit("> "),
        ],
        Pager::TokenNoMatch => vec![
            open(&frame),
            slash(),
            token_text(),
            search("no match for \"${mem:0}\" · Esc clears"),
            slash(),
            tool_text(),
            search(tool_footer),
            quit("> "),
        ],
        Pager::ToolNoMatch => vec![
            open(&frame),
            slash(),
            token_text(),
            search(token_footer),
            slash(),
            tool_text(),
            search("no match for \"No such file\" · Esc clears"),
            quit("> "),
        ],
        Pager::QStuck => vec![
            open(&frame),
            slash(),
            token_text(),
            search(token_footer),
            slash(),
            tool_text(),
            search(tool_footer),
            RuleBuilder::send_keys("^q$"),
        ],
        Pager::Interrupt => vec![
            open(&frame),
            slash(),
            token_text(),
            search(token_footer),
            slash(),
            tool_text(),
            search(tool_footer),
            quit("Interrupted\n> "),
        ],
    }
}

/// The history keys on the first pane. The query is remembered, so the loaded composer can
/// quote it after the box is cleared.
pub fn history_rules(shape: History) -> Vec<RuleBuilder> {
    let open = || RuleBuilder::send_keys("^ctrl\\+r$").append("search: ");
    let query = |screen: &str| {
        RuleBuilder::send_text("^.+ and .+$")
            .remember("$0")
            .append(screen)
    };
    let enter = |screen: &str| RuleBuilder::send_keys("^enter$").clear().append(screen);
    match shape {
        History::Happy => vec![
            open(),
            query("match: $0"),
            enter("> ${mem:last}"),
            RuleBuilder::send_keys("^ctrl\\+c$").clear(),
        ],
        History::EmptyBox => vec![
            open(),
            query("match: nothing"),
            enter("> ${mem:last}"),
            RuleBuilder::send_keys("^ctrl\\+c$").clear(),
        ],
        History::LoadFail => vec![
            open(),
            query("match: $0"),
            enter(""),
            RuleBuilder::send_keys("^ctrl\\+c$").clear(),
        ],
        History::ClearFail => vec![
            open(),
            query("match: $0"),
            enter("> ${mem:last}"),
            RuleBuilder::send_keys("^ctrl\\+c$"),
        ],
    }
}

/// The editor keys on the first pane.
pub fn editor_rules(shape: Editor) -> Vec<RuleBuilder> {
    let draft = || RuleBuilder::send_text("^draft-one$").append("> draft-one");
    let edit = |screen: &str| RuleBuilder::send_keys("^ctrl\\+g$").append(screen);
    let clear = || RuleBuilder::send_keys("^ctrl\\+c$").clear();
    match shape {
        Editor::Happy => vec![draft(), edit("> draft-one plus-two"), clear()],
        Editor::DraftUnchanged => vec![draft(), RuleBuilder::send_keys("^ctrl\\+g$"), clear()],
        Editor::Sent => vec![
            draft(),
            edit("> draft-one plus-two\nagent: rogue reply"),
            clear(),
        ],
    }
}

/// The theme submissions on the first pane.
pub fn theme_rules(shape: Theme) -> Vec<RuleBuilder> {
    let report = "/theme\nTheme\nSkin: default\nBackground: dark (set with /theme)\nColours: off (NO_COLOR)\nMotion: full (default)";
    let theme = |screen: &str| RuleBuilder::run("^/theme$").append(screen);
    let skin = |screen: &str| RuleBuilder::run("^/skin \\.\\./etc$").append(screen);
    let argument = |screen: &str| RuleBuilder::run("^/theme purple$").append(screen);
    let skin_usage = "/skin ../etc\nUsage: /skin [name]. A skin name is letters, digits, dot, dash or underscore, up to 64 characters.";
    let argument_usage = "/theme purple\nUsage: /theme [light|dark|auto]";
    match shape {
        Theme::Happy => vec![theme(report), skin(skin_usage), argument(argument_usage)],
        Theme::ReportMissing => vec![
            theme("/theme\nTheme\nSkin: default\nBackground: dark (set with /theme)"),
            skin(skin_usage),
            argument(argument_usage),
        ],
        Theme::SkinUsage => vec![
            theme(report),
            RuleBuilder::run("^/skin \\.\\./etc$"),
            argument(argument_usage),
        ],
        Theme::ArgUsage => vec![
            theme(report),
            skin(skin_usage),
            RuleBuilder::run("^/theme purple$"),
        ],
        Theme::Refused => vec![
            theme(report),
            skin(
                "/skin ../etc\nUsage: /skin [name].\nHermes refused to change the skin (invalid_params).",
            ),
            argument(argument_usage),
        ],
    }
}

/// The count prompt on the first pane. The counting lines mirror the first suite's.
pub fn pane_state_rules(shape: PaneState) -> Vec<RuleBuilder> {
    let count = |reads: u32, working: bool| {
        let rule = RuleBuilder::run(COUNT_PATTERN)
            .append("you: $0")
            .churn(reads)
            .churn_lines("{n}", 1, 1);
        if working {
            rule.get("talaria\nworking")
        } else {
            rule
        }
    };
    let esc = |idle: bool| {
        let rule = RuleBuilder::send_keys("^esc$")
            .stop_churn()
            .append("count stopped");
        if idle {
            rule.get("talaria\nidle")
        } else {
            rule
        }
    };
    match shape {
        PaneState::Happy => vec![count(100_000, true), esc(true)],
        PaneState::Counting => vec![count(10, true), esc(true)],
        PaneState::Working => vec![count(100_000, false), esc(true)],
        PaneState::Idle => vec![count(100_000, true), esc(false)],
    }
}

/// The checklist prompts on the first pane. The turn ends `end_polls` reads after the panel
/// appears; callers that run in real time pass a small count.
pub fn checklist_rules(shape: Checklist, end_polls: u32) -> Vec<RuleBuilder> {
    const PANEL: &str = "▾ Tasks 1/3 · Ctrl+T to collapse\n  [x] create a.txt\n  [>] run sleep 30\n  [ ] create b.txt";
    const COLLAPSED: &str = "▸ Tasks 1/3 · run sleep 30 · Ctrl+T to expand";
    const RECORD: &str =
        "Tasks 3/3 · done\n  [x] create a.txt\n  [x] run sleep 30\n  [x] create b.txt";
    let prompt = || RuleBuilder::run(PLAN_PATTERN).append("you: $0").churn(4);
    let end = |screen: &str| {
        prompt()
            .then(StageBuilder::new().after_polls(4).append(PANEL))
            .then(
                StageBuilder::new()
                    .after_polls(end_polls)
                    .clear()
                    .append(screen),
            )
    };
    let happy_end = format!("agent: done\n{RECORD}");
    let collapse = || {
        RuleBuilder::send_keys("^ctrl\\+t$")
            .clear()
            .append(COLLAPSED)
    };
    let expand = || RuleBuilder::send_keys("^ctrl\\+t$").clear().append(PANEL);
    match shape {
        Checklist::Happy => vec![end(&happy_end), collapse(), expand()],
        Checklist::SkipTwice => vec![
            prompt().then(StageBuilder::new().after_polls(4).append("agent: done")),
            prompt().then(StageBuilder::new().after_polls(4).append("agent: done")),
        ],
        // Ctrl+T leaves the panel as it is.
        Checklist::CollapseFail => vec![
            prompt().then(StageBuilder::new().after_polls(4).append(PANEL)),
            RuleBuilder::send_keys("^ctrl\\+t$"),
            expand(),
        ],
        Checklist::ExpandFail => vec![
            end(&happy_end),
            collapse(),
            RuleBuilder::send_keys("^ctrl\\+t$"),
        ],
        Checklist::EndFail => vec![
            prompt().then(StageBuilder::new().after_polls(4).append(PANEL)),
            collapse(),
            expand(),
        ],
        Checklist::PanelLeft => vec![
            prompt()
                .then(StageBuilder::new().after_polls(4).append(PANEL))
                .then(
                    StageBuilder::new()
                        .after_polls(end_polls)
                        .append("agent: done\nTasks 3/3 · done"),
                ),
            collapse(),
            expand(),
        ],
        Checklist::TwoRecords => vec![
            end("agent: done\nTasks 3/3 · done\nTasks 2/3 · 1 not finished"),
            collapse(),
            expand(),
        ],
    }
}

/// The quit submission on the first pane.
pub fn quit_rules() -> Vec<RuleBuilder> {
    vec![
        RuleBuilder::run("^/quit$")
            .foreground(&[])
            .append("you: /quit\nSession detached.\nSMOKE_EXIT:0\n$ "),
    ]
}

/// The resumed session on the split pane: the launch transcript, the reduced pager check,
/// the third prompt and the final quit.
pub fn resume_rules(shape: Resume) -> Vec<RuleBuilder> {
    let transcript = "$0\n── Resumed:\nresumed session\n${mem:0}\n${mem:1}\nTasks 3/3 · done (from before this resume)";
    let launch = |screen: &str| {
        RuleBuilder::run(RESUME_RUN)
            .foreground(&["-bash", "$0"])
            .append(screen)
    };
    let open = || {
        RuleBuilder::send_keys("^ctrl\\+o$").clear().append(
            " Transcript · fixture · 12 cells  idle\nagent: ${mem:0}\ncell 12/12 · following",
        )
    };
    let slash = || RuleBuilder::send_text("^/$").append("/");
    let token_text = || RuleBuilder::send_text("^(\\w+)-(\\d+)$").append("search: $0");
    let search = |footer: &str| RuleBuilder::send_keys("^enter$").append(footer);
    let quit = || RuleBuilder::send_keys("^q$").clear().append("> ");
    let prompt_c = || {
        RuleBuilder::run(PROMPT_TAIL)
            .append("you: $0")
            .remember("$1-$2")
            .churn(4)
            .then(StageBuilder::new().after_polls(4).append("agent: $1-$2"))
    };
    let final_quit = || {
        RuleBuilder::run("^/quit$")
            .foreground(&[])
            .append("you: /quit\nSession detached.\nSMOKE_EXIT:0\n$ ")
    };
    let footer = "\"${mem:0}\" 1 of 2 · n N · Esc clears";
    // The pager rules stay first after the launch; the tmux section's rules follow them, and
    // the split that reopens the pane resets the used flags, so the resumed search rule may
    // fire once more on a later `enter` (a harmless extra footer).
    let pager =
        |search_footer: &str| vec![open(), slash(), token_text(), search(search_footer), quit()];
    let tail = |search_footer: &str| {
        let mut rules = pager(search_footer);
        rules.push(prompt_c());
        rules.push(final_quit());
        rules
    };
    match shape {
        Resume::Happy => {
            let mut rules = vec![launch(transcript)];
            rules.extend(tail(footer));
            rules
        }
        Resume::LocalCell => {
            let mut rules = vec![launch(&format!(
                "{transcript}\nbtw · what code word did I give you?"
            ))];
            rules.extend(tail(footer));
            rules
        }
        Resume::RecordTwice => {
            let mut rules = vec![launch(&format!(
                "{transcript}\nTasks 2/3 · 1 not finished (from before this resume)"
            ))];
            rules.extend(tail(footer));
            rules
        }
        Resume::PagerFail => {
            let mut rules = vec![launch(transcript)];
            rules.extend(tail("no match for \"${mem:0}\" · Esc clears"));
            rules
        }
    }
}

/// The tmux launch on the split pane. All shapes share the rules; the failing shapes are
/// driven by scripted paster answers in the test.
pub fn tmux_pane_rules(_shape: TmuxPane) -> Vec<RuleBuilder> {
    vec![
        RuleBuilder::run("tmux new-session")
            .foreground(&["-bash", "$0"])
            .append("$0\nTalaria\nConnecting to Hermes"),
    ]
}

/// The collapsing paste on the split pane. The reply is appended by the question rule, so
/// a reopened pane that refires an earlier `enter` rule cannot swallow it.
pub fn paste_collapse_rules(shape: PasteCollapse) -> Vec<RuleBuilder> {
    let paste = |screen: &str| {
        RuleBuilder::paste("the marker is (\\w+)-(\\d+)")
            .remember("$1-$2")
            .append(screen)
    };
    let question = |reply: &str| {
        RuleBuilder::send_text("marker on line 87")
            .append("> What is the marker on line 87? Reply with only the marker word.")
            .churn(4)
            .then(StageBuilder::new().after_polls(4).append(reply))
    };
    let enter = || RuleBuilder::send_keys("^enter$");
    match shape {
        PasteCollapse::Happy => vec![
            paste("[Pasted text #1 +120 lines]"),
            question("agent: ${mem:last}"),
            enter(),
        ],
        PasteCollapse::NoLabel => vec![
            paste("pasted, but nothing to show"),
            question("agent: ${mem:last}"),
            enter(),
        ],
        PasteCollapse::Leak => vec![
            paste("[Pasted text #1 +120 lines]\nthe marker is $1-$2"),
            question("agent: ${mem:last}"),
            enter(),
        ],
        PasteCollapse::NoReply => vec![
            paste("[Pasted text #1 +120 lines]"),
            question("agent: done"),
            enter(),
        ],
    }
}

/// The dropped file on the split pane. The fake paster notes the secret token when the
/// path is pasted, so the reply rule quotes the last remembered value.
pub fn file_drop_rules(shape: FileDrop) -> Vec<RuleBuilder> {
    let paste = |screen: &str| RuleBuilder::paste("notes-([0-9a-f]+\\.txt)").append(screen);
    let question = |reply: &str| {
        RuleBuilder::send_text("secret word")
            .append("> What is the secret word? Reply with only that word.")
            .churn(4)
            .then(StageBuilder::new().after_polls(4).append(reply))
    };
    let enter = || RuleBuilder::send_keys("^enter$");
    match shape {
        FileDrop::Happy => vec![
            paste("[File #1: notes-$1]"),
            question("agent: ${mem:last}"),
            enter(),
        ],
        FileDrop::NoLabel => vec![
            paste("pasted, but nothing to show"),
            question("agent: ${mem:last}"),
            enter(),
        ],
        FileDrop::NoReply => vec![
            paste("[File #1: notes-$1]"),
            question("agent: done"),
            enter(),
        ],
    }
}

/// The second paste through the external editor on the split pane.
pub fn paste_editor_rules(shape: PasteEditor) -> Vec<RuleBuilder> {
    let paste = |screen: &str| {
        RuleBuilder::paste("the marker is (\\w+)-(\\d+)")
            .remember("$1-$2")
            .append(screen)
    };
    let edit = |screen: &str| RuleBuilder::send_keys("^ctrl\\+g$").append(screen);
    let question = |reply: &str| {
        RuleBuilder::send_text("marker on line 87 of the paste")
            .append(
                "> What is the marker on line 87 of the paste? Reply with only the marker word.",
            )
            .churn(4)
            .then(StageBuilder::new().after_polls(4).append(reply))
    };
    let enter = || RuleBuilder::send_keys("^enter$");
    match shape {
        PasteEditor::Happy => vec![
            paste("[Pasted text #2 +120 lines]"),
            edit("[Pasted text #2 +120 lines] plus-two"),
            question("agent: ${mem:last}"),
            enter(),
        ],
        PasteEditor::NoLabel => vec![
            paste("pasted, but nothing to show"),
            edit("[Pasted text #2 +120 lines] plus-two"),
            question("agent: ${mem:last}"),
            enter(),
        ],
        PasteEditor::LabelLost => vec![
            paste("[Pasted text #2 +120 lines]"),
            edit("plus-two"),
            question("agent: ${mem:last}"),
            enter(),
        ],
        PasteEditor::NoReply => vec![
            paste("[Pasted text #2 +120 lines]"),
            edit("[Pasted text #2 +120 lines] plus-two"),
            question("agent: done"),
            enter(),
        ],
    }
}

/// The quit inside the tmux session on the split pane. The marker is written when `/quit`
/// is typed, so the exit code is set however the confirming `enter` is absorbed.
pub fn tmux_quit_rules(shape: TmuxQuit) -> Vec<RuleBuilder> {
    let quit = |marker: Option<&str>| {
        let rule = RuleBuilder::send_text("^/quit$").append("> /quit");
        let rule = if let Some(code) = marker {
            rule.write_in_cwd("tmux-exit.txt", code)
        } else {
            rule
        };
        rule.foreground(&[])
            .append("Session detached.\nSMOKE_EXIT:0\n$ ")
    };
    let enter = || RuleBuilder::send_keys("^enter$");
    match shape {
        TmuxQuit::Happy => vec![quit(Some("SMOKE_EXIT:0")), enter()],
        TmuxQuit::ExitWrong => vec![quit(Some("SMOKE_EXIT:7")), enter()],
        TmuxQuit::SessionStuck => vec![quit(None), enter()],
    }
}

/// The steps of the phase 2a run, in order.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Step {
    AnswerA,
    ToolShell,
    ToolEdit,
    Palette,
    Mention,
    Shell,
    Btw,
    Pager,
    History,
    Editor,
    Theme,
    PaneState,
    Checklist,
    Quit,
    Resume,
    TmuxPane,
    PasteCollapse,
    FileDrop,
    PasteEditor,
    TmuxQuit,
}

/// The shape of every step of a script; all default to the passing shape.
#[derive(Default)]
pub struct Shapes {
    /// The startup shape.
    pub startup: Startup,
    /// The tool-shell shape.
    pub tool_shell: ToolShell,
    /// The tool-edit shape.
    pub tool_edit: ToolEdit,
    /// The palette shape.
    pub palette: Palette,
    /// The mention shape.
    pub mention: Mention,
    /// The shell shape.
    pub shell: Shell,
    /// The side-answer shape.
    pub btw: Btw,
    /// The pager shape.
    pub pager: Pager,
    /// The history shape.
    pub history: History,
    /// The editor shape.
    pub editor: Editor,
    /// The theme shape.
    pub theme: Theme,
    /// The pane-state shape.
    pub pane_state: PaneState,
    /// The checklist shape.
    pub checklist: Checklist,
    /// The resume shape.
    pub resume: Resume,
    /// The tmux-pane shape.
    pub tmux_pane: TmuxPane,
    /// The paste-collapse shape.
    pub paste_collapse: PasteCollapse,
    /// The file-drop shape.
    pub file_drop: FileDrop,
    /// The paste-editor shape.
    pub paste_editor: PasteEditor,
    /// The tmux-quit shape.
    pub tmux_quit: TmuxQuit,
    /// Reads after the panel before the checklist turn ends; 400 when unset.
    pub checklist_end_polls: Option<u32>,
}

/// A script with a passing run-up to `stop` and the shapes of `shapes` at and after it.
/// Steps after `stop` have no rules, so the run ends at the first of them.
pub fn script_2a(shapes: &Shapes, stop: Step) -> ScriptBuilder {
    let mut script = ScriptBuilder::new();
    let end_polls = shapes.checklist_end_polls.unwrap_or(400);
    let mut first: Vec<RuleBuilder> = Vec::new();
    first.extend(startup_rules(shapes.startup));
    first.extend(answer_a_rules());
    if stop >= Step::ToolShell {
        first.extend(tool_shell_rules(shapes.tool_shell));
    }
    if stop >= Step::ToolEdit {
        first.extend(tool_edit_rules(shapes.tool_edit));
    }
    if stop >= Step::Palette {
        first.extend(palette_rules(shapes.palette));
    }
    if stop >= Step::Mention {
        first.extend(mention_rules(shapes.mention));
    }
    if stop >= Step::Shell {
        first.extend(shell_rules(shapes.shell));
    }
    if stop >= Step::Btw {
        first.extend(btw_rules(shapes.btw));
    }
    if stop >= Step::Pager {
        first.extend(pager_rules(shapes.pager));
    }
    if stop >= Step::History {
        first.extend(history_rules(shapes.history));
    }
    if stop >= Step::Editor {
        first.extend(editor_rules(shapes.editor));
    }
    if stop >= Step::Theme {
        first.extend(theme_rules(shapes.theme));
    }
    if stop >= Step::PaneState {
        first.extend(pane_state_rules(shapes.pane_state));
    }
    if stop >= Step::Checklist {
        first.extend(checklist_rules(shapes.checklist, end_polls));
    }
    if stop >= Step::Quit {
        first.extend(quit_rules());
    }
    let mut second: Vec<RuleBuilder> = Vec::new();
    if stop >= Step::Resume {
        second.extend(resume_rules(shapes.resume));
    }
    if stop >= Step::TmuxPane {
        second.extend(tmux_pane_rules(shapes.tmux_pane));
    }
    if stop >= Step::PasteCollapse {
        second.extend(paste_collapse_rules(shapes.paste_collapse));
    }
    if stop >= Step::FileDrop {
        second.extend(file_drop_rules(shapes.file_drop));
    }
    if stop >= Step::PasteEditor {
        second.extend(paste_editor_rules(shapes.paste_editor));
    }
    if stop >= Step::TmuxQuit {
        second.extend(tmux_quit_rules(shapes.tmux_quit));
    }
    for rule in first {
        script = script.on_first(rule);
    }
    for rule in second {
        script = script.on_second(rule);
    }
    script
}

// ---- running phase 2a scripts in process ----

/// A finished in-process phase 2a run: the report, the terminal and the paster calls.
pub struct Exec2a {
    /// The run's report.
    pub report: SmokeReport,
    /// The terminal the run drove.
    pub term: FakeTerminal<'static>,
    /// The paster calls the run made, oldest first.
    pub paster_calls: Vec<PasterRecorded>,
    /// The temporary root the run used.
    pub root: PathBuf,
    _temp: tempfile::TempDir,
}

/// Runs `run_smoke_2a` against the script with a manual clock, a seeded config with the
/// phase 2a deadline and a fake paster sharing the terminal's machine.
pub fn exec_2a(
    script: &ScriptBuilder,
    tune_cfg: impl FnOnce(&mut SmokeConfig),
    tune_term: impl FnOnce(FakeTerminal<'static>) -> FakeTerminal<'static>,
    tune_paster: impl FnOnce(&FakePaster),
) -> Exec2a {
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
        deadline: Duration::from_secs(1800),
        step_scale: 1.0,
        report_dir: None,
        require: false,
        temp_root: root.clone(),
        seed: Some(11),
    };
    tune_cfg(&mut cfg);
    let cfg2 = Phase2aConfig {
        tmux: PathBuf::from("tmux"),
    };
    let term = tune_term(FakeTerminal::from_builder(script, clock));
    let paster = FakePaster::new(&term, clock, &cfg.temp_root);
    tune_paster(&paster);
    let stop = AtomicBool::new(false);
    let report = run_smoke_2a(&term, &paster, clock, &cfg, &cfg2, &stop);
    let paster_calls = paster.calls();
    Exec2a {
        report,
        term,
        paster_calls,
        root: cfg.temp_root.clone(),
        _temp: temp,
    }
}

impl Exec2a {
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

    /// Every killed session, oldest first.
    pub fn kills(&self) -> Vec<String> {
        use super::PasterCall;
        self.paster_calls
            .iter()
            .filter_map(|recorded| match &recorded.call {
                PasterCall::KillSession { session } => Some(session.clone()),
                _ => None,
            })
            .collect()
    }

    /// Every pasted `(session, text)` pair, oldest first.
    pub fn pastes(&self) -> Vec<(String, String)> {
        use super::PasterCall;
        self.paster_calls
            .iter()
            .filter_map(|recorded| match &recorded.call {
                PasterCall::Paste { session, text } => Some((session.clone(), text.clone())),
                _ => None,
            })
            .collect()
    }
}

/// The keys sent are exactly these, in order (passing scripts only; a failing reply step
/// may also carry the submit fallback `enter`, so failing scripts use [`assert_keys_in_order`]).
pub fn assert_keys_exact(exec: &Exec2a, expected: &[&str]) {
    let actual = exec.sent_keys();
    let expected: Vec<String> = expected.iter().map(ToString::to_string).collect();
    assert_eq!(
        actual,
        expected,
        "the keys sent in order; lines:\n{}",
        exec.lines()
    );
}

/// The keys sent start with these, in order: a passing step's own keys. The run goes on into
/// the next step, which has no rules and may send its first key (or the submit fallback
/// Enter) before it fails.
pub fn assert_keys_start_with(exec: &Exec2a, expected: &[&str]) {
    let actual = exec.sent_keys();
    let expected: Vec<String> = expected.iter().map(ToString::to_string).collect();
    assert!(
        actual.starts_with(&expected),
        "the keys sent start with {expected:?}, got {actual:?}; lines:\n{}",
        exec.lines()
    );
}

/// The keys hold these in order, possibly with fallback keys between them.
pub fn assert_keys_in_order(exec: &Exec2a, expected: &[&str]) {
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
pub fn assert_fails_with(exec: &Exec2a, step: &str, reason: &str) {
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
pub fn assert_passes(exec: &Exec2a, step: &str) {
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
pub fn assert_one_close_tab(exec: &Exec2a) {
    assert_eq!(
        exec.close_tabs(),
        1,
        "one close_tab; lines:\n{}",
        exec.lines()
    );
}
