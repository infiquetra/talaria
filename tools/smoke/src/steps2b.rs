//! The screen-driven session-control smoke suite for #238.

use std::cell::RefCell;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::AtomicBool;
use std::time::Duration;

use rand::rngs::StdRng;
use rand::{RngExt, SeedableRng};

use crate::screen::{POLL_INTERVAL, count_token};
use crate::screen2a::normalised;
use crate::screen2b::{
    context_header, context_rows, folder_segment, lines_after_last, picker_state, print_check,
    queue_indicator, rewind_list, rewound_line, status_approvals, usage_block,
};
use crate::steps::{
    AFTER_ESC, Abort, RECENT_LINES, REPORT_LINES, Run, Step, TabGuard, Watch, panic_message,
};
use crate::terminal::{ReadSource, TermError};
use crate::wording2b::*;
use crate::{Clock, PaneCapture, SmokeConfig, SmokeReport, StepOutcome, Terminal};

const STEPS: [&str; 15] = [
    "startup",
    "context-empty",
    "permission",
    "model-picker",
    "model-refusal",
    "rewind",
    "plain-answer",
    "steer-idle",
    "context",
    "hermes-commands",
    "rename-export",
    "queue",
    "cd",
    "quit",
    "headless",
];
const FIRST_CODE: &str = "Remember the code word heron. Reply ok.";
const SECOND_CODE: &str = "Now remember the code word lark instead. Reply ok.";
const CHECK_CODE: &str = "Which code word should you remember? One word.";
const STEER: &str = "/steer Reply with the word idle in capital letters.";
const STEER_MARKER: &str = "Reply with the word idle";
const LONG: &str =
    "Run the shell command sleep 25, then reply with the word first in capital letters.";
const LONG_MARKER: &str = "Run the shell command sleep 25";
const OMEGA: &str = "Reply with the word omega in capital letters.";

/// Removes only a scratch directory successfully created by this run.
struct Scratch(PathBuf);
impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn make_scratch(path: &Path) -> Result<Scratch, std::io::Error> {
    fs::create_dir(path)?;
    let guard = Scratch(path.to_path_buf());
    fs::set_permissions(path, fs::Permissions::from_mode(0o700))?;
    for name in ["a", "b"] {
        let folder = path.join(name);
        fs::create_dir(&folder)?;
        for args in [
            vec!["init", "-b", "main"],
            vec![
                "-c",
                "user.name=smoke",
                "-c",
                "user.email=smoke@example.invalid",
                "commit",
                "--allow-empty",
                "-m",
                "init",
            ],
        ] {
            let output = Command::new("git")
                .arg("-C")
                .arg(&folder)
                .args(args)
                .env_clear()
                .output()?;
            if !output.status.success() {
                return Err(std::io::Error::other(
                    String::from_utf8_lossy(&output.stderr).into_owned(),
                ));
            }
        }
    }
    fs::create_dir(path.join("empty-home"))?;
    Ok(guard)
}

/// Runs the fifteen checks, stopping at the first failure or unsupported-key skip.
pub fn run_smoke_2b(
    term: &dyn Terminal,
    clock: &dyn Clock,
    cfg: &SmokeConfig,
    stop: &AtomicBool,
) -> SmokeReport {
    let start = clock.now();
    let mut rng = cfg
        .seed
        .map_or_else(|| StdRng::from_rng(&mut rand::rng()), StdRng::seed_from_u64);
    let alphabet = b"abcdefghijklmnopqrstuvwxyz0123456789";
    let id: String = (0..8)
        .map(|_| alphabet[rng.random_range(0..alphabet.len())] as char)
        .collect();
    let label = format!("talaria-smoke-{id}");
    let scratch = cfg.temp_root.join(&label);
    let mut report = SmokeReport {
        outcomes: Vec::new(),
        version: term.version(),
        elapsed: Duration::ZERO,
        fallbacks: 0,
        interrupted: false,
        unclosed_tab: None,
        unclosed_session: None,
        tab: None,
        scratch: scratch.clone(),
        fallback_steps: Vec::new(),
        notes: Vec::new(),
    };
    let _scratch = match make_scratch(&scratch) {
        Ok(guard) => guard,
        Err(error) => {
            report.outcomes.push(StepOutcome::Fail {
                step: "startup",
                reason: format!("the scratch folder could not be created: {error}"),
                capture: Vec::new(),
            });
            report.elapsed = clock.now().saturating_sub(start);
            return report;
        }
    };
    let tab = match term.create_tab(&label, &scratch.join("a")) {
        Ok(tab) => tab,
        Err(error) => {
            report.outcomes.push(StepOutcome::Fail {
                step: "startup",
                reason: format!("the tab could not be created: {error}"),
                capture: Vec::new(),
            });
            report.elapsed = clock.now().saturating_sub(start);
            return report;
        }
    };
    report.tab = Some(tab.tab.clone());
    let cleanup = RefCell::new(None);
    let guard = TabGuard::new(term, tab.tab, &cleanup);
    let mut run = Run2b {
        base: Run {
            term,
            clock,
            cfg,
            stop,
            deadline: start + cfg.deadline,
            rng,
            tokens: Vec::new(),
            scratch: scratch.join("a"),
            answered_at: None,
            fallbacks: 0,
            step_fallbacks: 0,
            fallback_steps: Vec::new(),
            interrupted: false,
            pane: tab.pane,
            second: None,
            counts: (0, 0),
        },
        scratch,
        title: format!("fixture smoke {id}"),
        pending: false,
        fallback_sent: false,
    };
    for name in STEPS {
        let started = clock.now();
        run.base.step_fallbacks = 0;
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| run.step(name)))
            .unwrap_or_else(|panic| {
                Err(Abort::Failed(format!(
                    "internal error: {}",
                    panic_message(&panic)
                )))
            });
        if run.base.step_fallbacks > 0 {
            run.base.fallback_steps.push(name);
        }
        match result {
            Ok(Step::Pass) => report.outcomes.push(StepOutcome::Pass {
                step: name,
                elapsed: clock.now().saturating_sub(started),
            }),
            Ok(Step::Skip { reason }) => {
                if cfg.require {
                    report.outcomes.push(StepOutcome::Fail {
                        step: name,
                        reason: format!("skipped: {reason}; TALARIA_LIVE_REQUIRE=1"),
                        capture: run.capture(),
                    });
                } else {
                    report
                        .outcomes
                        .push(StepOutcome::Skip { step: name, reason });
                }
                break;
            }
            result => {
                let reason = match result {
                    Ok(Step::Fail { reason }) | Err(Abort::Failed(reason)) => reason,
                    Err(Abort::Deadline) => format!("deadline exceeded in step {name}"),
                    Err(Abort::Interrupted) => {
                        report.interrupted = true;
                        "interrupted by a signal".to_owned()
                    }
                    _ => unreachable!("passing and skipped outcomes were handled above"),
                };
                report.outcomes.push(StepOutcome::Fail {
                    step: name,
                    reason,
                    capture: run.capture(),
                });
                break;
            }
        }
    }
    let _ = guard.close();
    report.unclosed_tab = cleanup.into_inner().map(|error| error.tab);
    report.elapsed = clock.now().saturating_sub(start);
    report.fallbacks = run.base.fallbacks;
    report.fallback_steps = run.base.fallback_steps;
    report
}

struct Run2b<'a> {
    base: Run<'a>,
    scratch: PathBuf,
    title: String,
    pending: bool,
    fallback_sent: bool,
}
fn failed(error: impl std::fmt::Display) -> Abort {
    Abort::Failed(error.to_string())
}
fn fail(reason: impl Into<String>) -> Result<Step, Abort> {
    Ok(Step::Fail {
        reason: reason.into(),
    })
}
fn word(text: &str, needle: &str) -> bool {
    text.split(|c: char| !c.is_alphanumeric() && c != '_')
        .any(|part| part == needle)
}
fn after(text: &str, needle: &str) -> String {
    lines_after_last(text, needle).join("\n")
}
fn command_echo(line: &str, command: &str) -> bool {
    normalised(line)
        .first()
        .is_some_and(|line| line == command || line.strip_prefix(USER_LINE_PREFIX) == Some(command))
}
fn after_command(text: &str, command: &str) -> String {
    let lines: Vec<_> = text.lines().collect();
    let start = lines.iter().rposition(|line| command_echo(line, command));
    start.map_or_else(String::new, |index| lines[index + 1..].join("\n"))
}
fn quote(text: &str) -> String {
    format!("'{}'", text.replace('\'', "'\\''"))
}

fn working_status(text: &str) -> bool {
    text.lines()
        .rev()
        .find(|line| line.contains("approvals:"))
        .is_some_and(|line| {
            line.split('│').any(|part| {
                let part = part.trim();
                matches!(part, "working" | "streaming" | "busy")
                    || part
                        .strip_suffix(" working")
                        .and_then(|count| count.parse::<u32>().ok())
                        .is_some_and(|count| count > 0)
            })
        })
}

impl Run2b<'_> {
    fn check(&self) -> Result<(), Abort> {
        self.base.check_stop()?;
        if self.base.clock.now() >= self.base.deadline {
            return Err(Abort::Deadline);
        }
        Ok(())
    }
    fn read(&self, source: ReadSource) -> Result<String, Abort> {
        self.base
            .term
            .read(
                &self.base.pane,
                source,
                (source == ReadSource::RecentUnwrapped).then_some(RECENT_LINES),
            )
            .map_err(failed)
    }
    fn submit(&mut self, text: &str) -> Result<(), Abort> {
        self.check()?;
        self.base.term.run(&self.base.pane, text).map_err(failed)?;
        self.pending = true;
        self.fallback_sent = false;
        Ok(())
    }
    fn key(&mut self, key: &str) -> Result<(), Abort> {
        self.check()?;
        self.pending = false;
        self.base
            .term
            .send_keys(&self.base.pane, &[key])
            .map_err(failed)?;
        if key == "esc" {
            self.base.clock.sleep(AFTER_ESC);
        }
        Ok(())
    }
    fn typed(&mut self, text: &str) -> Result<(), Abort> {
        self.check()?;
        self.pending = false;
        self.base
            .term
            .send_text(&self.base.pane, text)
            .map_err(failed)?;
        self.base.clock.sleep(Duration::from_millis(300));
        Ok(())
    }
    fn wait(
        &mut self,
        seconds: u64,
        source: ReadSource,
        mut accepts: impl FnMut(&str) -> bool,
    ) -> Result<bool, Abort> {
        let started = self.base.clock.now();
        let mut previous = None;
        let mut changed_at = started;
        loop {
            self.check()?;
            let screen = self.read(source)?;
            let now = self.base.clock.now();
            if previous.as_ref() != Some(&screen) {
                changed_at = now;
                previous = Some(screen.clone());
            }
            if accepts(&screen) {
                self.pending = false;
                return Ok(true);
            }
            if self.pending
                && !self.fallback_sent
                && now.saturating_sub(started) <= Duration::from_secs(8)
                && now.saturating_sub(changed_at) >= Duration::from_secs(3)
            {
                let visible = if source == ReadSource::Visible {
                    screen
                } else {
                    self.read(ReadSource::Visible)?
                };
                if !working_status(&visible) && !crate::steps::card_words(&visible) {
                    self.base
                        .term
                        .send_keys(&self.base.pane, &["enter"])
                        .map_err(failed)?;
                    self.fallback_sent = true;
                    self.base.fallbacks += 1;
                    self.base.step_fallbacks += 1;
                }
            }
            if now.saturating_sub(started) >= self.base.limit(seconds) {
                return Ok(false);
            }
            self.base.clock.sleep(POLL_INTERVAL);
        }
    }

    fn settle(&mut self, seconds: u64) -> Result<bool, Abort> {
        let started = self.base.clock.now();
        let mut watch = Watch::new(started);
        loop {
            self.check()?;
            let visible = self
                .base
                .watch_visible(&self.base.pane.clone(), &mut watch)?;
            let now = self.base.clock.now();
            if now.saturating_sub(watch.changed_at) >= Duration::from_secs(3)
                && !working_status(&visible)
            {
                return Ok(true);
            }
            if now.saturating_sub(started) >= self.base.limit(seconds) {
                return Ok(false);
            }
            self.base.clock.sleep(POLL_INTERVAL);
        }
    }
    fn phrase(
        &mut self,
        command: &str,
        text: &str,
        seconds: u64,
        source: ReadSource,
        reason: &str,
    ) -> Result<(), Abort> {
        self.submit(command)?;
        if self.wait(seconds, source, |screen| {
            screen.contains(text)
                || screen
                    .replace("\\<", "<")
                    .replace("\\>", ">")
                    .contains(text)
        })? {
            let visible = self.read(ReadSource::Visible)?;
            let retained = visible
                .lines()
                .any(|line| line.trim_start().starts_with('│') && command_echo(line, command));
            if retained {
                self.key("ctrl+c")?;
            }
            Ok(())
        } else {
            Err(Abort::Failed(
                reason.replace("<n>", &self.base.limit_secs(seconds).to_string()),
            ))
        }
    }
    fn capture(&self) -> Vec<PaneCapture> {
        vec![PaneCapture {
            pane: "first",
            visible: self
                .base
                .term
                .read(&self.base.pane, ReadSource::Visible, None)
                .unwrap_or_default(),
            recent: self
                .base
                .term
                .read(
                    &self.base.pane,
                    ReadSource::RecentUnwrapped,
                    Some(REPORT_LINES),
                )
                .unwrap_or_default(),
        }]
    }
    fn step(&mut self, name: &str) -> Result<Step, Abort> {
        match name {
            "startup" => self.base.startup(&[]),
            "context-empty" => self.context_empty(),
            "permission" => self.permission(),
            "model-picker" => self.model_picker(),
            "model-refusal" => self.model_refusal(),
            "rewind" => self.rewind(),
            "plain-answer" => self.base.plain_answer(),
            "steer-idle" => self.steer_idle(),
            "context" => self.context(),
            "hermes-commands" => self.hermes_commands(),
            "rename-export" => self.rename_export(),
            "queue" => self.queue(),
            "cd" => self.cd(),
            "quit" => {
                // The shared exit check has two token slots; this suite uses A in both.
                if let Some(token) = self.base.tokens.first().copied() {
                    self.base.tokens.push(token);
                }
                self.quit()
            }
            "headless" => self.headless(),
            _ => fail("unknown smoke step"),
        }
    }

    fn context_empty(&mut self) -> Result<Step, Abort> {
        self.submit("/context")?;
        if !self.wait(5, ReadSource::Visible, |text| {
            CTX_EMPTY.iter().all(|needle| text.contains(needle))
        })? {
            return fail(format!(
                "/context did not show '{}' within {} s",
                CTX_EMPTY[0],
                self.base.limit_secs(5)
            ));
        }
        self.phrase(
            "/usage",
            USAGE_EMPTY,
            5,
            ReadSource::Visible,
            "/usage did not show 'No API calls yet in this session.' within <n> s",
        )?;
        Ok(Step::Pass)
    }
    fn permission(&mut self) -> Result<Step, Abort> {
        if status_approvals(&self.read(ReadSource::Visible)?).as_deref() != Some("manual") {
            return fail(format!(
                "the status line did not show '{APPROVALS_MANUAL}' at the start"
            ));
        }
        self.check()?;
        let key = match self.base.term.send_keys(&self.base.pane, &["shift+tab"]) {
            Ok(()) => "shift+tab",
            Err(TermError::Herdr { code, .. }) if code == "invalid_key" => {
                match self.base.term.send_keys(&self.base.pane, &["backtab"]) {
                    Ok(()) => "backtab",
                    Err(TermError::Herdr { code, message }) if code == "invalid_key" => {
                        return Ok(Step::Skip {
                            reason: format!(
                                "Herdr rejected the key names shift+tab and backtab: {code}: {}",
                                message.lines().next().unwrap_or_default()
                            ),
                        });
                    }
                    Err(error) => return Err(failed(error)),
                }
            }
            Err(error) => return Err(failed(error)),
        };
        let checked = (|| {
            if !self.wait(5, ReadSource::Visible, |text| {
                status_approvals(text).as_deref() == Some("off (this session)")
            })? {
                return fail(format!(
                    "Shift+Tab did not change the status line to '{APPROVALS_OFF}' within {} s",
                    self.base.limit_secs(5)
                ));
            }
            if !self.wait(5, ReadSource::RecentUnwrapped, |text| {
                text.contains(NOTICE_OFF)
            })? {
                return fail(format!(
                    "the notice '{NOTICE_OFF}' did not appear within {} s",
                    self.base.limit_secs(5)
                ));
            }
            self.key(key)?;
            if !self.wait(5, ReadSource::Visible, |text| {
                status_approvals(text).as_deref() == Some("manual")
            })? {
                return fail(format!(
                    "the second Shift+Tab did not restore '{APPROVALS_MANUAL}' within {} s",
                    self.base.limit_secs(5)
                ));
            }
            Ok(Step::Pass)
        })();
        if !matches!(checked, Ok(Step::Pass)) {
            let _ = self
                .base
                .term
                .run(&self.base.pane, "/permissions session default");
        }
        checked
    }
    fn model_picker(&mut self) -> Result<Step, Abort> {
        self.phrase(
            "/model",
            PICKER_TITLE,
            5,
            ReadSource::Visible,
            "/model did not open 'Switch model · this session' within <n> s",
        )?;
        let mut missing = Vec::new();
        if !self.wait(5, ReadSource::Visible, |text| {
            let picker = picker_state(text);
            missing.clear();
            if !picker.groups.iter().any(|group| group == PICKER_GROUP) {
                missing.push("Ollama Cloud header");
            }
            if !picker.has_current
                || !normalised(text).iter().any(|line| {
                    line.strip_prefix(PICKER_CURRENT_ROW).is_some_and(|rest| {
                        rest.is_empty() || rest.starts_with(char::is_whitespace)
                    })
                })
            {
                missing.push("current model row");
            }
            if !picker.effort_footer {
                missing.push("effort footer");
            }
            missing.is_empty()
        })? {
            return fail(format!("the model picker lacks: {}", missing.join(", ")));
        }
        self.typed("zzzz")?;
        if !self.wait(5, ReadSource::Visible, |text| {
            text.contains(PICKER_NO_MATCH)
        })? {
            return fail(format!(
                "typing zzzz did not show '{PICKER_NO_MATCH}' within {} s",
                self.base.limit_secs(5)
            ));
        }
        self.key("esc")?;
        let reason = "Esc did not step back: the first Esc must restore the list and the second must close the picker";
        if !self.wait(3, ReadSource::Visible, |text| {
            let picker = picker_state(text);
            picker.title
                && picker.groups.iter().any(|group| group == PICKER_GROUP)
                && picker.no_match.is_none()
                && !text.contains(PICKER_NO_MATCH)
        })? {
            return fail(reason);
        }
        self.key("esc")?;
        if !self.wait(3, ReadSource::Visible, |text| {
            !text.contains("Switch model")
        })? {
            return fail(reason);
        }
        Ok(Step::Pass)
    }
    fn model_refusal(&mut self) -> Result<Step, Abort> {
        self.phrase(
            "/model no-such-model-xyz --provider ollama-cloud",
            MODEL_REFUSAL,
            10,
            ReadSource::RecentUnwrapped,
            "the refusal for an unknown model did not appear within <n> s",
        )?;
        let visible = self.read(ReadSource::Visible)?;
        let status = visible
            .lines()
            .rev()
            .find(|line| line.contains("approvals:"))
            .unwrap_or_default();
        if !status
            .split('│')
            .any(|part| part.split_whitespace().next() == Some(self.base.cfg.model.as_str()))
        {
            return fail(
                "the status line no longer shows minimax-m3.1-flash after the refused switch",
            );
        }
        Ok(Step::Pass)
    }

    fn rewind(&mut self) -> Result<Step, Abort> {
        for prompt in [FIRST_CODE, SECOND_CODE] {
            self.submit(prompt)?;
            if !self.wait(90, ReadSource::RecentUnwrapped, |text| {
                lines_after_last(text, prompt)
                    .iter()
                    .any(|line| line.trim().eq_ignore_ascii_case("ok"))
            })? || !self.settle(90)?
            {
                return fail(format!(
                    "a reply to a code word prompt did not finish within {} s",
                    self.base.limit_secs(90)
                ));
            }
        }
        self.key("esc")?;
        self.key("esc")?;
        if !self.wait(5, ReadSource::Visible, |text| rewind_list(text).open)? {
            return fail(format!(
                "Esc twice did not open '{REWIND_TITLE}' within {} s",
                self.base.limit_secs(5)
            ));
        }
        let list = rewind_list(&self.read(ReadSource::Visible)?);
        if !list.prompts.contains(&2) || !list.prompts.contains(&1) || list.selected != Some(2) {
            return fail("the rewind list does not show both prompts #2 and #1");
        }
        self.key("enter")?;
        self.typed("c")?;
        self.key("enter")?;
        if !self.wait(10, ReadSource::RecentUnwrapped, |text| {
            rewound_line(text, 2).is_some()
        })? {
            return fail(format!(
                "no line starting '{REWOUND}2' appeared within {} s",
                self.base.limit_secs(10)
            ));
        }
        if !self.wait(3, ReadSource::Visible, |text| {
            normalised(text).iter().any(|line| {
                line.strip_prefix(USER_LINE_PREFIX).is_some_and(|draft| {
                    draft.starts_with("Now remember the code word lark instead")
                })
            })
        })? {
            return fail("the composer does not hold the second prompt after the rewind");
        }
        self.key("ctrl+c")?;
        self.submit(CHECK_CODE)?;
        let answered = self.wait(90, ReadSource::RecentUnwrapped, |text| {
            let reply = after(text, CHECK_CODE);
            count_token(&reply, "heron") > 0 || count_token(&reply, "lark") > 0
        })?;
        if answered {
            self.settle(90)?;
        }
        let recent = self.read(ReadSource::RecentUnwrapped)?;
        if recent.contains(REWIND_TITLE) {
            return fail("the rewind overlay left its text in the scrollback");
        }
        let reply = after(&recent, CHECK_CODE);
        if !answered || count_token(&reply, "heron") == 0 || count_token(&reply, "lark") > 0 {
            return fail("the reply after the rewind did not name heron, or named lark");
        }
        Ok(Step::Pass)
    }

    fn steer_idle(&mut self) -> Result<Step, Abort> {
        for attempt in 0..2 {
            self.phrase(STEER, STEER_IDLE, 5, ReadSource::RecentUnwrapped, "/steer on an idle session did not say 'No turn is running, so this was sent as a new prompt' within <n> s")?;
            if self.wait(90, ReadSource::RecentUnwrapped, |text| {
                word(&after(text, STEER_MARKER), "IDLE")
            })? && self.settle(60)?
            {
                return Ok(Step::Pass);
            }
            if attempt == 0 && self.settle(60)? {
                continue;
            }
            break;
        }
        fail(format!(
            "the prompt sent by /steer did not get an 'IDLE' reply within {} s",
            self.base.limit_secs(90)
        ))
    }

    fn context(&mut self) -> Result<Step, Abort> {
        self.submit("/context")?;
        if !self.wait(5, ReadSource::RecentUnwrapped, |text| {
            context_header(&after_command(text, "/context"))
                .is_some_and(|header| header.model == "minimax-m3.1-flash" && header.percent)
        })? {
            return fail(format!(
                "/context did not show a header 'Context · minimax-m3.1-flash · ' ending 'tokens (<n>%)' within {} s",
                self.base.limit_secs(5)
            ));
        }
        let mut missing = Vec::new();
        if !self.wait(5, ReadSource::RecentUnwrapped, |text| {
            let scoped = after_command(text, "/context");
            let rows =
                context_header(&scoped).map(|header| context_rows(&scoped, header.line_index));
            missing.clear();
            if !rows.as_ref().is_some_and(|rows| rows.categories) {
                missing.push("Estimated usage by category");
            }
            if !rows.as_ref().is_some_and(|rows| rows.free_space) {
                missing.push("Free space");
            }
            if !rows.as_ref().is_some_and(|rows| rows.source) {
                missing.push("Source line");
            }
            missing.is_empty()
        })? {
            return fail(format!("/context lacks: {}", missing.join(", ")));
        }
        self.submit("/usage")?;
        if !self.wait(5, ReadSource::RecentUnwrapped, |text| {
            usage_block(&after_command(text, "/usage"))
                .is_some_and(|block| block.tokens_row && block.cost_row)
        })? {
            return fail(format!(
                "/usage did not show '{USAGE_TITLE}' with a 'Tokens' row and a 'Cost' row within {} s",
                self.base.limit_secs(5)
            ));
        }
        self.phrase("/compact --aggressive", COMPACT_REFUSAL, 5, ReadSource::RecentUnwrapped, "/compact --aggressive did not say 'Hermes does not support --aggressive. Nothing was changed.' within <n> s")?;
        self.phrase(
            "/context now",
            CTX_ARG,
            5,
            ReadSource::RecentUnwrapped,
            "/context now did not say '/context does not take an argument.' within <n> s",
        )?;
        let recent = self.read(ReadSource::RecentUnwrapped)?;
        // Bound the check to the measured context command, before its argument refusal.
        let measured = recent
            .lines()
            .position(|line| command_echo(line, "/context"));
        if measured.is_some_and(|start| {
            recent
                .lines()
                .skip(start)
                .any(|line| line.contains(COMPRESSED))
        }) {
            return fail(
                "a '✓ Compressed' line appeared: a refused command changed the conversation",
            );
        }
        Ok(Step::Pass)
    }

    fn hermes_commands(&mut self) -> Result<Step, Abort> {
        self.phrase("/fast", FAST_REFUSAL, 10, ReadSource::RecentUnwrapped, "/fast did not show its refusal '⚡ /fast is only available for OpenAI models that support Priority Processing.' within <n> s")?;
        self.phrase("/loop 45m", LOOP_USAGE, 10, ReadSource::RecentUnwrapped, "/loop 45m did not show '/loop: missing prompt (usage: /loop [interval] <prompt>)' within <n> s")?;
        if self.read(ReadSource::Visible)?.contains(LOOP_LINE) {
            return fail("a loop line appeared after /loop 45m with no prompt");
        }
        Ok(Step::Pass)
    }

    fn rename_export(&mut self) -> Result<Step, Abort> {
        self.phrase(
            &format!("/rename {}", self.title),
            &RENAMED.replace("{}", &self.title),
            10,
            ReadSource::RecentUnwrapped,
            "/rename did not answer 'Renamed to \"<title>\".' within <n> s",
        )?;
        self.phrase(
            "/rename",
            &RENAME_SHOW.replace("{}", &self.title),
            10,
            ReadSource::RecentUnwrapped,
            "/rename with no title did not say 'This session is called \"<title>\".' within <n> s",
        )?;
        self.submit("/export")?;
        let title_prefix = EXPORTED.replacen("{}", &self.title, 1);
        let prefix = title_prefix.split("{}").next().unwrap_or_default();
        let mut filename = None;
        if !self.wait(15, ReadSource::RecentUnwrapped, |text| {
            filename = normalised(text).iter().rev().find_map(|line| {
                let rest = line.strip_prefix(prefix)?;
                if !rest.contains(EXPORTED_REDACTED) {
                    return None;
                }
                rest.split_once(" (").map(|(name, _)| name.to_owned())
            });
            filename.is_some()
        })? {
            return fail(format!(
                "/export did not print 'Exported \"<title>\" to ./<name>' within {} s",
                self.base.limit_secs(15)
            ));
        }
        let valid = filename.as_deref().is_some_and(|name| {
            if Path::new(name).file_name() != Some(std::ffi::OsStr::new(name)) {
                return false;
            }
            let path = self.scratch.join("a").join(name);
            let Some(token) = self.base.tokens.first() else {
                return false;
            };
            let expected = format!("{} and {}", token.word, token.number);
            fs::symlink_metadata(&path)
                .is_ok_and(|meta| meta.is_file() && meta.permissions().mode() & 0o777 == 0o600)
                && fs::read_to_string(path)
                    .is_ok_and(|text| text.lines().any(|line| line.contains(&expected)))
        });
        if !valid {
            return fail(
                "the exported file is missing, is not mode 0600, or lacks the plain-answer prompt",
            );
        }
        self.phrase("/export ./notes.txt", EXPORT_BAD, 5, ReadSource::RecentUnwrapped, "/export ./notes.txt did not say 'Use a file name ending in .md, .json or .html.' within <n> s")?;
        if self.scratch.join("a/notes.txt").exists() {
            return fail("/export ./notes.txt created a file");
        }
        Ok(Step::Pass)
    }

    fn queue(&mut self) -> Result<Step, Abort> {
        for attempt in 0..2 {
            let before = prompt_echoes(&self.read(ReadSource::Visible)?, LONG);
            self.submit(LONG)?;
            let started = self.base.clock.now();
            let mut acknowledged = false;
            loop {
                self.check()?;
                let visible = self.read(ReadSource::Visible)?;
                acknowledged |= prompt_echoes(&visible, LONG) > before;
                if self.base.clock.now().saturating_sub(started) >= Duration::from_secs(3) {
                    if !acknowledged
                        && !working_status(&visible)
                        && !crate::steps::card_words(&visible)
                    {
                        self.base
                            .term
                            .send_keys(&self.base.pane, &["enter"])
                            .map_err(failed)?;
                        self.base.fallbacks += 1;
                        self.base.step_fallbacks += 1;
                        self.base.clock.sleep(Duration::from_millis(300));
                    }
                    break;
                }
                self.base.clock.sleep(POLL_INTERVAL);
            }
            self.pending = false;
            self.submit(OMEGA)?;
            let mut preview_count = 0;
            let indicator = self.wait(5, ReadSource::Visible, |text| {
                let scoped = after(text, LONG_MARKER);
                preview_count = scoped
                    .lines()
                    .filter(|line| line.contains(QUEUE_PREVIEW))
                    .count();
                queue_indicator(&scoped) == Some(1) && preview_count > 0
            })?;
            if !indicator {
                if attempt == 0 && self.settle(60)? {
                    continue;
                }
                return fail(format!(
                    "'queued (1)' and the preview 'Reply with the word omega' did not appear within {} s",
                    self.base.limit_secs(5)
                ));
            }
            if preview_count > 1 {
                return fail("the queued prompt shows more than once on screen before it ran");
            }
            if !self.wait(120, ReadSource::RecentUnwrapped, |text| {
                word(&after(text, LONG_MARKER), "FIRST")
            })? {
                return fail(format!(
                    "the first reply 'FIRST' did not appear within {} s",
                    self.base.limit_secs(120)
                ));
            }
            if !self.wait(120, ReadSource::RecentUnwrapped, |text| {
                word(&after(text, LONG_MARKER), "OMEGA")
            })? || !self.settle(60)?
            {
                return fail(format!(
                    "the queued prompt did not run: 'OMEGA' did not appear within {} s",
                    self.base.limit_secs(120)
                ));
            }
            let recent = self.read(ReadSource::RecentUnwrapped)?;
            let scoped = lines_after_last(&recent, LONG_MARKER);
            let first = scoped.iter().position(|line| word(line, "FIRST"));
            let prompt = scoped.iter().position(|line| {
                line.trim_start().starts_with(USER_LINE_PREFIX) && line.contains(QUEUE_PREVIEW)
            });
            let omega = scoped.iter().position(|line| word(line, "OMEGA"));
            if !matches!((first, prompt, omega), (Some(a), Some(b), Some(c)) if a < b && b < c) {
                return fail(
                    "the order is wrong: the omega prompt and OMEGA must come after FIRST",
                );
            }
            if queue_indicator(&after(&self.read(ReadSource::Visible)?, LONG_MARKER)).is_some() {
                return fail("an indicator 'queued (' is still shown after the queue drained");
            }
            return Ok(Step::Pass);
        }
        fail("the queue could not be checked")
    }

    fn cd(&mut self) -> Result<Step, Abort> {
        self.submit("/cd ../b")?;
        if !self.wait(15, ReadSource::RecentUnwrapped, |text| {
            normalised(text)
                .iter()
                .any(|line| line.starts_with(CD_MOVING))
                && normalised(text)
                    .iter()
                    .any(|line| line.starts_with(CD_DONE) && line.contains("/b (main)"))
        })? {
            return fail(format!(
                "/cd ../b did not print '{CD_DONE}' with a path ending '/b (main)' within {} s",
                self.base.limit_secs(15)
            ));
        }
        if !self.wait(5, ReadSource::Visible, |text| {
            folder_segment(text).is_some_and(|folder| folder.ends_with("b (main)"))
        })? {
            return fail("the status line folder segment does not end with 'b (main)'");
        }
        self.submit("/cd")?;
        if !self.wait(5, ReadSource::RecentUnwrapped, |text| {
            normalised(text).iter().any(|line| {
                let Some(folder) = line.strip_prefix(CD_SHOW) else {
                    return false;
                };
                let folder = folder
                    .split(". Use /cd")
                    .next()
                    .unwrap_or_default()
                    .trim()
                    .trim_end_matches('.');
                folder
                    .strip_suffix(" (main)")
                    .unwrap_or(folder)
                    .ends_with("/b")
            })
        })? {
            return fail(format!(
                "/cd with no argument did not print '{CD_SHOW}' within {} s",
                self.base.limit_secs(5)
            ));
        }
        self.phrase(
            "/cd ./missing-folder-smoke",
            CD_MISSING,
            5,
            ReadSource::RecentUnwrapped,
            "/cd ./missing-folder-smoke did not print 'No folder at' within <n> s",
        )?;
        Ok(Step::Pass)
    }

    fn quit(&mut self) -> Result<Step, Abort> {
        let started = self.base.clock.now();
        let result = self.base.quit()?;
        if !matches!(&result, Step::Fail { reason } if reason == "the shell did not return to the foreground after /quit")
        {
            return Ok(result);
        }
        // A shell may briefly run prompt hooks after the zero exit marker is printed.
        loop {
            self.check()?;
            let foreground = self.base.term.foreground(&self.base.pane).map_err(failed)?;
            let binary = self.base.cfg.talaria.to_string_lossy();
            if foreground.len() == 1 && !foreground[0].contains(binary.as_ref()) {
                let recent = self.read(ReadSource::RecentUnwrapped)?;
                let count = self.base.tokens.first().map_or(0, |token| {
                    count_token(&recent, &format!("{}-{}", token.word, token.number))
                });
                if count == 0 {
                    return fail("committed output vanished on exit (token A missing)");
                }
                self.base.counts = (count, count);
                return Ok(Step::Pass);
            }
            if self.base.clock.now().saturating_sub(started) >= self.base.limit(15) {
                return Ok(result);
            }
            self.base.clock.sleep(POLL_INTERVAL);
        }
    }

    fn headless_command(&mut self, command: &str, seconds: u64) -> Result<Option<i32>, Abort> {
        let before = exit_markers(&self.read(ReadSource::RecentUnwrapped)?).len();
        self.submit(command)?;
        self.pending = false;
        let mut exit = None;
        if !self.wait(seconds, ReadSource::RecentUnwrapped, |text| {
            let markers = exit_markers(text);
            if markers.len() > before {
                exit = markers.last().copied();
            }
            exit.is_some()
        })? {
            return Err(Abort::Failed(format!(
                "a headless command did not finish within {} s",
                self.base.limit_secs(seconds)
            )));
        }
        Ok(exit)
    }

    fn headless(&mut self) -> Result<Step, Abort> {
        let cfg = self.base.cfg;
        let scratch = self.scratch.clone();
        let file = |name| quote(&scratch.join(name).to_string_lossy());
        let binary = quote(&cfg.talaria.to_string_lossy());
        let command = format!(
            "{binary} -p --no-ensure --profile {} --model {} --provider {} \"What is 1234 plus 4321? Reply with only the number.\" > {} 2> {}; echo SMOKE_EXIT:$?",
            quote(&cfg.profile),
            quote(&cfg.model),
            quote(&cfg.provider),
            file("print.out"),
            file("print.err")
        );
        let exit = self.headless_command(&command, 90)?;
        let out = fs::read_to_string(self.scratch.join("print.out"));
        let err = fs::read_to_string(self.scratch.join("print.err"));
        let checked = match (&out, &err) {
            (Ok(out), Ok(err)) => Some(print_check(out, err, exit)),
            _ => None,
        };
        if !checked.is_some_and(|check| check.text_ok && check.err_empty && check.exit_ok) {
            return fail(format!(
                "the plain headless run did not print 5555, exit 0 and leave standard error empty (exit {})",
                exit.map_or_else(|| "unknown".to_owned(), |code| code.to_string())
            ));
        }
        let command = format!(
            "HERMES_HOME={} {binary} -p --no-ensure --output-format json \"hello\" > {} 2> {}; echo SMOKE_EXIT:$?",
            file("empty-home"),
            file("nogw.out"),
            file("nogw.err")
        );
        let exit = self.headless_command(&command, 30)?;
        let out = fs::read_to_string(self.scratch.join("nogw.out")).unwrap_or_default();
        if exit != Some(4) || !print_check(&out, "", exit).not_started {
            return fail(
                "the headless run without a gateway did not print one JSON line with \"status\":\"not_started\" and \"exit_code\":4, and exit 4",
            );
        }
        let err = fs::read_to_string(self.scratch.join("nogw.err")).unwrap_or_default();
        if !err.starts_with(NO_GATEWAY) {
            return fail(format!(
                "the headless run without a gateway did not print '{NO_GATEWAY}' on standard error"
            ));
        }
        Ok(Step::Pass)
    }
}

fn exit_markers(text: &str) -> Vec<i32> {
    text.lines()
        .filter_map(|line| line.trim().strip_prefix("SMOKE_EXIT:")?.parse().ok())
        .collect()
}

fn prompt_echoes(text: &str, prompt: &str) -> usize {
    let compact = |text: &str| {
        text.chars()
            .filter(|character| !character.is_whitespace())
            .collect::<String>()
    };
    compact(&normalised(text).join(" "))
        .matches(&compact(prompt))
        .count()
}
