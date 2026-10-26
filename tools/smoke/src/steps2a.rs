//! The phase 2a run: the twenty-one steps of issues #221 to #228 behind one function.
//!
//! The first suite's `startup`, `plain-answer`, `quit` and `resume` steps run unchanged, with
//! an `EDITOR` setting on every launch line; the steps between them check one feature each,
//! and the tmux section at the end drives bracketed pastes into a third pane.

use std::cell::RefCell;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::time::Duration;

use rand::rngs::StdRng;
use rand::{RngExt, SeedableRng};

use crate::clock::Clock;
use crate::config::{Phase2aConfig, SmokeConfig};
use crate::report::{PaneCapture, SmokeReport, StepOutcome};
use crate::screen::{POLL_INTERVAL, Token, count_token, counting_values, holds_word};
use crate::screen2a::{
    AgentState, Mark, TaskView, agent_state, edit_header, file_label, has_debris, has_diff_rows,
    mention_list, normalised, pager_state, palette_state, paste_labels, prompt_after, rows_after,
    shell_cell, side_cell, task_records, task_view, theme_report, tool_header,
};
use crate::steps::{
    AFTER_ESC, Abort, COUNT_MARKER, COUNT_PROMPT, FALLBACK_WINDOW, QUIET_REPLY, RECENT_LINES,
    REPORT_LINES, Run, SETTLE_LIMIT, SHELL_LIMIT, Step, TabGuard, Watch, card_words, finish,
    panic_message,
};
use crate::terminal::{PaneId, ReadSource, TabId, Terminal};
use crate::tmux::{Paster, TmuxGuard};
use crate::wording::{
    DIFF_ROWS, EDIT_HEADERS, EDIT_STAT, HISTORY_NO_MATCH, LOCAL_COMMANDS, MENTION_FOLDER,
    MENTION_INSERT, MENTION_ROW, PALETTE_FOOTER, PALETTE_GROUP, PALETTE_TAG, REFUSED,
    SHELL_EXIT_MARK, SHELL_HEADER, SHELL_NO_SUCH_DIR, SHELL_REFUSAL, SIDE_FOOTER, SIDE_HEADER,
    TASK_COLLAPSED, TASK_EXPAND_HINT, TASK_RESUMED, THEME_LINES, USAGE_SKIN, USAGE_THEME,
};

/// The steps of a phase 2a run, in order.
const STEPS: [&str; 21] = [
    "startup",
    "plain-answer",
    "tool-shell",
    "tool-edit",
    "palette",
    "mention",
    "shell",
    "btw",
    "pager",
    "history-search",
    "editor",
    "theme",
    "pane-state",
    "checklist",
    "quit",
    "resume",
    "tmux-pane",
    "paste-collapse",
    "file-drop",
    "paste-editor",
    "tmux-quit",
];

/// The folder name of every run, before its id.
const SCRATCH_PREFIX: &str = "talaria-smoke-";

/// The command the tool-shell step asks the model to run.
const SHELL_COMMAND: &str = "ls /talaria-no-such-dir";
/// The action of the tool-shell prompt.
const SHELL_ACTION: &str = "Run the shell command ls /talaria-no-such-dir";
/// The action of the first tool-edit prompt.
const EDIT_ACTION: &str = "In notes.txt change the word beta to gamma. Use your file editing tool";
/// The action of the second tool-edit prompt.
const EDIT_RETRY_ACTION: &str =
    "Use your file editing tool now to change the word beta to gamma in notes.txt";
/// The file the tool-edit step edits, and its starting text.
const NOTES_FILE: &str = "notes.txt";
const NOTES_TEXT: &str = "alpha\nbeta\n";
/// The text typed to open the palette on one command.
const PALETTE_QUERY: &str = "/vers";
/// The command the palette completes to.
const VERSION_COMMAND: &str = "/version";
/// The mention text typed before Tab, and the rest of the question.
const MENTION_QUERY: &str = "Summarize @calc";
const MENTION_REST: &str = " in one sentence";
/// Words the mention reply must hold one of.
const MENTION_WORDS: [&str; 3] = ["average", "averages", "mean"];
/// The one refused shell line.
const REFUSED_SHELL_LINE: &str = "!rm -r ./keepme";
/// The side question.
const BTW_QUESTION: &str = "which two words did I ask you to join in my first message?";
/// The pager search for the tool output.
const PAGER_TOOL_QUERY: &str = "No such file";
/// The draft the editor step types, and what the editor script makes of it.
const DRAFT: &str = "draft-one";
const EDITED_DRAFT: &str = "draft-one plus-two";
/// The three theme submissions.
const THEME_COMMAND: &str = "/theme";
const BAD_SKIN: &str = "/skin ../etc";
const BAD_THEME: &str = "/theme purple";
/// The plan prompt of the checklist step (issue #226, verbatim).
const PLAN_PROMPT: &str = "Use your todo_list tool to write a plan with exactly three items: create a.txt, run sleep 30, create b.txt. Before each step mark it in_progress with the todo_list tool, and after it mark it completed. Do the steps in this folder. Then reply with the single word: done.";
/// The rows of an open task panel.
const TASK_ROW_MARKS: [&str; 4] = ["[ ]", "[>]", "[x]", "[-]"];
/// The questions of the tmux section.
const PASTE_QUESTION: &str = " What is the marker on line 87? Reply with only the marker word.";
const DROP_QUESTION: &str = " What is the secret word? Reply with only that word.";
const PASTE_EDITOR_QUESTION: &str =
    " What is the marker on line 87 of the paste? Reply with only the marker word.";
/// The text a pasted file must never show in the composer.
const MARKER_TEXT: &str = "the marker is";
/// The lines of each big paste, and the line that holds the marker.
const BIG_LINES: usize = 120;
const MARKER_LINE: usize = 87;
/// The slash line that ends a session.
const QUIT_LINE: &str = "/quit";

/// The reason of a skipped pane-state step (issue #228, pane state scenario).
const PANE_STATE_SKIP: &str = "herdr pane get shows no agent state for talaria; read the Herdr sidebar by hand (issue #228, pane state scenario)";
/// The reason of a skipped checklist step.
const CHECKLIST_SKIP: &str = "the model did not use the todo tool in two attempts";
/// The start of the reason of a reply that never came.
const TOKEN_REASON: &str = "the reply token did not appear and settle within";

/// The per-step limits, in seconds, before the scale is applied.
const TOOL_LIMIT: u64 = 90;
const PALETTE_OPEN: u64 = 5;
const PALETTE_BLOCK: u64 = 15;
const PALETTE_BROWSE: u64 = 5;
const PALETTE_CLOSE: u64 = 3;
const MENTION_LIST: u64 = 5;
const MENTION_TAB: u64 = 3;
const MENTION_REPLY: u64 = 60;
const SHELL_CHECK: u64 = 5;
const BTW_LIMIT: u64 = 90;
const PAGER_CHECK: u64 = 5;
const HISTORY_BOX: u64 = 5;
const HISTORY_LOAD: u64 = 3;
const HISTORY_CLEAR: u64 = 3;
const EDITOR_LIMIT: u64 = 10;
const THEME_CHECK: u64 = 5;
const COUNT_LIMIT: u64 = 60;
const IDLE_LIMIT: u64 = 10;
const PANEL_LIMIT: u64 = 120;
const TOGGLE_LIMIT: u64 = 3;
const TURN_END_LIMIT: u64 = 150;
const SESSION_LIMIT: u64 = 30;
const LABEL_LIMIT: u64 = 5;
const PASTE_EDITOR_LIMIT: u64 = 10;
const PASTE_REPLY: u64 = 60;
const TMUX_QUIT_LIMIT: u64 = 15;

/// The wait after typed text, longer than the composer's 120 ms paste window; never scaled.
const AFTER_TYPED: Duration = Duration::from_millis(300);

/// The quiet periods, in seconds; they are never scaled.
const QUIET_BLOCK: u64 = 1;
const QUIET_EDITOR: u64 = 2;
const QUIET_TMUX: u64 = 2;
const QUIET_PLAN: u64 = 5;

/// Runs the phase 2a suite: the shared first-suite steps, then one step per feature, then the
/// tmux section. The steps after the first failure are not attempted.
pub fn run_smoke_2a(
    term: &dyn Terminal,
    paster: &dyn Paster,
    clock: &dyn Clock,
    cfg: &SmokeConfig,
    cfg2: &Phase2aConfig,
    stop: &AtomicBool,
) -> SmokeReport {
    let start = clock.now();
    let mut rng = match cfg.seed {
        Some(seed) => StdRng::seed_from_u64(seed),
        None => StdRng::from_rng(&mut rand::rng()),
    };
    let run_id = draw_run_id(&mut rng);
    let label = format!("{SCRATCH_PREFIX}{run_id}");
    let scratch = cfg.temp_root.join(&label);
    let drop = cfg.temp_root.join(format!("{label}-drop"));
    let _ = fs::remove_dir_all(&scratch);
    let _ = fs::remove_dir_all(&drop);
    make_scratch(&scratch);

    let mut run = Run2a {
        base: Run {
            term,
            clock,
            cfg,
            stop,
            deadline: start + cfg.deadline,
            rng,
            tokens: Vec::new(),
            scratch: scratch.clone(),
            answered_at: None,
            fallbacks: 0,
            step_fallbacks: 0,
            fallback_steps: Vec::new(),
            interrupted: false,
            pane: PaneId(String::new()),
            second: None,
            counts: (0, 0),
        },
        paster,
        cfg2,
        session: label.clone(),
        drop: drop.clone(),
        drop_file: None,
        third: None,
        guard: None,
        shell_token: None,
        checklist_passed: false,
        markers: None,
    };

    let tab = match term.create_tab(&label, &scratch) {
        Ok(tab) => tab,
        Err(error) => {
            let _ = fs::remove_dir_all(&scratch);
            let _ = fs::remove_dir_all(&drop);
            return SmokeReport {
                outcomes: vec![StepOutcome::Fail {
                    step: "startup",
                    reason: format!("the tab could not be created: {error}"),
                    capture: Vec::new(),
                }],
                version: term.version(),
                elapsed: clock.now().saturating_sub(start),
                fallbacks: 0,
                interrupted: false,
                unclosed_tab: None,
                unclosed_session: None,
                tab: None,
                scratch,
                fallback_steps: Vec::new(),
                notes: Vec::new(),
            };
        }
    };
    run.base.pane = tab.pane.clone();

    let mut outcomes: Vec<StepOutcome> = Vec::new();
    let mut step_index = 0;
    while step_index < STEPS.len() {
        let name = STEPS[step_index];
        let started = clock.now();
        run.base.step_fallbacks = 0;
        let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| run.step(name)))
            .unwrap_or_else(|panic| {
                let message = panic_message(&panic);
                Ok(Step::Fail {
                    reason: format!("internal error: {message}"),
                })
            });
        if run.base.step_fallbacks > 0 {
            run.base.fallback_steps.push(name);
        }
        match outcome {
            Ok(Step::Pass) => {
                outcomes.push(StepOutcome::Pass {
                    step: name,
                    elapsed: clock.now().saturating_sub(started),
                });
                step_index += 1;
            }
            Ok(Step::Skip { reason }) => {
                if cfg.require {
                    outcomes.push(StepOutcome::Fail {
                        step: name,
                        reason: format!("skipped: {reason}; TALARIA_LIVE_REQUIRE=1"),
                        capture: Vec::new(),
                    });
                    break;
                }
                outcomes.push(StepOutcome::Skip { step: name, reason });
                step_index += 1;
            }
            Ok(Step::Fail { reason }) => {
                let capture = run.capture(name);
                outcomes.push(StepOutcome::Fail {
                    step: name,
                    reason,
                    capture,
                });
                break;
            }
            Err(abort) => {
                let reason = match abort {
                    Abort::Interrupted => {
                        run.base.interrupted = true;
                        "interrupted by a signal".to_owned()
                    }
                    Abort::Deadline => format!("deadline exceeded in step {name}"),
                    Abort::Failed(reason) => reason,
                };
                let capture = run.capture(name);
                outcomes.push(StepOutcome::Fail {
                    step: name,
                    reason,
                    capture,
                });
                break;
            }
        }
    }

    // The tmux session goes first: a session left behind keeps talaria running after its
    // pane is gone.
    let mut unclosed_session = None;
    if let Some(guard) = run.guard.take()
        && guard.close().is_err()
    {
        unclosed_session = Some(label.clone());
    }
    let failure_cell: RefCell<Option<crate::steps::CleanupError>> = RefCell::new(None);
    let mut unclosed: Option<TabId> = None;
    let guard = TabGuard::new(term, tab.tab.clone(), &failure_cell);
    if guard.close().is_err() {
        unclosed = Some(tab.tab.clone());
    }
    if let Some(error) = failure_cell.into_inner() {
        unclosed = Some(error.tab);
    }
    let _ = fs::remove_dir_all(&scratch);
    let _ = fs::remove_dir_all(&drop);

    SmokeReport {
        outcomes,
        version: term.version(),
        elapsed: clock.now().saturating_sub(start),
        fallbacks: run.base.fallbacks,
        interrupted: run.base.interrupted,
        unclosed_tab: unclosed,
        unclosed_session,
        tab: Some(tab.tab),
        scratch,
        fallback_steps: run.base.fallback_steps,
        notes: Vec::new(),
    }
}

/// The scratch project of a run with this seed under this temporary root: the command line
/// fixes the seed so that it can hand tmux the folder before the run starts.
pub fn scratch_for(temp_root: &Path, seed: u64) -> PathBuf {
    let mut rng = StdRng::seed_from_u64(seed);
    let run_id = draw_run_id(&mut rng);
    temp_root.join(format!("{SCRATCH_PREFIX}{run_id}"))
}

/// The eight characters of a run id.
fn draw_run_id(rng: &mut StdRng) -> String {
    const ALPHABET: &[u8] = b"abcdefghijklmnopqrstuvwxyz0123456789";
    (0..8)
        .map(|_| ALPHABET[rng.random_range(0..ALPHABET.len())] as char)
        .collect()
}

/// The tokens the tmux section's files hold.
struct Markers {
    /// The marker of the first paste.
    first: Token,
    /// The secret word of the dropped file.
    secret: Token,
    /// The marker of the second paste.
    second: Token,
}

/// The state of one phase 2a run.
struct Run2a<'a> {
    base: Run<'a>,
    paster: &'a dyn Paster,
    cfg2: &'a Phase2aConfig,
    /// The tmux session, named like the scratch project.
    session: String,
    /// The folder outside the project that holds the dropped file.
    drop: PathBuf,
    /// The dropped file's name, once written.
    drop_file: Option<String>,
    /// The pane the tmux session runs in.
    third: Option<PaneId>,
    /// Ends the tmux session, once it was started.
    guard: Option<TmuxGuard<'a>>,
    /// The joined token the shell step echoed.
    shell_token: Option<String>,
    /// Whether the checklist step passed, so the resumed pane must show its record.
    checklist_passed: bool,
    /// The tokens of the tmux section, once its files are written.
    markers: Option<Markers>,
}

impl Run2a<'_> {
    /// Runs one step by its name.
    fn step(&mut self, name: &'static str) -> Result<Step, Abort> {
        match name {
            "startup" => {
                let editor = self.editor_script();
                self.base.startup(&[("EDITOR", editor.as_str())])
            }
            "plain-answer" => self.base.plain_answer(),
            "tool-shell" => self.tool_shell(),
            "tool-edit" => self.tool_edit(),
            "palette" => self.palette(),
            "mention" => self.mention(),
            "shell" => self.shell(),
            "btw" => self.btw(),
            "pager" => self.pager(),
            "history-search" => self.history_search(),
            "editor" => self.editor(),
            "theme" => self.theme(),
            "pane-state" => self.pane_state(),
            "checklist" => self.checklist(),
            "quit" => self.base.quit(),
            "resume" => {
                let editor = self.editor_script();
                let first = self.first_token().join();
                let shell = self.shell_token.clone();
                let checklist = self.checklist_passed;
                self.base
                    .resume(&[("EDITOR", editor.as_str())], &mut |run, pane| {
                        resumed_checks(run, pane, &first, shell.as_deref(), checklist)
                    })
            }
            "tmux-pane" => self.tmux_pane(),
            "paste-collapse" => self.paste_collapse(),
            "file-drop" => self.file_drop(),
            "paste-editor" => self.paste_editor(),
            "tmux-quit" => self.tmux_quit(),
            other => Ok(Step::Fail {
                reason: format!("there is no step named {other}"),
            }),
        }
    }

    /// The editor script every launch line names.
    fn editor_script(&self) -> String {
        self.base
            .scratch
            .join("smoke-editor.sh")
            .to_string_lossy()
            .into_owned()
    }

    /// The plain-answer token.
    fn first_token(&self) -> Token {
        self.base.tokens[0]
    }

    /// The pane the tmux session runs in.
    fn third(&self) -> PaneId {
        self.third.clone().unwrap_or_else(|| PaneId(String::new()))
    }

    /// The screen of every pane, for the failure report: the panes not in use first, the pane
    /// the step used last (the report prints the recent text of the last one).
    fn capture(&self, step: &str) -> Vec<PaneCapture> {
        let in_use = match step {
            "resume" => "second",
            "tmux-pane" | "paste-collapse" | "file-drop" | "paste-editor" | "tmux-quit" => "third",
            _ => "first",
        };
        let panes: [(&'static str, Option<&PaneId>); 3] = [
            ("first", Some(&self.base.pane)),
            ("second", self.base.second.as_ref()),
            ("third", self.third.as_ref()),
        ];
        let mut ordered: Vec<(&'static str, Option<&PaneId>)> = panes
            .iter()
            .filter(|(name, _)| *name != in_use)
            .copied()
            .collect();
        ordered.extend(panes.iter().filter(|(name, _)| *name == in_use).copied());
        ordered
            .into_iter()
            .map(|(name, pane)| match pane {
                Some(pane) => PaneCapture {
                    pane: name,
                    visible: self
                        .base
                        .term
                        .read(pane, ReadSource::Visible, None)
                        .unwrap_or_default(),
                    recent: self
                        .base
                        .term
                        .read(pane, ReadSource::RecentUnwrapped, Some(REPORT_LINES))
                        .unwrap_or_default(),
                },
                None => PaneCapture {
                    pane: name,
                    visible: "(the run did not open this pane)".to_owned(),
                    recent: String::new(),
                },
            })
            .collect()
    }
}

// ---- terminal helpers ----

/// Turns a terminal error into the end of the step.
fn failed(error: impl std::fmt::Display) -> Abort {
    Abort::Failed(error.to_string())
}

/// Turns a tmux error into the end of the step.
fn tmux_failed(error: impl std::fmt::Display) -> Abort {
    Abort::Failed(format!("tmux failed: {error}"))
}

/// A failing step with this reason.
fn fail(reason: impl Into<String>) -> Result<Step, Abort> {
    Ok(Step::Fail {
        reason: reason.into(),
    })
}

/// What one poll of a wait saw.
struct Seen {
    /// The visible text of the pane.
    visible: String,
    /// The recent text of the pane, when the wait reads it.
    recent: String,
    /// How long the visible text has not changed.
    quiet: Duration,
}

/// Which texts a wait reads.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Reads {
    /// The visible text only.
    Visible,
    /// The visible and the recent text.
    Both,
}

impl Run<'_> {
    /// Submits a line with `pane run`.
    fn submit(&self, pane: &PaneId, text: &str) -> Result<(), Abort> {
        self.term.run(pane, text).map_err(failed)
    }

    /// Types text without Enter, then waits out the composer's paste window: typed text arrives
    /// as fast characters, and an Enter within 120 ms of them is a line break, not a submit.
    fn typed(&self, pane: &PaneId, text: &str) -> Result<(), Abort> {
        self.term.send_text(pane, text).map_err(failed)?;
        self.clock.sleep(AFTER_TYPED);
        Ok(())
    }

    /// Sends one key; after Esc it waits, so no stray character follows the key.
    fn key(&self, pane: &PaneId, key: &str) -> Result<(), Abort> {
        self.term.send_keys(pane, &[key]).map_err(failed)?;
        if key == "esc" {
            self.clock.sleep(AFTER_ESC);
        }
        Ok(())
    }

    /// The recent text of the pane.
    fn recent(&self, pane: &PaneId) -> Result<String, Abort> {
        self.term
            .read(pane, ReadSource::RecentUnwrapped, Some(RECENT_LINES))
            .map_err(failed)
    }

    /// The visible text of the pane.
    fn visible(&self, pane: &PaneId) -> Result<String, Abort> {
        self.term
            .read(pane, ReadSource::Visible, None)
            .map_err(failed)
    }

    /// Polls the pane until `done` holds or the scaled limit passes; `Ok(true)` when it held.
    /// With `fallback`, a submit that left the screen quiet and unchanged gets one Enter, as
    /// the first suite's reply wait does.
    fn wait_until(
        &mut self,
        pane: &PaneId,
        base: u64,
        reads: Reads,
        fallback: bool,
        mut done: impl FnMut(&Seen) -> bool,
    ) -> Result<bool, Abort> {
        let limit = self.limit(base);
        let started = self.clock.now();
        let mut watch = Watch::new(started);
        let mut fallback_sent = !fallback;
        loop {
            self.check_stop()?;
            let now = self.clock.now();
            if now >= self.deadline {
                return Err(Abort::Deadline);
            }
            let recent = match reads {
                Reads::Both => self.recent(pane)?,
                Reads::Visible => String::new(),
            };
            let visible = self.watch_visible(pane, &mut watch)?;
            let seen = Seen {
                visible,
                recent,
                quiet: now.saturating_sub(watch.changed_at),
            };
            if done(&seen) {
                return Ok(true);
            }
            if !fallback_sent
                && now.saturating_sub(started) <= FALLBACK_WINDOW
                && seen.quiet >= Duration::from_secs(QUIET_REPLY)
                && !card_words(&seen.visible)
                && counting_values(&seen.visible, COUNT_MARKER).is_empty()
            {
                self.term.send_keys(pane, &["enter"]).map_err(failed)?;
                fallback_sent = true;
                self.fallbacks += 1;
                self.step_fallbacks += 1;
            }
            if now.saturating_sub(started) >= limit {
                return Ok(false);
            }
            self.clock.sleep(POLL_INTERVAL);
        }
    }
}

// ---- screen helpers the steps share ----

/// Whether any normalised line holds the text.
fn any_line(text: &str, needle: &str) -> bool {
    normalised(text).iter().any(|line| line.contains(needle))
}

/// Whether a line holds the text, or the composer's rows (the lines that start with `│`)
/// joined with single spaces do: a narrow composer wraps a draft at its spaces.
fn draft_holds(text: &str, needle: &str) -> bool {
    let rows: Vec<&str> = text
        .lines()
        .filter(|line| line.trim_start().starts_with('│'))
        .collect();
    any_line(text, needle) || normalised(&rows.join("\n")).join(" ").contains(needle)
}

/// How many normalised lines hold the text.
fn lines_holding(text: &str, needle: &str) -> usize {
    normalised(text)
        .iter()
        .filter(|line| line.contains(needle))
        .count()
}

/// The line without a leading prompt or user-row marker.
fn without_marker(line: &str) -> &str {
    line.trim_start_matches(|c: char| c == '>' || c == '›' || c == '❯' || c.is_whitespace())
}

/// Whether the palette lists `/version` and shows its footer.
fn palette_lists_version(visible: &str) -> bool {
    palette_state(visible).footer && any_line(visible, VERSION_COMMAND)
}

/// Whether the recent text holds the `/version` block: its command line, then text.
fn version_block(recent: &str) -> bool {
    let lines = normalised(recent);
    lines.iter().enumerate().any(|(index, line)| {
        without_marker(line) == VERSION_COMMAND
            && lines[index + 1..]
                .iter()
                .take(3)
                .any(|next| !next.is_empty())
    })
}

/// The local commands the Talaria group of the palette lacks, in order.
fn group_lacks(visible: &str) -> Vec<&'static str> {
    let lines = normalised(visible);
    let Some(header) = lines.iter().rposition(|line| line == PALETTE_GROUP) else {
        return LOCAL_COMMANDS.to_vec();
    };
    LOCAL_COMMANDS
        .iter()
        .copied()
        .filter(|name| {
            !lines[header + 1..].iter().any(|line| {
                line.split_whitespace().next() == Some(*name) && holds_word(line, PALETTE_TAG)
            })
        })
        .collect()
}

/// How many lines of the recent text mention an interrupt.
fn interrupt_lines(recent: &str) -> usize {
    normalised(recent)
        .iter()
        .filter(|line| line.to_lowercase().contains("interrupt"))
        .count()
}

/// The non-blank normalised lines of `after` that `before` does not hold, counted as a
/// multiset, leaving out lines that hold `skip`.
fn new_lines(before: &str, after: &str, skip: &str) -> Vec<String> {
    let mut old: Vec<String> = normalised(before)
        .into_iter()
        .filter(|line| !line.is_empty())
        .collect();
    let mut added = Vec::new();
    for line in normalised(after) {
        if line.is_empty() {
            continue;
        }
        if let Some(at) = old.iter().position(|seen| *seen == line) {
            old.swap_remove(at);
        } else if !line.contains(skip) {
            added.push(line);
        }
    }
    added
}

/// Whether the visible text shows the collapsed task panel: one header row with its hint and
/// no task rows after it.
fn panel_collapsed(visible: &str) -> bool {
    if !matches!(task_view(visible), TaskView::Collapsed { .. }) {
        return false;
    }
    let lines = normalised(visible);
    let headers: Vec<usize> = lines
        .iter()
        .enumerate()
        .filter(|(_, line)| line.starts_with(TASK_COLLAPSED))
        .map(|(index, _)| index)
        .collect();
    let &[header] = &headers[..] else {
        return false;
    };
    lines[header].ends_with(TASK_EXPAND_HINT)
        && !lines[header + 1..]
            .iter()
            .any(|line| TASK_ROW_MARKS.iter().any(|mark| line.contains(mark)))
}

/// Whether the visible text shows the expanded panel with the sleep step in progress.
fn panel_on_sleep(visible: &str) -> bool {
    matches!(
        task_view(visible),
        TaskView::Expanded { active_row: Some(row) } if row.to_lowercase().contains("sleep")
    )
}

/// Whether the visible text shows the expanded panel with a row in progress.
fn panel_expanded(visible: &str) -> bool {
    matches!(
        task_view(visible),
        TaskView::Expanded {
            active_row: Some(_)
        }
    )
}

/// The word as a shell word: bare when it is plain, else single-quoted.
fn shell_word(text: &str) -> String {
    let plain = !text.is_empty()
        && text
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "/._-+".contains(c));
    if plain {
        text.to_owned()
    } else {
        format!("'{}'", text.replace('\'', "'\\''"))
    }
}

// ---- the steps ----

impl Run2a<'_> {
    /// A failed shell command shows its tool cell, its output row and the reply.
    fn tool_shell(&mut self) -> Result<Step, Abort> {
        let pane = self.base.pane.clone();
        let token = self.base.token();
        self.base
            .submit(&pane, &prompt_after(&token, SHELL_ACTION))?;
        match self
            .base
            .wait_for_reply(&pane, &token, TOOL_LIMIT, TOKEN_REASON)?
        {
            Step::Pass => {}
            other => return Ok(other),
        }
        let recent = self.base.recent(&pane)?;
        let header = tool_header(&recent, SHELL_COMMAND)
            .filter(|header| header.mark == Mark::Failure && header.exit.is_some());
        let Some(header) = header else {
            return fail(format!(
                "no tool cell '{SHELL_HEADER}{SHELL_COMMAND})  {SHELL_EXIT_MARK}<n>' appeared"
            ));
        };
        if !rows_after(&recent, header.line_index, 8, SHELL_NO_SUCH_DIR) {
            return fail(format!(
                "the failing command's output row '{SHELL_NO_SUCH_DIR}' is missing"
            ));
        }
        Ok(Step::Pass)
    }

    /// A file edit shows its numbered diff and reaches the disk; one retry when the model
    /// used no file editing tool.
    fn tool_edit(&mut self) -> Result<Step, Abort> {
        let pane = self.base.pane.clone();
        let notes = self.base.scratch.join(NOTES_FILE);
        for attempt in 1..=2 {
            let _ = fs::write(&notes, NOTES_TEXT);
            let token = self.base.token();
            let action = if attempt == 1 {
                EDIT_ACTION
            } else {
                EDIT_RETRY_ACTION
            };
            self.base.submit(&pane, &prompt_after(&token, action))?;
            match self
                .base
                .wait_for_reply(&pane, &token, TOOL_LIMIT, TOKEN_REASON)?
            {
                Step::Pass => {}
                other => return Ok(other),
            }
            let recent = self.base.recent(&pane)?;
            let edited = normalised(&recent)
                .iter()
                .any(|line| EDIT_HEADERS.iter().any(|start| line.starts_with(start)));
            if !edited {
                continue;
            }
            // Hermes cuts a long path in the cell's subject, so the file name can be lost.
            let Some(header) = edit_header(&recent, NOTES_FILE) else {
                return fail(format!("the edit cell does not name {NOTES_FILE}"));
            };
            if !header.stat_ok {
                return fail(format!("the edit cell lacks '{EDIT_STAT}'"));
            }
            if has_debris(&recent) {
                return fail("the edit cell shows Hermes's diff header or escape debris");
            }
            if !has_diff_rows(&recent, header.line_index) {
                return fail(format!(
                    "the diff rows '{}' and '{}' are missing",
                    DIFF_ROWS[0], DIFF_ROWS[1]
                ));
            }
            let text = fs::read_to_string(&notes).unwrap_or_default();
            if !text.contains("gamma") || text.contains("beta") {
                return fail("notes.txt does not hold the edit on disk");
            }
            return Ok(Step::Pass);
        }
        fail("the model did not use a file editing tool in two attempts")
    }

    /// The slash palette lists, completes and runs a command, shows the Talaria group and
    /// closes.
    fn palette(&mut self) -> Result<Step, Abort> {
        let pane = self.base.pane.clone();
        let run = &mut self.base;
        run.typed(&pane, PALETTE_QUERY)?;
        if !run.wait_until(&pane, PALETTE_OPEN, Reads::Visible, false, |seen| {
            palette_lists_version(&seen.visible)
        })? {
            return fail(format!(
                "the slash palette did not list /version with its footer within {} s",
                run.limit_secs(PALETTE_OPEN)
            ));
        }
        run.key(&pane, "enter")?;
        if !run.wait_until(&pane, PALETTE_BLOCK, Reads::Both, false, |seen| {
            version_block(&seen.recent) && seen.quiet >= Duration::from_secs(QUIET_BLOCK)
        })? {
            return fail(format!(
                "/vers and Enter did not print the /version block within {} s",
                run.limit_secs(PALETTE_BLOCK)
            ));
        }
        run.typed(&pane, "/")?;
        let mut lacks = LOCAL_COMMANDS.to_vec();
        run.wait_until(&pane, PALETTE_BROWSE, Reads::Visible, false, |seen| {
            lacks = group_lacks(&seen.visible);
            lacks.is_empty()
        })?;
        // The palette is closed either way, so a failure leaves the composer as it found it.
        run.key(&pane, "esc")?;
        run.key(&pane, "ctrl+c")?;
        if !lacks.is_empty() {
            return fail(format!(
                "the Talaria group of the palette lacks: {}",
                lacks.join(", ")
            ));
        }
        if !run.wait_until(&pane, PALETTE_CLOSE, Reads::Visible, false, |seen| {
            !any_line(&seen.visible, PALETTE_FOOTER)
        })? {
            return fail("the palette stayed open after Esc and Ctrl+C");
        }
        Ok(Step::Pass)
    }

    /// An `@` mention lists the file, Tab inserts it and the reply reads its content.
    fn mention(&mut self) -> Result<Step, Abort> {
        let pane = self.base.pane.clone();
        let run = &mut self.base;
        // The list opens only for typed text, and one send arrives as a paste burst, so the
        // last character goes on its own.
        let (head, last) = MENTION_QUERY.split_at(MENTION_QUERY.len() - 1);
        run.typed(&pane, head)?;
        run.typed(&pane, last)?;
        if !run.wait_until(&pane, MENTION_LIST, Reads::Visible, false, |seen| {
            mention_list(&seen.visible)
                .is_some_and(|row| row.file == MENTION_ROW && row.folder == MENTION_FOLDER)
        })? {
            return fail(format!(
                "the @ list did not show {MENTION_ROW} with its folder {MENTION_FOLDER} within {} s",
                run.limit_secs(MENTION_LIST)
            ));
        }
        run.key(&pane, "tab")?;
        if !run.wait_until(&pane, MENTION_TAB, Reads::Visible, false, |seen| {
            any_line(&seen.visible, MENTION_INSERT)
        })? {
            return fail(format!(
                "Tab did not insert {MENTION_INSERT} within {} s",
                run.limit_secs(MENTION_TAB)
            ));
        }
        run.typed(&pane, MENTION_REST)?;
        run.key(&pane, "enter")?;
        if !run.wait_until(&pane, MENTION_REPLY, Reads::Both, false, |seen| {
            MENTION_WORDS
                .iter()
                .any(|word| holds_word(&seen.recent, word))
                && seen.quiet >= Duration::from_secs(QUIET_REPLY)
        })? {
            return fail(format!(
                "the reply did not mention the file's content (average) within {} s",
                run.limit_secs(MENTION_REPLY)
            ));
        }
        Ok(Step::Pass)
    }

    /// A `!` command shows its output; the refused one leaves the folder in place.
    fn shell(&mut self) -> Result<Step, Abort> {
        let pane = self.base.pane.clone();
        let token = self.base.token().join();
        self.shell_token = Some(token.clone());
        let run = &mut self.base;
        run.typed(&pane, &format!("!echo {token}"))?;
        run.key(&pane, "enter")?;
        let command = format!("echo {token}");
        if !run.wait_until(&pane, SHELL_CHECK, Reads::Both, false, |seen| {
            shell_cell(&seen.recent, &command)
                .is_some_and(|cell| cell.output_has.iter().take(5).any(|row| *row == token))
        })? {
            return fail(format!(
                "the output of the ! command did not appear within {} s",
                run.limit_secs(SHELL_CHECK)
            ));
        }
        run.typed(&pane, REFUSED_SHELL_LINE)?;
        run.key(&pane, "enter")?;
        if !run.wait_until(&pane, SHELL_CHECK, Reads::Both, false, |seen| {
            any_line(&seen.recent, SHELL_REFUSAL)
        })? {
            return fail(format!(
                "the refusal '{SHELL_REFUSAL}' did not appear within {} s",
                run.limit_secs(SHELL_CHECK)
            ));
        }
        if !run.scratch.join("keepme").join("keep.txt").is_file() {
            return fail("the refused command removed the keepme folder");
        }
        Ok(Step::Pass)
    }

    /// A side question is answered in its own cell from the first message.
    fn btw(&mut self) -> Result<Step, Abort> {
        let pane = self.base.pane.clone();
        let word = self.first_token().word;
        let run = &mut self.base;
        run.submit(&pane, &format!("/btw {BTW_QUESTION}"))?;
        let mut cell = None;
        run.wait_until(&pane, BTW_LIMIT, Reads::Both, true, |seen| {
            cell = side_cell(&seen.recent, BTW_QUESTION);
            cell.is_some()
        })?;
        let Some(cell) = cell else {
            return fail(format!(
                "no side answer cell appeared within {} s",
                run.limit_secs(BTW_LIMIT)
            ));
        };
        if cell.failed {
            return fail(format!("the side question failed: {}", cell.answer));
        }
        if cell.empty {
            return fail("the side answer was empty");
        }
        if !holds_word(&cell.answer, word) {
            return fail("the side answer did not contain the word of the first token");
        }
        Ok(Step::Pass)
    }

    /// The transcript pager opens, follows, finds the first token and the tool output, and
    /// closes without interrupting anything.
    fn pager(&mut self) -> Result<Step, Abort> {
        let pane = self.base.pane.clone();
        let first = self.first_token().join();
        let run = &mut self.base;
        let before = interrupt_lines(&run.recent(&pane)?);
        run.key(&pane, "ctrl+o")?;
        let mut state = None;
        run.wait_until(&pane, PAGER_CHECK, Reads::Visible, false, |seen| {
            let pager = pager_state(&seen.visible);
            let done = pager.open && pager.following;
            state = Some(pager);
            done
        })?;
        match state {
            Some(pager) if pager.open && pager.following => {}
            Some(pager) if pager.open => {
                return fail("the pager footer does not say 'following'");
            }
            _ => {
                return fail(format!(
                    "Ctrl+O did not open the transcript pager within {} s",
                    run.limit_secs(PAGER_CHECK)
                ));
            }
        }
        run.typed(&pane, "/")?;
        run.typed(&pane, &first)?;
        run.key(&pane, "enter")?;
        if !run.wait_until(&pane, PAGER_CHECK, Reads::Visible, false, |seen| {
            pager_state(&seen.visible)
                .search
                .is_some_and(|(_, count, query)| query == first && count >= 1)
        })? {
            return fail("searching the pager for the first token found no match");
        }
        run.typed(&pane, "/")?;
        run.typed(&pane, PAGER_TOOL_QUERY)?;
        run.key(&pane, "enter")?;
        if !run.wait_until(&pane, PAGER_CHECK, Reads::Visible, false, |seen| {
            any_line(&seen.visible, SHELL_NO_SUCH_DIR)
                && pager_state(&seen.visible)
                    .search
                    .is_some_and(|(_, count, query)| query == PAGER_TOOL_QUERY && count >= 1)
        })? {
            return fail(format!(
                "the pager did not find the tool output '{SHELL_NO_SUCH_DIR}'"
            ));
        }
        run.key(&pane, "q")?;
        if !run.wait_until(&pane, PAGER_CHECK, Reads::Visible, false, |seen| {
            !pager_state(&seen.visible).open
        })? {
            return fail(format!(
                "q did not return to the inline view within {} s",
                run.limit_secs(PAGER_CHECK)
            ));
        }
        if interrupt_lines(&run.recent(&pane)?) != before {
            return fail("the pager's q or Esc interrupted the agent");
        }
        Ok(Step::Pass)
    }

    /// Ctrl+R finds the first prompt, Enter loads it unsent and Ctrl+C clears it.
    fn history_search(&mut self) -> Result<Step, Abort> {
        let pane = self.base.pane.clone();
        let first = self.first_token();
        let needle = format!("{} and {}", first.word, first.number);
        let run = &mut self.base;
        let visible = run.visible(&pane)?;
        let shown = lines_holding(&visible, &needle);
        // A match row holds the earlier prompt; the query row starts with what was typed, and
        // the box says so in its own line when nothing matches.
        let matched = |text: &str| {
            normalised(text)
                .iter()
                .filter(|line| {
                    line.contains(&needle)
                        && !without_marker(line).starts_with(&needle)
                        && !line.contains(HISTORY_NO_MATCH)
                })
                .count()
        };
        let matched_before = matched(&visible);
        run.key(&pane, "ctrl+r")?;
        run.typed(&pane, &needle)?;
        if !run.wait_until(&pane, HISTORY_BOX, Reads::Visible, false, |seen| {
            matched(&seen.visible) > matched_before
        })? {
            return fail(format!(
                "Ctrl+R did not list the earlier prompt within {} s",
                run.limit_secs(HISTORY_BOX)
            ));
        }
        run.key(&pane, "enter")?;
        if !run.wait_until(&pane, HISTORY_LOAD, Reads::Visible, false, |seen| {
            lines_holding(&seen.visible, &needle) == shown + 1
        })? {
            return fail("Enter did not load the earlier prompt into the composer");
        }
        run.key(&pane, "ctrl+c")?;
        if !run.wait_until(&pane, HISTORY_CLEAR, Reads::Visible, false, |seen| {
            lines_holding(&seen.visible, &needle) == shown
        })? {
            return fail("Ctrl+C did not clear the recalled draft");
        }
        Ok(Step::Pass)
    }

    /// Ctrl+G hands the draft to the editor script and brings it back unsent.
    fn editor(&mut self) -> Result<Step, Abort> {
        let pane = self.base.pane.clone();
        let run = &mut self.base;
        run.typed(&pane, DRAFT)?;
        let before = run.recent(&pane)?;
        run.key(&pane, "ctrl+g")?;
        if !run.wait_until(&pane, EDITOR_LIMIT, Reads::Visible, false, |seen| {
            normalised(&seen.visible)
                .iter()
                .any(|line| without_marker(line) == EDITED_DRAFT)
                && seen.quiet >= Duration::from_secs(QUIET_EDITOR)
        })? {
            return fail(format!(
                "Ctrl+G did not return the edited draft '{EDITED_DRAFT}' within {} s",
                run.limit_secs(EDITOR_LIMIT)
            ));
        }
        let after = run.recent(&pane)?;
        if !new_lines(&before, &after, DRAFT).is_empty() {
            return fail("the edited draft was sent without Enter");
        }
        run.key(&pane, "ctrl+c")?;
        Ok(Step::Pass)
    }

    /// `/theme` prints its report; two bad arguments print their usage lines and never reach
    /// Hermes.
    fn theme(&mut self) -> Result<Step, Abort> {
        let pane = self.base.pane.clone();
        let run = &mut self.base;
        run.submit(&pane, THEME_COMMAND)?;
        let mut present = [false; 5];
        run.wait_until(&pane, THEME_CHECK, Reads::Both, true, |seen| {
            present = theme_report(&seen.recent).present;
            present.iter().all(|line| *line)
        })?;
        if !present.iter().all(|line| *line) {
            let missing: Vec<&str> = THEME_LINES
                .iter()
                .zip(present)
                .filter(|(_, shown)| !shown)
                .map(|(name, _)| *name)
                .collect();
            return fail(format!(
                "/theme did not print its report (missing: {}) within {} s",
                missing.join(", "),
                run.limit_secs(THEME_CHECK)
            ));
        }
        run.submit(&pane, BAD_SKIN)?;
        if !run.wait_until(&pane, THEME_CHECK, Reads::Both, true, |seen| {
            normalised(&seen.recent)
                .iter()
                .any(|line| line.starts_with(USAGE_SKIN))
        })? {
            return fail(format!(
                "{BAD_SKIN} did not print its usage line within {} s",
                run.limit_secs(THEME_CHECK)
            ));
        }
        run.submit(&pane, BAD_THEME)?;
        if !run.wait_until(&pane, THEME_CHECK, Reads::Both, true, |seen| {
            normalised(&seen.recent)
                .iter()
                .any(|line| line.starts_with(USAGE_THEME))
        })? {
            return fail(format!(
                "{BAD_THEME} did not print its usage line within {} s",
                run.limit_secs(THEME_CHECK)
            ));
        }
        let lines = normalised(&run.recent(&pane)?);
        let report = lines
            .iter()
            .rposition(|line| line == THEME_LINES[0])
            .unwrap_or(0);
        if lines[report..].iter().any(|line| line.contains(REFUSED)) {
            return fail(
                "a bad /skin or /theme argument reached Hermes: a 'Hermes refused' line appeared",
            );
        }
        Ok(Step::Pass)
    }

    /// Herdr reports the pane idle, working while a reply streams, and idle after Esc.
    fn pane_state(&mut self) -> Result<Step, Abort> {
        let pane = self.base.pane.clone();
        let run = &mut self.base;
        let pane_get = |run: &Run<'_>| {
            run.term
                .pane_get(&pane)
                .map_err(|error| Abort::Failed(format!("herdr pane get failed: {error}")))
        };
        if agent_state(&pane_get(run)?) != Some(AgentState::Idle) {
            return Ok(Step::Skip {
                reason: PANE_STATE_SKIP.to_owned(),
            });
        }
        run.submit(&pane, COUNT_PROMPT)?;
        if !run.wait_until(&pane, COUNT_LIMIT, Reads::Visible, false, |seen| {
            counting_values(&seen.visible, COUNT_MARKER).len() >= 15
        })? {
            return fail(format!(
                "fewer than 15 counting lines appeared within {} s",
                run.limit_secs(COUNT_LIMIT)
            ));
        }
        if agent_state(&pane_get(run)?) != Some(AgentState::Working) {
            return fail("the pane did not report 'working' while the reply streamed");
        }
        run.key(&pane, "esc")?;
        if !run.wait_until(&pane, SETTLE_LIMIT, Reads::Visible, false, |seen| {
            seen.quiet >= Duration::from_secs(QUIET_REPLY)
        })? {
            return fail("output kept growing after Esc");
        }
        let limit = run.limit(IDLE_LIMIT);
        let started = run.clock.now();
        loop {
            run.check_stop()?;
            let now = run.clock.now();
            if now >= run.deadline {
                return Err(Abort::Deadline);
            }
            if agent_state(&pane_get(run)?) == Some(AgentState::Idle) {
                return Ok(Step::Pass);
            }
            if now.saturating_sub(started) >= limit {
                return fail(format!(
                    "the pane did not report 'idle' within {} s after the interrupt",
                    run.limit_secs(IDLE_LIMIT)
                ));
            }
            run.clock.sleep(POLL_INTERVAL);
        }
    }

    /// The task panel follows a plan, collapses and expands with Ctrl+T, and leaves one record.
    fn checklist(&mut self) -> Result<Step, Abort> {
        let pane = self.base.pane.clone();
        let run = &mut self.base;
        let mut shown = false;
        for _attempt in 1..=2 {
            run.submit(&pane, PLAN_PROMPT)?;
            let mut panel = false;
            run.wait_until(&pane, PANEL_LIMIT, Reads::Visible, true, |seen| {
                panel = panel_on_sleep(&seen.visible);
                panel || seen.quiet >= Duration::from_secs(QUIET_PLAN)
            })?;
            if panel {
                shown = true;
                break;
            }
        }
        if !shown {
            return Ok(Step::Skip {
                reason: CHECKLIST_SKIP.to_owned(),
            });
        }
        run.key(&pane, "ctrl+t")?;
        if !run.wait_until(&pane, TOGGLE_LIMIT, Reads::Visible, false, |seen| {
            panel_collapsed(&seen.visible)
        })? {
            return fail(format!(
                "Ctrl+T did not collapse the task panel to one row within {} s",
                run.limit_secs(TOGGLE_LIMIT)
            ));
        }
        run.key(&pane, "ctrl+t")?;
        if !run.wait_until(&pane, TOGGLE_LIMIT, Reads::Visible, false, |seen| {
            panel_expanded(&seen.visible)
        })? {
            return fail(format!(
                "Ctrl+T did not expand the task panel within {} s",
                run.limit_secs(TOGGLE_LIMIT)
            ));
        }
        let mut left = false;
        let ended = run.wait_until(&pane, TURN_END_LIMIT, Reads::Both, false, |seen| {
            if seen.quiet < Duration::from_secs(QUIET_REPLY) {
                return false;
            }
            if task_view(&seen.visible) == TaskView::Absent {
                return true;
            }
            left = !task_records(&seen.recent).is_empty();
            left
        })?;
        if left {
            return fail("the task panel was still shown after the turn ended");
        }
        if !ended {
            return fail(format!(
                "the task turn did not end within {} s",
                run.limit_secs(TURN_END_LIMIT)
            ));
        }
        let records = task_records(&run.recent(&pane)?).len();
        if records != 1 {
            return fail(format!(
                "the turn ended with {records} 'Tasks <c>/<t> · ' records in scrollback, expected 1"
            ));
        }
        self.checklist_passed = true;
        Ok(Step::Pass)
    }

    /// Writes the tmux section's files: the two big pastes, the dropped file and the launch
    /// script.
    fn write_tmux_files(&mut self) -> Result<PathBuf, Abort> {
        let first = self.base.token();
        let secret = self.base.token();
        let second = self.base.token();
        let scratch = self.base.scratch.clone();
        let big = |token: &Token| {
            (1..=BIG_LINES)
                .map(|index| {
                    if index == MARKER_LINE {
                        format!("{MARKER_TEXT} {}\n", token.join())
                    } else {
                        format!("line {index}\n")
                    }
                })
                .collect::<String>()
        };
        write_file(&scratch.join("big1.txt"), &big(&first), 0o600)?;
        write_file(&scratch.join("big2.txt"), &big(&second), 0o600)?;
        let hex: String = (0..8)
            .map(|_| char::from_digit(self.base.rng.random_range(0..16u32), 16).unwrap_or('0'))
            .collect();
        let name = format!("notes-{hex}.txt");
        make_private_dir(&self.drop)?;
        write_file(
            &self.drop.join(&name),
            &format!("The secret word is {}.\n", secret.join()),
            0o600,
        )?;
        self.drop_file = Some(name);
        self.markers = Some(Markers {
            first,
            secret,
            second,
        });
        let launch = scratch.join("launch.sh");
        let line = crate::config::launch_line_env(
            &[("EDITOR", self.editor_script().as_str())],
            &self.base.cfg.talaria,
            &[
                "--no-ensure",
                "--profile",
                &self.base.cfg.profile,
                "--model",
                &self.base.cfg.model,
                "--provider",
                &self.base.cfg.provider,
            ],
        );
        // The launch line already echoes its exit; the marker file is what tmux leaves behind.
        let line = line
            .strip_suffix("; echo SMOKE_EXIT:$?")
            .unwrap_or(&line)
            .to_owned();
        let marker = scratch.join("tmux-exit.txt");
        let script = format!(
            "#!/bin/sh\nunset HERDR_ENV HERDR_PANE_ID HERDR_BIN_PATH\n{line}\necho \"SMOKE_EXIT:$?\" > '{}'\n",
            marker.display()
        );
        write_file(&launch, &script, 0o700)?;
        Ok(launch)
    }

    /// Talaria starts inside tmux in a third pane.
    fn tmux_pane(&mut self) -> Result<Step, Abort> {
        let launch = self.write_tmux_files()?;
        let third = self
            .base
            .term
            .split_pane(&self.base.pane, &self.base.scratch)
            .map_err(failed)?;
        self.third = Some(third.clone());
        if let Err(abort) = self.base.wait_for_shell(&third, SHELL_LIMIT) {
            return finish(abort);
        }
        // The status line and a pane border line can name the host, so both are switched off
        // for this session.
        let line = format!(
            "{} new-session -s {} '{}' \\; set-option status off \\; set-option pane-border-status off",
            shell_word(&self.cfg2.tmux.to_string_lossy()),
            self.session,
            launch.display()
        );
        self.base.submit(&third, &line)?;
        self.guard = Some(TmuxGuard::new(self.paster, &self.session));
        let limit = self.base.limit(SESSION_LIMIT);
        let started = self.base.clock.now();
        let mut watch = Watch::new(started);
        loop {
            self.base.check_stop()?;
            let now = self.base.clock.now();
            if now >= self.base.deadline {
                return Err(Abort::Deadline);
            }
            // A clean exit this early shows as a missing session; a failing one is named.
            if let Some(code) = self.paster.exit_code().filter(|code| *code != 0) {
                return fail(format!(
                    "talaria exited with code {code} before showing its screen"
                ));
            }
            let exists = self
                .paster
                .session_exists(&self.session)
                .map_err(tmux_failed)?;
            self.base.watch_visible(&third, &mut watch)?;
            if exists && now.saturating_sub(watch.changed_at) >= Duration::from_secs(QUIET_TMUX) {
                return Ok(Step::Pass);
            }
            if now.saturating_sub(started) >= limit {
                return fail(format!(
                    "tmux did not start the talaria session within {} s",
                    self.base.limit_secs(SESSION_LIMIT)
                ));
            }
            self.base.clock.sleep(POLL_INTERVAL);
        }
    }

    /// Pastes the text into the tmux session.
    fn paste(&self, text: &str) -> Result<(), Abort> {
        self.paster.paste(&self.session, text).map_err(tmux_failed)
    }

    /// Waits for the token in the third pane's recent text and a settled screen.
    fn tmux_reply(&mut self, token: &Token, reason: &str) -> Result<Step, Abort> {
        let pane = self.third();
        let joined = token.join();
        if !self
            .base
            .wait_until(&pane, PASTE_REPLY, Reads::Both, false, |seen| {
                count_token(&seen.recent, &joined) > 0
                    && seen.quiet >= Duration::from_secs(QUIET_REPLY)
            })?
        {
            return fail(format!(
                "{reason} within {} s",
                self.base.limit_secs(PASTE_REPLY)
            ));
        }
        Ok(Step::Pass)
    }

    /// A 120-line bracketed paste collapses to a label and is sent in full.
    fn paste_collapse(&mut self) -> Result<Step, Abort> {
        let pane = self.third();
        let text = fs::read_to_string(self.base.scratch.join("big1.txt")).map_err(failed)?;
        let token = self
            .markers
            .as_ref()
            .map(|m| m.first)
            .ok_or_else(missing_files)?;
        self.paste(&text)?;
        let mut leak = false;
        let labelled = self
            .base
            .wait_until(&pane, LABEL_LIMIT, Reads::Visible, false, |seen| {
                let labels = paste_labels(&seen.visible);
                let one = matches!(&labels[..], [label] if label.number == 1 && label.lines == 120);
                leak = one && any_line(&seen.visible, MARKER_TEXT);
                one
            })?;
        if !labelled {
            return fail(format!(
                "the pasted text did not collapse into '[Pasted text #1 +120 lines]' within {} s",
                self.base.limit_secs(LABEL_LIMIT)
            ));
        }
        if leak {
            return fail("the collapsed paste still shows its text in the composer");
        }
        self.base.typed(&pane, PASTE_QUESTION)?;
        self.base.key(&pane, "enter")?;
        self.tmux_reply(
            &token,
            "the reply did not contain the marker from the pasted text",
        )
    }

    /// A pasted path of a file outside the project becomes a file label and is read.
    fn file_drop(&mut self) -> Result<Step, Abort> {
        let pane = self.third();
        let name = self.drop_file.clone().ok_or_else(missing_files)?;
        let token = self
            .markers
            .as_ref()
            .map(|m| m.secret)
            .ok_or_else(missing_files)?;
        let path = self.drop.join(&name);
        self.paste(&path.to_string_lossy())?;
        if !self
            .base
            .wait_until(&pane, LABEL_LIMIT, Reads::Visible, false, |seen| {
                file_label(&seen.visible, &name)
            })?
        {
            return fail(format!(
                "the pasted file path did not become '[File #1: {name}]' within {} s",
                self.base.limit_secs(LABEL_LIMIT)
            ));
        }
        self.base.typed(&pane, DROP_QUESTION)?;
        self.base.key(&pane, "enter")?;
        self.tmux_reply(
            &token,
            "the reply did not contain the word from the dropped file",
        )
    }

    /// A second paste label survives the external editor and is sent in full.
    fn paste_editor(&mut self) -> Result<Step, Abort> {
        let pane = self.third();
        let text = fs::read_to_string(self.base.scratch.join("big2.txt")).map_err(failed)?;
        let token = self
            .markers
            .as_ref()
            .map(|m| m.second)
            .ok_or_else(missing_files)?;
        self.paste(&text)?;
        let mut number = None;
        self.base
            .wait_until(&pane, LABEL_LIMIT, Reads::Visible, false, |seen| {
                number = paste_labels(&seen.visible)
                    .iter()
                    .find(|label| label.number > 1 && label.lines == 120)
                    .map(|label| label.number);
                number.is_some()
            })?;
        let Some(number) = number else {
            return fail(format!(
                "the second paste did not collapse into a '[Pasted text #<n> +120 lines]' label within {} s",
                self.base.limit_secs(LABEL_LIMIT)
            ));
        };
        self.base.key(&pane, "ctrl+g")?;
        let edited = format!("[Pasted text #{number} +120 lines] plus-two");
        if !self
            .base
            .wait_until(&pane, PASTE_EDITOR_LIMIT, Reads::Visible, false, |seen| {
                draft_holds(&seen.visible, &edited)
            })?
        {
            return fail("the paste label did not survive the external editor");
        }
        self.base.typed(&pane, PASTE_EDITOR_QUESTION)?;
        self.base.key(&pane, "enter")?;
        self.tmux_reply(
            &token,
            "the reply did not contain the marker from the second paste",
        )
    }

    /// `/quit` in the tmux pane ends talaria with exit code 0 and the session with it.
    fn tmux_quit(&mut self) -> Result<Step, Abort> {
        let pane = self.third();
        self.base.typed(&pane, QUIT_LINE)?;
        self.base.key(&pane, "enter")?;
        let limit = self.base.limit(TMUX_QUIT_LIMIT);
        let started = self.base.clock.now();
        loop {
            self.base.check_stop()?;
            let now = self.base.clock.now();
            if now >= self.base.deadline {
                return Err(Abort::Deadline);
            }
            let exists = self
                .paster
                .session_exists(&self.session)
                .map_err(tmux_failed)?;
            let code = self.paster.exit_code();
            if !exists && code == Some(0) {
                return Ok(Step::Pass);
            }
            if now.saturating_sub(started) >= limit {
                if code != Some(0) {
                    return fail(format!(
                        "/quit in the tmux pane did not end talaria with exit code 0 within {} s",
                        self.base.limit_secs(TMUX_QUIT_LIMIT)
                    ));
                }
                return fail("the tmux session was still running after /quit");
            }
            self.base.clock.sleep(POLL_INTERVAL);
        }
    }
}

/// The resumed pane's added checks: no local cell came back, the pager finds the first
/// token, and a finished plan's record shows once.
fn resumed_checks(
    run: &mut Run<'_>,
    pane: &PaneId,
    first: &str,
    shell: Option<&str>,
    checklist: bool,
) -> Result<Step, Abort> {
    // Read before the pager opens, so no pager frame counts.
    let recent = run.recent(pane)?;
    let lines = normalised(&recent);
    let local = lines.iter().any(|line| {
        shell.is_some_and(|token| line.contains(&format!("! echo {token}")) || line.contains(token))
            || line.starts_with(SIDE_HEADER)
            || line.contains(SIDE_FOOTER)
    });
    if local {
        return fail(
            "a local cell was printed again after resume ('! echo' or 'btw ·'): Hermes does not store it",
        );
    }
    let records = lines_holding(&recent, TASK_RESUMED);

    const PAGER_REASON: &str = "the pager of the resumed session did not find the first token";
    run.key(pane, "ctrl+o")?;
    if !run.wait_until(pane, PAGER_CHECK, Reads::Visible, false, |seen| {
        pager_state(&seen.visible).open
    })? {
        return fail(PAGER_REASON);
    }
    run.typed(pane, "/")?;
    run.typed(pane, first)?;
    run.key(pane, "enter")?;
    if !run.wait_until(pane, PAGER_CHECK, Reads::Visible, false, |seen| {
        pager_state(&seen.visible)
            .search
            .is_some_and(|(_, count, query)| query == first && count >= 1)
    })? {
        return fail(PAGER_REASON);
    }
    run.key(pane, "q")?;
    if !run.wait_until(pane, PAGER_CHECK, Reads::Visible, false, |seen| {
        !pager_state(&seen.visible).open
    })? {
        return fail(format!(
            "q did not return to the inline view within {} s",
            run.limit_secs(PAGER_CHECK)
        ));
    }
    if checklist && records != 1 {
        return fail(format!(
            "the resumed task list was printed {records} times, expected 1"
        ));
    }
    Ok(Step::Pass)
}

/// The end of a step whose files were never written.
fn missing_files() -> Abort {
    Abort::Failed("the tmux section's files were not written".to_owned())
}

/// The scratch project: a private folder with the files of the steps.
fn make_scratch(scratch: &Path) {
    let _ = make_private_dir(scratch);
    let _ = fs::write(scratch.join(NOTES_FILE), NOTES_TEXT);
    let _ = fs::create_dir_all(scratch.join("src"));
    let _ = fs::write(
        scratch.join("src").join("calc.py"),
        "def average(values): return sum(values) / len(values)\n",
    );
    let _ = fs::create_dir_all(scratch.join("keepme"));
    let _ = fs::write(scratch.join("keepme").join("keep.txt"), "keep\n");
    let _ = write_file(
        &scratch.join("smoke-editor.sh"),
        "#!/bin/sh\nprintf ' plus-two' >> \"$1\"\n",
        0o700,
    );
}

/// Makes a folder only its owner may enter.
fn make_private_dir(path: &Path) -> Result<(), Abort> {
    fs::create_dir_all(path).map_err(failed)?;
    set_mode(path, 0o700)
}

/// Writes a file with the given mode.
fn write_file(path: &Path, text: &str, mode: u32) -> Result<(), Abort> {
    fs::write(path, text).map_err(failed)?;
    set_mode(path, mode)
}

/// Sets the permission bits of a path.
fn set_mode(path: &Path, mode: u32) -> Result<(), Abort> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(mode)).map_err(failed)?;
    }
    #[cfg(not(unix))]
    {
        let _ = (path, mode);
    }
    Ok(())
}
