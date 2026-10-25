//! The phase 2a run on the fake terminal, the fake paster and the manual clock.
//!
//! Each passing script runs a passing run-up to its step and asserts the pass, the keys sent
//! in order and one `close_tab`. Each failing script asserts the printed reason exactly, the
//! keys sent in order, the times and one `close_tab`. Times are asserted two ways: the reason
//! text names the limit in force, and for waits the run is shown to have waited at least the
//! limit since the step's trigger call. All fail until the stub `run_smoke_2a` is replaced.
//!
//! Reason-to-test map (every exact reason, and the test that asserts it):
//!
//! | Function | Reasons asserted |
//! |---|---|
//! | `happy_path_passes_every_step_and_closes_everything` | all 21 `PASS` lines, `SMOKE_RESULT: PASSED`, one `close_tab`, one `kill_session` |
//! | `launch_lines_set_editor_and_no_ensure` | every launch line holds `EDITOR='<scratch>/smoke-editor.sh'` and `--no-ensure` |
//! | `tool_shell_passes`, `tool_shell_fails_without_header` | tool-shell: token missing, no header |
//! | `tool_shell_fails_without_output_row`, `tool_shell_other_command_is_refused` | tool-shell: no output row; no header (a header for `ls /other`) |
//! | `tool_edit_passes_and_checks_the_file`, `tool_edit_retries_once_after_a_shell_edit` | the retry path (two prompts, the second stronger, a new token) |
//! | `tool_edit_fails_on_debris` | tool-edit: debris, no stat, no diff rows |
//! | `tool_edit_fails_when_file_unchanged_twice` | tool-edit: no edit tool in two attempts, file, token missing |
//! | `palette_passes`, `palette_fails_without_footer` | palette: not open, not closed |
//! | `palette_fails_without_version_block`, `palette_fails_when_group_lacks_a_command` | palette: no block; group (names listed in order) |
//! | `mention_passes`, `mention_fails_without_row`, `mention_fails_when_tab_inserts_nothing`, `mention_fails_when_reply_misses_the_word` | mention: list; tab; reply |
//! | `shell_passes`, `shell_fails_without_output`, `shell_refusal_needs_text_and_surviving_folder`, `shell_fails_when_folder_removed` | shell: output; refusal; folder |
//! | `btw_passes`, `btw_fails_without_footer`, `btw_fails_on_failed_question`, `btw_fails_when_answer_lacks_the_word` | btw: no cell and empty; failed; word (an answer naming only token B's word fails) |
//! | `pager_passes`, `pager_fails_when_not_following`, `pager_fails_when_a_search_finds_nothing`, `pager_fails_when_q_does_not_close` | pager: open and follow; token search and tool search (a footer `no match for ...`); close and interrupt (`pager_passes` accepts a footer `"<token>" 1 of 1`) |
//! | `history_search_passes`, `history_search_fails_on_empty_box`, `history_search_fails_when_draft_not_loaded` | history-search: box; load and clear |
//! | `editor_passes`, `editor_fails_when_draft_unchanged` | editor: draft, sent |
//! | `theme_passes`, `theme_fails_without_a_line`, `theme_fails_when_skin_usage_shows_a_refusal` | theme: report, skin, argument; refused |
//! | `pane_state_passes`, `pane_state_skips_when_no_agent_shown`, `pane_state_fails_without_working` | pane-state: skip (result line `PASSED WITH SKIPS (pane-state)`, exit 3); counting, working, idle |
//! | `checklist_passes_with_collapse_and_record`, `checklist_skips_after_two_attempts`, `checklist_fails_when_panel_stays`, `checklist_fails_with_two_records` | checklist: skip (two prompts sent, two settled screens); collapse, expand, end, panel left; records (`2`) |
//! | `resume_checks_local_cells_are_gone`, `resume_fails_when_record_printed_twice` | resume: local cell; record, pager |
//! | `tmux_start_fails_with_exit_code` | tmux-pane: start, exit |
//! | `paste_collapse_passes`, `paste_collapse_fails_without_label` | paste-collapse: label, leak, reply |
//! | `file_drop_passes`, `file_drop_fails_without_label` | file-drop: label, reply |
//! | `paste_editor_passes`, `paste_editor_fails_when_label_lost` | paste-editor: label, editor, reply |
//! | `tmux_quit_needs_exit_zero_and_session_gone` | tmux-quit: exit, session |
//! | `a_failing_step_stops_the_run_and_cleans_up` | later steps print nothing; one `close_tab`, one `kill_session`; any: tmux error, pane get error (one case each) |
//! | `deadline_names_the_step` | the 1800 s deadline, one step past it: `deadline exceeded in step <name>` |
//! | `a_skip_is_never_a_pass`, `live_require_turns_a_skip_into_a_failure` | `PASSED WITH SKIPS`, exit 3; with `TALARIA_LIVE_REQUIRE=1`: `FAIL pane-state (skipped: ...; TALARIA_LIVE_REQUIRE=1)`, exit 1 |
//! | `step_scale_leaves_quiet_periods` | with scale 0.5 the 5 s limits become 3 s (rounded) and the 3 s quiet period stays 3 s |

mod support;

use std::time::Duration;

use smoke::{SmokeConfig, StepOutcome};
use support::scripts2a::{
    Btw, Checklist, Editor, Exec2a, FileDrop, History, Mention, Pager, Palette, PaneState,
    PasteCollapse, PasteEditor, Resume, Shapes, Shell, Startup, Step, Theme, TmuxQuit, ToolEdit,
    ToolShell, assert_fails_with, assert_keys_exact, assert_keys_in_order, assert_keys_start_with,
    assert_one_close_tab, assert_passes, exec_2a, script_2a,
};
use support::{Call, FailBuilder, FakePaster, PasterCall};

// ---- helpers ----

/// The keys each step sends, in run order.
const K_PALETTE: &[&str] = &["enter", "esc", "ctrl+c"];
const K_MENTION: &[&str] = &["tab", "enter"];
const K_SHELL: &[&str] = &["enter", "enter"];
const K_PAGER: &[&str] = &["ctrl+o", "enter", "enter", "q"];
const K_HISTORY: &[&str] = &["ctrl+r", "enter", "ctrl+c"];
const K_EDITOR: &[&str] = &["ctrl+g", "ctrl+c"];
const K_PANE_STATE: &[&str] = &["esc"];
const K_CHECKLIST: &[&str] = &["ctrl+t", "ctrl+t"];
const K_RESUME_PAGER: &[&str] = &["ctrl+o", "enter", "q"];
const K_TMUX: &[&str] = &["enter", "enter", "ctrl+g", "enter", "enter"];

fn keys<'a>(parts: &[&'a [&'a str]]) -> Vec<&'a str> {
    parts.iter().flat_map(|part| part.iter().copied()).collect()
}

const TOKEN_MISSING_90: &str = "the reply token did not appear and settle within 90 s";

/// The no-header reason quotes the cell shape with a literal `<n>`: with no matching cell
/// there is no exit number to print.
const NO_HEADER: &str = "no tool cell '⏺ Shell(ls /talaria-no-such-dir)  ✗ exit <n>' appeared";

/// The pane-state skip reason names its source issue and scenario in words.
const PANE_STATE_SKIP: &str = "herdr pane get shows no agent state for talaria; read the Herdr sidebar by hand (issue #228, pane state scenario)";

fn play(shapes: &Shapes, stop: Step) -> Exec2a {
    let script = script_2a(shapes, stop);
    exec_2a(&script, |_| {}, |term| term, |_| {})
}

fn play_cfg(shapes: &Shapes, stop: Step, tune: impl FnOnce(&mut SmokeConfig)) -> Exec2a {
    let script = script_2a(shapes, stop);
    exec_2a(&script, tune, |term| term, |_| {})
}

fn play_paster(shapes: &Shapes, stop: Step, tune: impl FnOnce(&FakePaster)) -> Exec2a {
    let script = script_2a(shapes, stop);
    exec_2a(&script, |_| {}, |term| term, tune)
}

/// The dropped file's name (`notes-<hex>.txt`), from the path the run pasted: the run
/// removes the drop folder on every path, so the folder itself is gone by now.
fn drop_name(exec: &Exec2a) -> String {
    exec.pastes()
        .iter()
        .rev()
        .find_map(|(_, text)| {
            let name = std::path::Path::new(text).file_name()?.to_str()?;
            (text.starts_with('/') && name.starts_with("notes-")).then(|| name.to_owned())
        })
        .unwrap_or_else(|| panic!("no dropped file was pasted; lines:\n{}", exec.lines()))
}

/// The time of the last terminal call matching the predicate.
fn last_call_at(exec: &Exec2a, want: impl Fn(&Call) -> bool) -> Duration {
    exec.term
        .calls()
        .into_iter()
        .rev()
        .find_map(|recorded| want(&recorded.call).then_some(recorded.at))
        .expect("the trigger call")
}

/// The time of the last paste.
fn last_paste_at(exec: &Exec2a) -> Duration {
    exec.paster_calls
        .iter()
        .rev()
        .find_map(|recorded| {
            matches!(recorded.call, PasterCall::Paste { .. }).then_some(recorded.at)
        })
        .expect("the paste")
}

/// The run waited at least the limit since the trigger call.
fn assert_waited(exec: &Exec2a, trigger: Duration, limit: u64) {
    assert!(
        exec.last_at() >= trigger + Duration::from_secs(limit),
        "the step waited at least {limit} s; lines:\n{}",
        exec.lines()
    );
}

/// The elapsed time of the passing step.
fn pass_elapsed(exec: &Exec2a, step: &str) -> Duration {
    exec.report
        .outcomes
        .iter()
        .find_map(|outcome| match outcome {
            StepOutcome::Pass {
                step: passed,
                elapsed,
            } if *passed == step => Some(*elapsed),
            _ => None,
        })
        .unwrap_or_else(|| panic!("{step} must pass; lines:\n{}", exec.lines()))
}

fn submitted(exec: &Exec2a, needle: &str) -> Vec<String> {
    exec.run_texts()
        .into_iter()
        .filter(|text| text.contains(needle))
        .collect()
}

/// Whether the call sends exactly these keys.
fn is_keys(call: &Call, want: &[&str]) -> bool {
    match call {
        Call::SendKeys { keys, .. } => keys.iter().map(String::as_str).collect::<Vec<_>>() == want,
        _ => false,
    }
}

/// Whether the text is a joined token (`word-number`).
fn is_token_text(text: &str) -> bool {
    let Some(dash) = text.find('-') else {
        return false;
    };
    let (word, number) = text.split_at(dash);
    !word.is_empty()
        && word.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
        && number.len() > 1
        && number[1..].chars().all(|c| c.is_ascii_digit())
}

// ---- the run ----

#[test]
fn happy_path_passes_every_step_and_closes_everything() {
    let exec = play(&Shapes::default(), Step::TmuxQuit);
    assert_eq!(
        exec.kinds(),
        [
            "startup:pass",
            "plain-answer:pass",
            "tool-shell:pass",
            "tool-edit:pass",
            "palette:pass",
            "mention:pass",
            "shell:pass",
            "btw:pass",
            "pager:pass",
            "history-search:pass",
            "editor:pass",
            "theme:pass",
            "pane-state:pass",
            "checklist:pass",
            "quit:pass",
            "resume:pass",
            "tmux-pane:pass",
            "paste-collapse:pass",
            "file-drop:pass",
            "paste-editor:pass",
            "tmux-quit:pass",
        ]
        .map(str::to_owned),
        "all 21 steps pass"
    );
    assert!(
        exec.report
            .result_line()
            .starts_with("SMOKE_RESULT: PASSED"),
        "the result line passes: {}",
        exec.report.result_line()
    );
    assert_eq!(exec.report.exit_code(), 0);
    assert_keys_exact(
        &exec,
        &keys(&[
            K_PALETTE,
            K_MENTION,
            K_SHELL,
            K_PAGER,
            K_HISTORY,
            K_EDITOR,
            K_PANE_STATE,
            K_CHECKLIST,
            K_RESUME_PAGER,
            K_TMUX,
        ]),
    );
    assert_one_close_tab(&exec);
    assert_eq!(exec.kills().len(), 1, "one kill_session");
}

#[test]
fn launch_lines_set_editor_and_no_ensure() {
    let exec = play(&Shapes::default(), Step::TmuxQuit);
    let launches: Vec<String> = exec
        .run_texts()
        .into_iter()
        .filter(|text| text.contains("--no-ensure"))
        .collect();
    assert_eq!(
        launches.len(),
        2,
        "the startup and resume launches: {launches:?}"
    );
    for line in &launches {
        assert!(
            line.contains("--no-ensure") && line.contains("/smoke-editor.sh'"),
            "the launch line sets the editor and never ensures: {line}"
        );
        assert!(
            line.contains("EDITOR='"),
            "the editor rides on the launch line: {line}"
        );
    }
}

// ---- tool-shell ----

#[test]
fn tool_shell_passes() {
    let exec = play(&Shapes::default(), Step::ToolShell);
    assert_passes(&exec, "tool-shell");
    assert_keys_start_with(&exec, &[]);
    assert_one_close_tab(&exec);
}

#[test]
fn tool_shell_fails_without_header() {
    let shapes = Shapes {
        tool_shell: ToolShell::NoToken,
        ..Default::default()
    };
    let exec = play(&shapes, Step::ToolShell);
    assert_fails_with(&exec, "tool-shell", TOKEN_MISSING_90);
    let trigger = last_call_at(
        &exec,
        |call| matches!(call, Call::Run { text, .. } if text.contains("Run the shell command")),
    );
    assert_waited(&exec, trigger, 90);
    assert_one_close_tab(&exec);

    let shapes = Shapes {
        tool_shell: ToolShell::NoHeader,
        ..Default::default()
    };
    let exec = play(&shapes, Step::ToolShell);
    assert_fails_with(&exec, "tool-shell", NO_HEADER);
    assert_one_close_tab(&exec);
}

#[test]
fn tool_shell_fails_without_output_row() {
    let shapes = Shapes {
        tool_shell: ToolShell::NoOutput,
        ..Default::default()
    };
    let exec = play(&shapes, Step::ToolShell);
    assert_fails_with(
        &exec,
        "tool-shell",
        "the failing command's output row 'No such file or directory' is missing",
    );
    assert_one_close_tab(&exec);
}

#[test]
fn tool_shell_other_command_is_refused() {
    let shapes = Shapes {
        tool_shell: ToolShell::OtherCmd,
        ..Default::default()
    };
    let exec = play(&shapes, Step::ToolShell);
    assert_fails_with(&exec, "tool-shell", NO_HEADER);
    assert_one_close_tab(&exec);
}

// ---- tool-edit ----

#[test]
fn tool_edit_passes_and_checks_the_file() {
    let exec = play(&Shapes::default(), Step::ToolEdit);
    assert_passes(&exec, "tool-edit");
    assert_keys_exact(&exec, &[]);
    // The step reads notes.txt on disk itself (`tool_edit_fails_when_file_unchanged_twice`
    // covers its failure); the run removes the scratch project afterwards.
    assert_one_close_tab(&exec);
}

#[test]
fn tool_edit_retries_once_after_a_shell_edit() {
    let shapes = Shapes {
        tool_edit: ToolEdit::RetryShell,
        ..Default::default()
    };
    let exec = play(&shapes, Step::ToolEdit);
    assert_passes(&exec, "tool-edit");
    let prompts = submitted(&exec, "file editing tool");
    assert_eq!(prompts.len(), 2, "two prompts are sent: {prompts:?}");
    assert!(
        prompts[1].contains("Use your file editing tool now"),
        "the second prompt is stronger: {}",
        prompts[1]
    );
    let token = |prompt: &str| {
        let pattern = regex::Regex::new(r"(\w+) and (\d+)").expect("regex");
        let found = pattern.captures(prompt).expect("the token words");
        format!("{}-{}", &found[1], &found[2])
    };
    assert_ne!(
        token(&prompts[0]),
        token(&prompts[1]),
        "the retry draws a new token"
    );
    assert_one_close_tab(&exec);
}

#[test]
fn tool_edit_fails_on_debris() {
    let shapes = Shapes {
        tool_edit: ToolEdit::Debris,
        ..Default::default()
    };
    let exec = play(&shapes, Step::ToolEdit);
    assert_fails_with(
        &exec,
        "tool-edit",
        "the edit cell shows Hermes's diff header or escape debris",
    );
    assert_one_close_tab(&exec);

    let shapes = Shapes {
        tool_edit: ToolEdit::NoStat,
        ..Default::default()
    };
    let exec = play(&shapes, Step::ToolEdit);
    assert_fails_with(&exec, "tool-edit", "the edit cell lacks '✓ +1 -1'");
    assert_one_close_tab(&exec);

    let shapes = Shapes {
        tool_edit: ToolEdit::NoDiffRows,
        ..Default::default()
    };
    let exec = play(&shapes, Step::ToolEdit);
    assert_fails_with(
        &exec,
        "tool-edit",
        "the diff rows '2 - beta' and '2 + gamma' are missing",
    );
    assert_one_close_tab(&exec);
}

#[test]
fn tool_edit_fails_when_file_unchanged_twice() {
    let shapes = Shapes {
        tool_edit: ToolEdit::NoEditToolTwice,
        ..Default::default()
    };
    let exec = play(&shapes, Step::ToolEdit);
    assert_fails_with(
        &exec,
        "tool-edit",
        "the model did not use a file editing tool in two attempts",
    );
    assert_eq!(
        submitted(&exec, "file editing tool").len(),
        2,
        "both attempts are sent"
    );
    assert_one_close_tab(&exec);

    let shapes = Shapes {
        tool_edit: ToolEdit::FileUnchanged,
        ..Default::default()
    };
    let exec = play(&shapes, Step::ToolEdit);
    assert_fails_with(
        &exec,
        "tool-edit",
        "notes.txt does not hold the edit on disk",
    );
    assert_one_close_tab(&exec);

    let shapes = Shapes {
        tool_edit: ToolEdit::NoToken,
        ..Default::default()
    };
    let exec = play(&shapes, Step::ToolEdit);
    assert_fails_with(&exec, "tool-edit", TOKEN_MISSING_90);
    let trigger = last_call_at(
        &exec,
        |call| matches!(call, Call::Run { text, .. } if text.contains("beta to gamma")),
    );
    assert_waited(&exec, trigger, 90);
    assert_one_close_tab(&exec);
}

// ---- palette ----

#[test]
fn palette_passes() {
    let exec = play(&Shapes::default(), Step::Palette);
    assert_passes(&exec, "palette");
    assert_keys_exact(&exec, &keys(&[K_PALETTE]));
    assert_one_close_tab(&exec);
}

#[test]
fn palette_fails_without_footer() {
    let shapes = Shapes {
        palette: Palette::NotOpen,
        ..Default::default()
    };
    let exec = play(&shapes, Step::Palette);
    assert_fails_with(
        &exec,
        "palette",
        "the slash palette did not list /version with its footer within 5 s",
    );
    let trigger = last_call_at(
        &exec,
        |call| matches!(call, Call::SendText { text, .. } if text == "/vers"),
    );
    assert_waited(&exec, trigger, 5);
    assert_one_close_tab(&exec);

    let shapes = Shapes {
        palette: Palette::NotClosed,
        ..Default::default()
    };
    let exec = play(&shapes, Step::Palette);
    assert_fails_with(
        &exec,
        "palette",
        "the palette stayed open after Esc and Ctrl+C",
    );
    assert_keys_in_order(&exec, &["enter", "esc", "ctrl+c"]);
    assert_one_close_tab(&exec);
}

#[test]
fn palette_fails_without_version_block() {
    let shapes = Shapes {
        palette: Palette::NoBlock,
        ..Default::default()
    };
    let exec = play(&shapes, Step::Palette);
    assert_fails_with(
        &exec,
        "palette",
        "/vers and Enter did not print the /version block within 15 s",
    );
    let trigger = last_call_at(&exec, |call| is_keys(call, &["enter"]));
    assert_waited(&exec, trigger, 15);
    assert_keys_in_order(&exec, &["enter"]);
    assert_one_close_tab(&exec);
}

#[test]
fn palette_fails_when_group_lacks_a_command() {
    let shapes = Shapes {
        palette: Palette::GroupLacks,
        ..Default::default()
    };
    let exec = play(&shapes, Step::Palette);
    assert_fails_with(
        &exec,
        "palette",
        "the Talaria group of the palette lacks: /skin, /theme",
    );
    assert_keys_in_order(&exec, &["enter", "esc", "ctrl+c"]);
    assert_one_close_tab(&exec);
}

// ---- mention ----

#[test]
fn mention_passes() {
    let exec = play(&Shapes::default(), Step::Mention);
    assert_passes(&exec, "mention");
    assert_keys_start_with(&exec, &keys(&[K_PALETTE, K_MENTION]));
    assert_one_close_tab(&exec);
}

#[test]
fn mention_fails_without_row() {
    let shapes = Shapes {
        mention: Mention::NoRow,
        ..Default::default()
    };
    let exec = play(&shapes, Step::Mention);
    assert_fails_with(
        &exec,
        "mention",
        "the @ list did not show calc.py with its folder src within 5 s",
    );
    let trigger = last_call_at(
        &exec,
        |call| matches!(call, Call::SendText { text, .. } if text == "c"),
    );
    assert_waited(&exec, trigger, 5);
    assert_one_close_tab(&exec);
}

#[test]
fn mention_fails_when_tab_inserts_nothing() {
    let shapes = Shapes {
        mention: Mention::TabNothing,
        ..Default::default()
    };
    let exec = play(&shapes, Step::Mention);
    assert_fails_with(
        &exec,
        "mention",
        "Tab did not insert @file:src/calc.py within 3 s",
    );
    let trigger = last_call_at(&exec, |call| is_keys(call, &["tab"]));
    assert_waited(&exec, trigger, 3);
    assert_keys_in_order(&exec, &["tab"]);
    assert_one_close_tab(&exec);
}

#[test]
fn mention_fails_when_reply_misses_the_word() {
    let shapes = Shapes {
        mention: Mention::ReplyMisses,
        ..Default::default()
    };
    let exec = play(&shapes, Step::Mention);
    assert_fails_with(
        &exec,
        "mention",
        "the reply did not mention the file's content (average) within 60 s",
    );
    let trigger = last_call_at(&exec, |call| is_keys(call, &["enter"]));
    assert_waited(&exec, trigger, 60);
    assert_keys_in_order(&exec, &["tab", "enter"]);
    assert_one_close_tab(&exec);
}

// ---- shell ----

#[test]
fn shell_passes() {
    let exec = play(&Shapes::default(), Step::Shell);
    assert_passes(&exec, "shell");
    assert_keys_start_with(&exec, &keys(&[K_PALETTE, K_MENTION, K_SHELL]));
    // The step checks that keepme survived (`shell_fails_when_folder_removed` covers its
    // failure); the run removes the scratch project afterwards.
    assert_one_close_tab(&exec);
}

#[test]
fn shell_fails_without_output() {
    let shapes = Shapes {
        shell: Shell::NoOutput,
        ..Default::default()
    };
    let exec = play(&shapes, Step::Shell);
    assert_fails_with(
        &exec,
        "shell",
        "the output of the ! command did not appear within 5 s",
    );
    let trigger = last_call_at(
        &exec,
        |call| matches!(call, Call::SendText { text, .. } if text.starts_with("!echo")),
    );
    assert_waited(&exec, trigger, 5);
    assert_keys_in_order(&exec, &["enter"]);
    assert_one_close_tab(&exec);
}

#[test]
fn shell_refusal_needs_text_and_surviving_folder() {
    let shapes = Shapes {
        shell: Shell::NoRefusal,
        ..Default::default()
    };
    let exec = play(&shapes, Step::Shell);
    assert_fails_with(
        &exec,
        "shell",
        "the refusal 'Hermes refused to run this: it looks dangerous' did not appear within 5 s",
    );
    let trigger = last_call_at(
        &exec,
        |call| matches!(call, Call::SendText { text, .. } if text.starts_with("!rm")),
    );
    assert_waited(&exec, trigger, 5);
    assert_keys_in_order(&exec, &["enter", "enter"]);
    assert_one_close_tab(&exec);
}

#[test]
fn shell_fails_when_folder_removed() {
    let shapes = Shapes {
        shell: Shell::FolderRemoved,
        ..Default::default()
    };
    let exec = play(&shapes, Step::Shell);
    assert_fails_with(
        &exec,
        "shell",
        "the refused command removed the keepme folder",
    );
    assert_keys_in_order(&exec, &["enter", "enter"]);
    assert_one_close_tab(&exec);
}

// ---- btw ----

#[test]
fn btw_passes() {
    let exec = play(&Shapes::default(), Step::Btw);
    assert_passes(&exec, "btw");
    assert_keys_start_with(&exec, &keys(&[K_PALETTE, K_MENTION, K_SHELL]));
    assert_one_close_tab(&exec);
}

#[test]
fn btw_fails_without_footer() {
    let shapes = Shapes {
        btw: Btw::NoCell,
        ..Default::default()
    };
    let exec = play(&shapes, Step::Btw);
    assert_fails_with(&exec, "btw", "no side answer cell appeared within 90 s");
    let trigger = last_call_at(
        &exec,
        |call| matches!(call, Call::Run { text, .. } if text.starts_with("/btw")),
    );
    assert_waited(&exec, trigger, 90);
    assert_one_close_tab(&exec);

    let shapes = Shapes {
        btw: Btw::Empty,
        ..Default::default()
    };
    let exec = play(&shapes, Step::Btw);
    assert_fails_with(&exec, "btw", "the side answer was empty");
    assert_one_close_tab(&exec);
}

#[test]
fn btw_fails_on_failed_question() {
    let shapes = Shapes {
        btw: Btw::Failed,
        ..Default::default()
    };
    let exec = play(&shapes, Step::Btw);
    assert_fails_with(&exec, "btw", "the side question failed: model busy");
    assert_one_close_tab(&exec);
}

#[test]
fn btw_fails_when_answer_lacks_the_word() {
    let shapes = Shapes {
        btw: Btw::LacksWord,
        ..Default::default()
    };
    let exec = play(&shapes, Step::Btw);
    assert_fails_with(
        &exec,
        "btw",
        "the side answer did not contain the word of the first token",
    );
    assert_one_close_tab(&exec);
}

// ---- pager ----

#[test]
fn pager_passes() {
    let exec = play(&Shapes::default(), Step::Pager);
    assert_passes(&exec, "pager");
    assert_keys_start_with(&exec, &keys(&[K_PALETTE, K_MENTION, K_SHELL, K_PAGER]));
    assert_one_close_tab(&exec);
}

#[test]
fn pager_fails_when_not_following() {
    let shapes = Shapes {
        pager: Pager::NotOpen,
        ..Default::default()
    };
    let exec = play(&shapes, Step::Pager);
    assert_fails_with(
        &exec,
        "pager",
        "Ctrl+O did not open the transcript pager within 5 s",
    );
    let trigger = last_call_at(&exec, |call| is_keys(call, &["ctrl+o"]));
    assert_waited(&exec, trigger, 5);
    assert_one_close_tab(&exec);

    let shapes = Shapes {
        pager: Pager::NotFollowing,
        ..Default::default()
    };
    let exec = play(&shapes, Step::Pager);
    assert_fails_with(&exec, "pager", "the pager footer does not say 'following'");
    assert_keys_in_order(&exec, &["ctrl+o"]);
    assert_one_close_tab(&exec);
}

#[test]
fn pager_fails_when_a_search_finds_nothing() {
    let shapes = Shapes {
        pager: Pager::TokenNoMatch,
        ..Default::default()
    };
    let exec = play(&shapes, Step::Pager);
    assert_fails_with(
        &exec,
        "pager",
        "searching the pager for the first token found no match",
    );
    let trigger = last_call_at(
        &exec,
        |call| matches!(call, Call::SendText { text, .. } if is_token_text(text)),
    );
    assert_waited(&exec, trigger, 5);
    assert_keys_in_order(&exec, &["ctrl+o", "enter"]);
    assert_one_close_tab(&exec);

    let shapes = Shapes {
        pager: Pager::ToolNoMatch,
        ..Default::default()
    };
    let exec = play(&shapes, Step::Pager);
    assert_fails_with(
        &exec,
        "pager",
        "the pager did not find the tool output 'No such file or directory'",
    );
    let trigger = last_call_at(
        &exec,
        |call| matches!(call, Call::SendText { text, .. } if text == "No such file"),
    );
    assert_waited(&exec, trigger, 5);
    assert_keys_in_order(&exec, &["ctrl+o", "enter", "enter"]);
    assert_one_close_tab(&exec);
}

#[test]
fn pager_fails_when_q_does_not_close() {
    let shapes = Shapes {
        pager: Pager::QStuck,
        ..Default::default()
    };
    let exec = play(&shapes, Step::Pager);
    assert_fails_with(
        &exec,
        "pager",
        "q did not return to the inline view within 5 s",
    );
    let trigger = last_call_at(&exec, |call| is_keys(call, &["q"]));
    assert_waited(&exec, trigger, 5);
    assert_one_close_tab(&exec);

    let shapes = Shapes {
        pager: Pager::Interrupt,
        ..Default::default()
    };
    let exec = play(&shapes, Step::Pager);
    assert_fails_with(&exec, "pager", "the pager's q or Esc interrupted the agent");
    assert_keys_in_order(&exec, &["ctrl+o", "enter", "enter", "q"]);
    assert_one_close_tab(&exec);
}

// ---- history-search ----

#[test]
fn history_search_passes() {
    let exec = play(&Shapes::default(), Step::History);
    assert_passes(&exec, "history-search");
    assert_keys_start_with(
        &exec,
        &keys(&[K_PALETTE, K_MENTION, K_SHELL, K_PAGER, K_HISTORY]),
    );
    assert_one_close_tab(&exec);
}

#[test]
fn history_search_fails_on_empty_box() {
    let shapes = Shapes {
        history: History::EmptyBox,
        ..Default::default()
    };
    let exec = play(&shapes, Step::History);
    assert_fails_with(
        &exec,
        "history-search",
        "Ctrl+R did not list the earlier prompt within 5 s",
    );
    let trigger = last_call_at(
        &exec,
        |call| matches!(call, Call::SendText { text, .. } if text.contains(" and ")),
    );
    assert_waited(&exec, trigger, 5);
    assert_keys_in_order(&exec, &["ctrl+r"]);
    assert_one_close_tab(&exec);
}

#[test]
fn history_search_fails_when_draft_not_loaded() {
    let shapes = Shapes {
        history: History::LoadFail,
        ..Default::default()
    };
    let exec = play(&shapes, Step::History);
    assert_fails_with(
        &exec,
        "history-search",
        "Enter did not load the earlier prompt into the composer",
    );
    let trigger = last_call_at(&exec, |call| is_keys(call, &["enter"]));
    assert_waited(&exec, trigger, 3);
    assert_keys_in_order(&exec, &["ctrl+r", "enter"]);
    assert_one_close_tab(&exec);

    let shapes = Shapes {
        history: History::ClearFail,
        ..Default::default()
    };
    let exec = play(&shapes, Step::History);
    assert_fails_with(
        &exec,
        "history-search",
        "Ctrl+C did not clear the recalled draft",
    );
    let trigger = last_call_at(&exec, |call| is_keys(call, &["ctrl+c"]));
    assert_waited(&exec, trigger, 3);
    assert_keys_in_order(&exec, &["ctrl+r", "enter", "ctrl+c"]);
    assert_one_close_tab(&exec);
}

// ---- editor ----

#[test]
fn editor_passes() {
    let exec = play(&Shapes::default(), Step::Editor);
    assert_passes(&exec, "editor");
    assert_keys_start_with(
        &exec,
        &keys(&[K_PALETTE, K_MENTION, K_SHELL, K_PAGER, K_HISTORY, K_EDITOR]),
    );
    assert_one_close_tab(&exec);
}

#[test]
fn editor_fails_when_draft_unchanged() {
    let shapes = Shapes {
        editor: Editor::DraftUnchanged,
        ..Default::default()
    };
    let exec = play(&shapes, Step::Editor);
    assert_fails_with(
        &exec,
        "editor",
        "Ctrl+G did not return the edited draft 'draft-one plus-two' within 10 s",
    );
    let trigger = last_call_at(&exec, |call| is_keys(call, &["ctrl+g"]));
    assert_waited(&exec, trigger, 10);
    assert_one_close_tab(&exec);

    let shapes = Shapes {
        editor: Editor::Sent,
        ..Default::default()
    };
    let exec = play(&shapes, Step::Editor);
    assert_fails_with(&exec, "editor", "the edited draft was sent without Enter");
    assert_keys_in_order(&exec, &["ctrl+g"]);
    assert_one_close_tab(&exec);
}

// ---- theme ----

#[test]
fn theme_passes() {
    let exec = play(&Shapes::default(), Step::Theme);
    assert_passes(&exec, "theme");
    assert_keys_exact(
        &exec,
        &keys(&[K_PALETTE, K_MENTION, K_SHELL, K_PAGER, K_HISTORY, K_EDITOR]),
    );
    assert_one_close_tab(&exec);
}

#[test]
fn theme_fails_without_a_line() {
    let shapes = Shapes {
        theme: Theme::ReportMissing,
        ..Default::default()
    };
    let exec = play(&shapes, Step::Theme);
    assert_fails_with(
        &exec,
        "theme",
        "/theme did not print its report (missing: Colours:, Motion:) within 5 s",
    );
    let trigger = last_call_at(
        &exec,
        |call| matches!(call, Call::Run { text, .. } if text == "/theme"),
    );
    assert_waited(&exec, trigger, 5);
    assert_one_close_tab(&exec);

    let shapes = Shapes {
        theme: Theme::SkinUsage,
        ..Default::default()
    };
    let exec = play(&shapes, Step::Theme);
    assert_fails_with(
        &exec,
        "theme",
        "/skin ../etc did not print its usage line within 5 s",
    );
    let trigger = last_call_at(
        &exec,
        |call| matches!(call, Call::Run { text, .. } if text == "/skin ../etc"),
    );
    assert_waited(&exec, trigger, 5);
    assert_one_close_tab(&exec);

    let shapes = Shapes {
        theme: Theme::ArgUsage,
        ..Default::default()
    };
    let exec = play(&shapes, Step::Theme);
    assert_fails_with(
        &exec,
        "theme",
        "/theme purple did not print its usage line within 5 s",
    );
    let trigger = last_call_at(
        &exec,
        |call| matches!(call, Call::Run { text, .. } if text == "/theme purple"),
    );
    assert_waited(&exec, trigger, 5);
    assert_one_close_tab(&exec);
}

#[test]
fn theme_fails_when_skin_usage_shows_a_refusal() {
    let shapes = Shapes {
        theme: Theme::Refused,
        ..Default::default()
    };
    let exec = play(&shapes, Step::Theme);
    assert_fails_with(
        &exec,
        "theme",
        "a bad /skin or /theme argument reached Hermes: a 'Hermes refused' line appeared",
    );
    assert_one_close_tab(&exec);
}

// ---- pane-state ----

#[test]
fn pane_state_passes() {
    let exec = play(&Shapes::default(), Step::PaneState);
    assert_passes(&exec, "pane-state");
    assert_keys_start_with(
        &exec,
        &keys(&[
            K_PALETTE,
            K_MENTION,
            K_SHELL,
            K_PAGER,
            K_HISTORY,
            K_EDITOR,
            K_PANE_STATE,
        ]),
    );
    assert_one_close_tab(&exec);
}

#[test]
fn pane_state_skips_when_no_agent_shown() {
    let shapes = Shapes {
        startup: Startup::NoAgent,
        ..Default::default()
    };
    let exec = play(&shapes, Step::PaneState);
    match exec
        .report
        .outcomes
        .iter()
        .find_map(|outcome| match outcome {
            StepOutcome::Skip { step, reason } if *step == "pane-state" => Some(reason.clone()),
            _ => None,
        }) {
        Some(reason) => assert_eq!(reason, PANE_STATE_SKIP, "the skip reason is exact"),
        None => panic!("pane-state must skip; lines:\n{}", exec.lines()),
    }
    // The checklist has no rules here, so its prompt is sent once and repeated once.
    assert!(
        !submitted(&exec, "todo_list tool to write a plan").is_empty(),
        "the run continues with the checklist prompt"
    );
    assert_one_close_tab(&exec);
}

#[test]
fn pane_state_fails_without_working() {
    let shapes = Shapes {
        pane_state: PaneState::Counting,
        ..Default::default()
    };
    let exec = play(&shapes, Step::PaneState);
    assert_fails_with(
        &exec,
        "pane-state",
        "fewer than 15 counting lines appeared within 60 s",
    );
    let trigger = last_call_at(
        &exec,
        |call| matches!(call, Call::Run { text, .. } if text.contains("Count from 1 to 2000")),
    );
    assert_waited(&exec, trigger, 60);
    assert_one_close_tab(&exec);

    let shapes = Shapes {
        pane_state: PaneState::Working,
        ..Default::default()
    };
    let exec = play(&shapes, Step::PaneState);
    assert_fails_with(
        &exec,
        "pane-state",
        "the pane did not report 'working' while the reply streamed",
    );
    assert_one_close_tab(&exec);

    let shapes = Shapes {
        pane_state: PaneState::Idle,
        ..Default::default()
    };
    let exec = play(&shapes, Step::PaneState);
    assert_fails_with(
        &exec,
        "pane-state",
        "the pane did not report 'idle' within 10 s after the interrupt",
    );
    let trigger = last_call_at(&exec, |call| is_keys(call, &["esc"]));
    assert_waited(&exec, trigger, 10);
    assert_keys_in_order(&exec, &["esc"]);
    assert_one_close_tab(&exec);
}

// ---- checklist ----

#[test]
fn checklist_passes_with_collapse_and_record() {
    let exec = play(&Shapes::default(), Step::Checklist);
    assert_passes(&exec, "checklist");
    assert_keys_start_with(
        &exec,
        &keys(&[
            K_PALETTE,
            K_MENTION,
            K_SHELL,
            K_PAGER,
            K_HISTORY,
            K_EDITOR,
            K_PANE_STATE,
            K_CHECKLIST,
        ]),
    );
    assert_one_close_tab(&exec);
}

#[test]
fn checklist_skips_after_two_attempts() {
    let shapes = Shapes {
        checklist: Checklist::SkipTwice,
        ..Default::default()
    };
    let exec = play(&shapes, Step::Checklist);
    match exec
        .report
        .outcomes
        .iter()
        .find_map(|outcome| match outcome {
            StepOutcome::Skip { step, reason } if *step == "checklist" => Some(reason.clone()),
            _ => None,
        }) {
        Some(reason) => assert_eq!(
            reason, "the model did not use the todo tool in two attempts",
            "the skip reason is exact"
        ),
        None => panic!("checklist must skip; lines:\n{}", exec.lines()),
    }
    assert_eq!(
        submitted(&exec, "todo_list tool to write a plan").len(),
        2,
        "two prompts are sent"
    );
    assert!(
        !submitted(&exec, "/quit").is_empty(),
        "the run continues with the quit step"
    );
    assert_one_close_tab(&exec);
}

#[test]
fn checklist_fails_when_panel_stays() {
    let shapes = Shapes {
        checklist: Checklist::CollapseFail,
        ..Default::default()
    };
    let exec = play(&shapes, Step::Checklist);
    assert_fails_with(
        &exec,
        "checklist",
        "Ctrl+T did not collapse the task panel to one row within 3 s",
    );
    assert_one_close_tab(&exec);

    let shapes = Shapes {
        checklist: Checklist::ExpandFail,
        ..Default::default()
    };
    let exec = play(&shapes, Step::Checklist);
    assert_fails_with(
        &exec,
        "checklist",
        "Ctrl+T did not expand the task panel within 3 s",
    );
    let trigger = last_call_at(&exec, |call| is_keys(call, &["ctrl+t"]));
    assert_waited(&exec, trigger, 3);
    assert_keys_in_order(&exec, &["ctrl+t", "ctrl+t"]);
    assert_one_close_tab(&exec);

    let shapes = Shapes {
        checklist: Checklist::EndFail,
        ..Default::default()
    };
    let exec = play(&shapes, Step::Checklist);
    assert_fails_with(&exec, "checklist", "the task turn did not end within 150 s");
    let trigger = last_call_at(&exec, |call| is_keys(call, &["ctrl+t"]));
    assert_waited(&exec, trigger, 150);
    assert_keys_in_order(&exec, &["ctrl+t", "ctrl+t"]);
    assert_one_close_tab(&exec);

    let shapes = Shapes {
        checklist: Checklist::PanelLeft,
        ..Default::default()
    };
    let exec = play(&shapes, Step::Checklist);
    assert_fails_with(
        &exec,
        "checklist",
        "the task panel was still shown after the turn ended",
    );
    assert_keys_in_order(&exec, &["ctrl+t", "ctrl+t"]);
    assert_one_close_tab(&exec);
}

#[test]
fn checklist_fails_with_two_records() {
    let shapes = Shapes {
        checklist: Checklist::TwoRecords,
        ..Default::default()
    };
    let exec = play(&shapes, Step::Checklist);
    assert_fails_with(
        &exec,
        "checklist",
        "the turn ended with 2 'Tasks <c>/<t> · ' records in scrollback, expected 1",
    );
    assert_keys_in_order(&exec, &["ctrl+t", "ctrl+t"]);
    assert_one_close_tab(&exec);
}

// ---- resume ----

#[test]
fn resume_checks_local_cells_are_gone() {
    let shapes = Shapes {
        resume: Resume::LocalCell,
        ..Default::default()
    };
    let exec = play(&shapes, Step::Resume);
    assert_fails_with(
        &exec,
        "resume",
        "a local cell was printed again after resume ('! echo' or 'btw ·'): Hermes does not store it",
    );
    assert_one_close_tab(&exec);
}

#[test]
fn resume_fails_when_record_printed_twice() {
    let shapes = Shapes {
        resume: Resume::RecordTwice,
        ..Default::default()
    };
    let exec = play(&shapes, Step::Resume);
    assert_fails_with(
        &exec,
        "resume",
        "the resumed task list was printed 2 times, expected 1",
    );
    assert_one_close_tab(&exec);

    let shapes = Shapes {
        resume: Resume::PagerFail,
        ..Default::default()
    };
    let exec = play(&shapes, Step::Resume);
    assert_fails_with(
        &exec,
        "resume",
        "the pager of the resumed session did not find the first token",
    );
    assert_keys_in_order(&exec, &["ctrl+o", "enter"]);
    assert_one_close_tab(&exec);
}

// ---- tmux-pane ----

#[test]
fn tmux_start_fails_with_exit_code() {
    let exec = play_paster(&Shapes::default(), Step::TmuxPane, |paster| {
        paster.set_exists(false);
    });
    assert_fails_with(
        &exec,
        "tmux-pane",
        "tmux did not start the talaria session within 30 s",
    );
    let trigger = last_call_at(
        &exec,
        |call| matches!(call, Call::Run { text, .. } if text.contains("tmux new-session")),
    );
    assert_waited(&exec, trigger, 30);
    assert_one_close_tab(&exec);

    let exec = play_paster(&Shapes::default(), Step::TmuxPane, |paster| {
        paster.set_exit_code(Some(7));
    });
    assert_fails_with(
        &exec,
        "tmux-pane",
        "talaria exited with code 7 before showing its screen",
    );
    assert_one_close_tab(&exec);
}

// ---- paste-collapse ----

#[test]
fn paste_collapse_passes() {
    let exec = play(&Shapes::default(), Step::PasteCollapse);
    assert_passes(&exec, "paste-collapse");
    assert_keys_exact(
        &exec,
        &keys(&[
            K_PALETTE,
            K_MENTION,
            K_SHELL,
            K_PAGER,
            K_HISTORY,
            K_EDITOR,
            K_PANE_STATE,
            K_CHECKLIST,
            K_RESUME_PAGER,
            &["enter"],
        ]),
    );
    assert_one_close_tab(&exec);
}

#[test]
fn paste_collapse_fails_without_label() {
    let shapes = Shapes {
        paste_collapse: PasteCollapse::NoLabel,
        ..Default::default()
    };
    let exec = play(&shapes, Step::PasteCollapse);
    assert_fails_with(
        &exec,
        "paste-collapse",
        "the pasted text did not collapse into '[Pasted text #1 +120 lines]' within 5 s",
    );
    assert_waited(&exec, last_paste_at(&exec), 5);
    assert_one_close_tab(&exec);

    let shapes = Shapes {
        paste_collapse: PasteCollapse::Leak,
        ..Default::default()
    };
    let exec = play(&shapes, Step::PasteCollapse);
    assert_fails_with(
        &exec,
        "paste-collapse",
        "the collapsed paste still shows its text in the composer",
    );
    assert_one_close_tab(&exec);

    let shapes = Shapes {
        paste_collapse: PasteCollapse::NoReply,
        ..Default::default()
    };
    let exec = play(&shapes, Step::PasteCollapse);
    assert_fails_with(
        &exec,
        "paste-collapse",
        "the reply did not contain the marker from the pasted text within 60 s",
    );
    let trigger = last_call_at(
        &exec,
        |call| matches!(call, Call::SendText { text, .. } if text.contains("marker on line 87")),
    );
    assert_waited(&exec, trigger, 60);
    assert_keys_in_order(&exec, &["enter"]);
    assert_one_close_tab(&exec);
}

// ---- file-drop ----

#[test]
fn file_drop_passes() {
    let exec = play(&Shapes::default(), Step::FileDrop);
    assert_passes(&exec, "file-drop");
    assert_keys_exact(
        &exec,
        &keys(&[
            K_PALETTE,
            K_MENTION,
            K_SHELL,
            K_PAGER,
            K_HISTORY,
            K_EDITOR,
            K_PANE_STATE,
            K_CHECKLIST,
            K_RESUME_PAGER,
            &["enter", "enter"],
        ]),
    );
    assert_one_close_tab(&exec);
}

#[test]
fn file_drop_fails_without_label() {
    let shapes = Shapes {
        file_drop: FileDrop::NoLabel,
        ..Default::default()
    };
    let exec = play(&shapes, Step::FileDrop);
    assert_fails_with(
        &exec,
        "file-drop",
        &format!(
            "the pasted file path did not become '[File #1: {}]' within 5 s",
            drop_name(&exec)
        ),
    );
    assert_waited(&exec, last_paste_at(&exec), 5);
    assert_one_close_tab(&exec);

    let shapes = Shapes {
        file_drop: FileDrop::NoReply,
        ..Default::default()
    };
    let exec = play(&shapes, Step::FileDrop);
    assert_fails_with(
        &exec,
        "file-drop",
        "the reply did not contain the word from the dropped file within 60 s",
    );
    let trigger = last_call_at(
        &exec,
        |call| matches!(call, Call::SendText { text, .. } if text.contains("secret word")),
    );
    assert_waited(&exec, trigger, 60);
    assert_keys_in_order(&exec, &["enter"]);
    assert_one_close_tab(&exec);
}

// ---- paste-editor ----

#[test]
fn paste_editor_passes() {
    let exec = play(&Shapes::default(), Step::PasteEditor);
    assert_passes(&exec, "paste-editor");
    assert_keys_start_with(
        &exec,
        &keys(&[
            K_PALETTE,
            K_MENTION,
            K_SHELL,
            K_PAGER,
            K_HISTORY,
            K_EDITOR,
            K_PANE_STATE,
            K_CHECKLIST,
            K_RESUME_PAGER,
            &["enter", "enter", "ctrl+g", "enter"],
        ]),
    );
    assert_one_close_tab(&exec);
}

#[test]
fn paste_editor_fails_when_label_lost() {
    let shapes = Shapes {
        paste_editor: PasteEditor::NoLabel,
        ..Default::default()
    };
    let exec = play(&shapes, Step::PasteEditor);
    assert_fails_with(
        &exec,
        "paste-editor",
        "the second paste did not collapse into a '[Pasted text #<n> +120 lines]' label within 5 s",
    );
    assert_waited(&exec, last_paste_at(&exec), 5);
    assert_one_close_tab(&exec);

    let shapes = Shapes {
        paste_editor: PasteEditor::LabelLost,
        ..Default::default()
    };
    let exec = play(&shapes, Step::PasteEditor);
    assert_fails_with(
        &exec,
        "paste-editor",
        "the paste label did not survive the external editor",
    );
    assert_keys_in_order(&exec, &["ctrl+g"]);
    assert_one_close_tab(&exec);

    let shapes = Shapes {
        paste_editor: PasteEditor::NoReply,
        ..Default::default()
    };
    let exec = play(&shapes, Step::PasteEditor);
    assert_fails_with(
        &exec,
        "paste-editor",
        "the reply did not contain the marker from the second paste within 60 s",
    );
    let trigger = last_call_at(
        &exec,
        |call| matches!(call, Call::SendText { text, .. } if text.contains("of the paste")),
    );
    assert_waited(&exec, trigger, 60);
    assert_keys_in_order(&exec, &["ctrl+g", "enter"]);
    assert_one_close_tab(&exec);
}

// ---- tmux-quit ----

#[test]
fn tmux_quit_needs_exit_zero_and_session_gone() {
    let shapes = Shapes {
        tmux_quit: TmuxQuit::ExitWrong,
        ..Default::default()
    };
    let exec = play(&shapes, Step::TmuxQuit);
    assert_fails_with(
        &exec,
        "tmux-quit",
        "/quit in the tmux pane did not end talaria with exit code 0 within 15 s",
    );
    let trigger = last_call_at(
        &exec,
        |call| matches!(call, Call::SendText { text, .. } if text == "/quit"),
    );
    assert_waited(&exec, trigger, 15);
    assert_one_close_tab(&exec);

    let shapes = Shapes {
        tmux_quit: TmuxQuit::SessionStuck,
        ..Default::default()
    };
    let exec = play_paster(&shapes, Step::TmuxQuit, |paster| {
        paster.set_exists(true);
        paster.set_exit_code(Some(0));
    });
    assert_fails_with(
        &exec,
        "tmux-quit",
        "the tmux session was still running after /quit",
    );
    let trigger = last_call_at(
        &exec,
        |call| matches!(call, Call::SendText { text, .. } if text == "/quit"),
    );
    assert_waited(&exec, trigger, 15);
    assert_one_close_tab(&exec);
}

// ---- run-level ----

#[test]
fn a_failing_step_stops_the_run_and_cleans_up() {
    let shapes = Shapes {
        paste_collapse: PasteCollapse::NoLabel,
        ..Default::default()
    };
    let exec = play(&shapes, Step::PasteCollapse);
    assert_fails_with(
        &exec,
        "paste-collapse",
        "the pasted text did not collapse into '[Pasted text #1 +120 lines]' within 5 s",
    );
    for kind in exec.kinds() {
        assert!(
            !kind.starts_with("file-drop")
                && !kind.starts_with("paste-editor")
                && !kind.starts_with("tmux-quit"),
            "later steps print nothing: {:?}",
            exec.kinds()
        );
    }
    assert_one_close_tab(&exec);
    assert_eq!(exec.kills().len(), 1, "one kill_session");

    let exec = play_paster(&Shapes::default(), Step::TmuxPane, |paster| {
        paster.fail_exists_with("broken pipe");
    });
    assert_fails_with(&exec, "tmux-pane", "tmux failed: broken pipe");
    assert_one_close_tab(&exec);

    let script = script_2a(&Shapes::default(), Step::PaneState)
        .fail(FailBuilder::call("pane get", "pane_gone").message("gone"));
    let exec = exec_2a(&script, |_| {}, |term| term, |_| {});
    assert_fails_with(
        &exec,
        "pane-state",
        "herdr pane get failed: pane_gone: gone",
    );
    assert_one_close_tab(&exec);
}

#[test]
fn deadline_names_the_step() {
    let shapes = Shapes {
        tool_shell: ToolShell::NoToken,
        ..Default::default()
    };
    let exec = play_cfg(&shapes, Step::ToolShell, |cfg| {
        cfg.deadline = Duration::from_secs(1800);
        cfg.step_scale = 100.0;
    });
    assert_fails_with(&exec, "tool-shell", "deadline exceeded in step tool-shell");
    assert!(
        exec.last_at() >= Duration::from_secs(1800),
        "the run reached the 1800 s deadline"
    );
    assert_one_close_tab(&exec);
}

#[test]
fn a_skip_is_never_a_pass() {
    let shapes = Shapes {
        startup: Startup::NoAgent,
        ..Default::default()
    };
    let exec = play(&shapes, Step::TmuxQuit);
    assert!(
        exec.kinds().iter().any(|kind| kind == "pane-state:skip"),
        "pane-state skips: {:?}",
        exec.kinds()
    );
    assert!(
        exec.kinds().iter().all(|kind| !kind.ends_with(":fail")),
        "nothing fails: {:?}",
        exec.kinds()
    );
    assert_eq!(
        exec.report.result_line(),
        "SMOKE_RESULT: PASSED WITH SKIPS (pane-state)"
    );
    assert_eq!(exec.report.exit_code(), 3);
    assert_one_close_tab(&exec);
    assert_eq!(exec.kills().len(), 1, "one kill_session");
}

#[test]
fn live_require_turns_a_skip_into_a_failure() {
    let shapes = Shapes {
        startup: Startup::NoAgent,
        ..Default::default()
    };
    let exec = play_cfg(&shapes, Step::TmuxQuit, |cfg| {
        cfg.require = true;
    });
    let line = format!("FAIL pane-state (skipped: {PANE_STATE_SKIP}; TALARIA_LIVE_REQUIRE=1)");
    assert!(
        exec.lines().contains(&line),
        "the skip fails under the live requirement; lines:\n{}",
        exec.lines()
    );
    assert_eq!(exec.report.exit_code(), 1);
    assert!(
        exec.kinds()
            .last()
            .is_some_and(|kind| kind == "pane-state:fail"),
        "the run stops at the failure: {:?}",
        exec.kinds()
    );
    assert_one_close_tab(&exec);
}

#[test]
fn step_scale_leaves_quiet_periods() {
    let exec = play_cfg(&Shapes::default(), Step::Palette, |cfg| {
        cfg.step_scale = 0.5;
    });
    assert_passes(&exec, "palette");
    assert!(
        pass_elapsed(&exec, "palette") >= Duration::from_secs(1),
        "the 1 s quiet period is not scaled"
    );
    assert_one_close_tab(&exec);

    let shapes = Shapes {
        palette: Palette::NotOpen,
        ..Default::default()
    };
    let exec = play_cfg(&shapes, Step::Palette, |cfg| {
        cfg.step_scale = 0.5;
    });
    assert_fails_with(
        &exec,
        "palette",
        "the slash palette did not list /version with its footer within 3 s",
    );
    assert_one_close_tab(&exec);
}
