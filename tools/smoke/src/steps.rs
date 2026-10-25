//! The six steps, the run and the tab guard.

use std::cell::RefCell;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use rand::rngs::StdRng;
use rand::{RngExt, SeedableRng};

use super::clock::Clock;
use super::config::{SmokeConfig, launch_line_env};
use super::report::{PaneCapture, SmokeReport, StepOutcome};
use super::screen::{
    Card, POLL_INTERVAL, Token, approval_card, count_token, counting_values, exit_code,
    highest_number, without_resume_header,
};
use super::terminal::{PaneId, ReadSource, TabId, Terminal};

/// The steps of a run, in order.
const STEPS: [&str; 6] = [
    "startup",
    "plain-answer",
    "approval",
    "interrupt",
    "quit",
    "resume",
];

/// The command the approval step asks for.
const APPROVAL_COMMAND: &str = "rm -rf .pytest_cache";
/// The first approval prompt.
const APPROVAL_PROMPT: &str =
    "Run this exact shell command and then tell me its exit code: rm -rf .pytest_cache";
/// The second approval prompt.
const APPROVAL_RETRY_PROMPT: &str =
    "Use your shell tool now and run exactly this command: rm -rf .pytest_cache";
/// The prompt that makes the model count.
pub(crate) const COUNT_PROMPT: &str =
    "Count from 1 to 2000, one number per line, and do not stop early.";
/// The text that marks the echo of the counting prompt.
pub(crate) const COUNT_MARKER: &str = "Count from 1 to 2000";
/// The only slash command.
const QUIT_COMMAND: &str = "/quit";

/// The reason of a skipped approval step.
const SKIP_REASON: &str = "no approval card appeared and the command ran; the profile fixture must set approvals.mode: manual (the default is smart, in which Hermes may approve by itself; a hosted-room policy can also override the mode)";

/// The per-step limits, in seconds, before the scale is applied.
pub(crate) const SHELL_LIMIT: u64 = 45;
const STARTUP_LIMIT: u64 = 30;
const REPLY_LIMIT: u64 = 60;
const APPROVAL_LIMIT: u64 = 90;
const APPROVAL_AFTER_ANSWER: u64 = 60;
const COUNT_LIMIT: u64 = 60;
pub(crate) const SETTLE_LIMIT: u64 = 20;
const QUIT_LIMIT: u64 = 15;
const RESUME_LIMIT: u64 = 30;

/// The quiet periods, in seconds; they are never scaled.
const QUIET_SHELL: u64 = 1;
const QUIET_STARTUP: u64 = 1;
pub(crate) const QUIET_REPLY: u64 = 3;
const QUIET_RETRY: u64 = 5;
const QUIET_RESUME: u64 = 2;
/// The wait after Esc, so that no stray character follows the key.
pub(crate) const AFTER_ESC: Duration = Duration::from_millis(400);
/// The wait after the digit of a card, before the Enter is considered.
const AFTER_DIGIT: Duration = Duration::from_millis(1500);
/// The wait after the Enter of a card, before the card is given up on.
const AFTER_ENTER: Duration = Duration::from_millis(3000);
/// The window in which a submit fallback may fire.
pub(crate) const FALLBACK_WINDOW: Duration = Duration::from_secs(8);
/// How many lines of the recent text a failure report holds.
pub(crate) const REPORT_LINES: u32 = 200;
/// How many lines the searches read.
pub(crate) const RECENT_LINES: u32 = 3000;
/// Why a step stopped early.
pub(crate) enum Abort {
    /// A signal arrived.
    Interrupted,
    /// The run deadline passed.
    Deadline,
    /// The step could not go on.
    Failed(String),
}

/// Turns an abort into the outcome of the step.
pub(crate) fn finish(abort: Abort) -> Result<Step, Abort> {
    match abort {
        Abort::Failed(reason) => Ok(Step::Fail { reason }),
        other => Err(other),
    }
}

/// What a step ended with, before the capture is added.
pub(crate) enum Step {
    Pass,
    Skip { reason: String },
    Fail { reason: String },
}

/// The screen of a pane, and when it last changed.
pub(crate) struct Watch {
    previous: Option<String>,
    pub(crate) changed_at: Duration,
}

impl Watch {
    pub(crate) fn new(now: Duration) -> Self {
        Self {
            previous: None,
            changed_at: now,
        }
    }
}

/// Owns the tab id; `Drop` is only a fallback.
pub struct TabGuard<'a> {
    term: &'a dyn Terminal,
    tab: TabId,
    failure: &'a RefCell<Option<CleanupError>>,
    closed: bool,
}

impl<'a> TabGuard<'a> {
    /// A guard for `tab`; a failure of the fallback is recorded in `failure`.
    pub fn new(
        term: &'a dyn Terminal,
        tab: TabId,
        failure: &'a RefCell<Option<CleanupError>>,
    ) -> Self {
        Self {
            term,
            tab,
            failure,
            closed: false,
        }
    }

    /// Closes the tab and checks that it is gone.
    pub fn close(mut self) -> Result<(), CleanupError> {
        self.closed = true;
        let _ = self.term.close_tab(&self.tab);
        let gone = match self.term.tab_exists(&self.tab) {
            Ok(exists) => !exists,
            Err(_) => false,
        };
        if gone {
            return Ok(());
        }
        let error = CleanupError {
            tab: self.tab.clone(),
            detail: "the tab is still in the list".to_owned(),
        };
        *self.failure.borrow_mut() = Some(error.clone());
        Err(error)
    }
}

impl Drop for TabGuard<'_> {
    fn drop(&mut self) {
        if self.closed {
            return;
        }
        let _ = self.term.close_tab(&self.tab);
        if self.failure.borrow().is_none() {
            *self.failure.borrow_mut() = Some(CleanupError {
                tab: self.tab.clone(),
                detail: "the run ended before the tab was closed".to_owned(),
            });
        }
    }
}

/// The tab could not be closed.
#[derive(Clone, Debug, thiserror::Error)]
#[error("tab {} was not closed", tab.0)]
pub struct CleanupError {
    pub tab: TabId,
    pub detail: String,
}

/// Runs the six steps and closes the tab.
pub fn run_smoke(
    term: &dyn Terminal,
    clock: &dyn Clock,
    cfg: &SmokeConfig,
    stop: &AtomicBool,
) -> SmokeReport {
    let start = clock.now();
    let deadline = start + cfg.deadline;
    let mut run = Run {
        term,
        clock,
        cfg,
        stop,
        deadline,
        rng: match cfg.seed {
            Some(seed) => StdRng::seed_from_u64(seed),
            None => StdRng::from_rng(&mut rand::rng()),
        },
        tokens: Vec::new(),
        scratch: PathBuf::new(),
        answered_at: None,
        fallbacks: 0,
        step_fallbacks: 0,
        fallback_steps: Vec::new(),
        interrupted: false,
        pane: PaneId(String::new()),
        second: None,
        counts: (0, 0),
    };

    let run_id = run.run_id();
    let label = format!("talaria-smoke-{run_id}");
    let scratch = cfg.temp_root.join(&label);
    run.scratch = scratch.clone();
    let _ = fs::remove_dir_all(&scratch);
    make_scratch(&scratch);

    let mut outcomes: Vec<StepOutcome> = Vec::new();
    let mut unclosed: Option<TabId> = None;
    let failure_cell: RefCell<Option<CleanupError>> = RefCell::new(None);

    let tab = match term.create_tab(&label, &scratch) {
        Ok(tab) => tab,
        Err(error) => {
            let _ = fs::remove_dir_all(&scratch);
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
                scratch: scratch.clone(),
                fallback_steps: Vec::new(),
            };
        }
    };
    run.pane = tab.pane.clone();

    let mut step_index = 0;
    while step_index < STEPS.len() {
        let name = STEPS[step_index];
        let started = clock.now();
        run.step_fallbacks = 0;
        let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| run.step(name)))
            .unwrap_or_else(|panic| {
                let message = panic_message(&panic);
                Ok(Step::Fail {
                    reason: format!("internal error: {message}"),
                })
            });
        if run.step_fallbacks > 0 {
            run.fallback_steps.push(name);
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
                let capture = run.capture();
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
                        run.interrupted = true;
                        "interrupted by a signal".to_owned()
                    }
                    Abort::Deadline => format!("deadline exceeded in step {name}"),
                    Abort::Failed(reason) => reason,
                };
                let capture = run.capture();
                outcomes.push(StepOutcome::Fail {
                    step: name,
                    reason,
                    capture,
                });
                break;
            }
        }
    }

    let guard = TabGuard::new(term, tab.tab.clone(), &failure_cell);
    if guard.close().is_err() {
        unclosed = Some(tab.tab.clone());
    }
    if let Some(error) = failure_cell.into_inner() {
        unclosed = Some(error.tab);
    }
    let _ = fs::remove_dir_all(&scratch);

    SmokeReport {
        outcomes,
        version: term.version(),
        elapsed: clock.now().saturating_sub(start),
        fallbacks: run.fallbacks,
        interrupted: run.interrupted,
        unclosed_tab: unclosed,
        unclosed_session: None,
        tab: Some(tab.tab),
        scratch,
        fallback_steps: run.fallback_steps,
    }
}

/// The state of one run.
pub(crate) struct Run<'a> {
    pub(crate) term: &'a dyn Terminal,
    pub(crate) clock: &'a dyn Clock,
    pub(crate) cfg: &'a SmokeConfig,
    pub(crate) stop: &'a AtomicBool,
    pub(crate) deadline: Duration,
    pub(crate) rng: StdRng,
    /// The tokens drawn so far: A, B and C in the order they were used.
    pub(crate) tokens: Vec<Token>,
    /// The scratch project of this run.
    pub(crate) scratch: PathBuf,
    /// When the approval card was answered.
    pub(crate) answered_at: Option<Duration>,
    pub(crate) fallbacks: u32,
    pub(crate) step_fallbacks: u32,
    pub(crate) fallback_steps: Vec<&'static str>,
    pub(crate) interrupted: bool,
    pub(crate) pane: PaneId,
    pub(crate) second: Option<PaneId>,
    /// How often A and B appeared in the first pane.
    pub(crate) counts: (usize, usize),
}

impl Run<'_> {
    /// The eight characters of the run id.
    fn run_id(&mut self) -> String {
        const ALPHABET: &[u8] = b"abcdefghijklmnopqrstuvwxyz0123456789";
        let mut id = String::with_capacity(8);
        for _ in 0..8 {
            let index = self.rng.random_range(0..ALPHABET.len());
            id.push(ALPHABET[index] as char);
        }
        id
    }

    /// A fresh token, never one that was used before.
    pub(crate) fn token(&mut self) -> Token {
        let token = Token::new(&mut self.rng, &self.tokens);
        self.tokens.push(token);
        token
    }

    /// The scratch project of this run.
    fn scratch(&self) -> PathBuf {
        self.scratch.clone()
    }

    /// A per-step limit, scaled.
    pub(crate) fn limit(&self, base: u64) -> Duration {
        let seconds = (base as f64 * self.cfg.step_scale as f64).clamp(0.001, 86_400.0);
        Duration::from_secs_f64(seconds)
    }

    /// The per-step limit in whole seconds, rounded to the nearest.
    pub(crate) fn limit_secs(&self, base: u64) -> u64 {
        let seconds = (base as f64 * self.cfg.step_scale as f64).round();
        (seconds.max(1.0) as u64).min(86_400)
    }

    /// The binary under test, as the text of a command line.
    fn binary(&self) -> String {
        self.cfg.talaria.to_string_lossy().into_owned()
    }

    /// The signal flag.
    pub(crate) fn check_stop(&self) -> Result<(), Abort> {
        if self.stop.load(Ordering::SeqCst) {
            return Err(Abort::Interrupted);
        }
        Ok(())
    }

    /// Reads the visible text and notes when it last changed.
    pub(crate) fn watch_visible(
        &mut self,
        pane: &PaneId,
        watch: &mut Watch,
    ) -> Result<String, Abort> {
        let visible = self
            .term
            .read(pane, ReadSource::Visible, None)
            .map_err(|error| Abort::Failed(error.to_string()))?;
        let now = self.clock.now();
        if watch.previous.as_ref() != Some(&visible) {
            watch.previous = Some(visible.clone());
            watch.changed_at = now;
        }
        Ok(visible)
    }

    /// The screen of every pane of the tab, for the failure report.
    fn capture(&self) -> Vec<PaneCapture> {
        let mut captures = Vec::new();
        let mut panes: Vec<(&'static str, &PaneId)> = vec![("first", &self.pane)];
        if let Some(second) = &self.second {
            panes.push(("second", second));
        }
        for (name, pane) in panes {
            let visible = self
                .term
                .read(pane, ReadSource::Visible, None)
                .unwrap_or_default();
            let recent = self
                .term
                .read(pane, ReadSource::RecentUnwrapped, Some(REPORT_LINES))
                .unwrap_or_default();
            captures.push(PaneCapture {
                pane: name,
                visible,
                recent,
            });
        }
        captures
    }

    /// Runs one step by its name.
    fn step(&mut self, name: &'static str) -> Result<Step, Abort> {
        match name {
            "startup" => self.startup(&[]),
            "plain-answer" => self.plain_answer(),
            "approval" => self.approval(),
            "interrupt" => self.interrupt(),
            "quit" => self.quit(),
            "resume" => self.resume(&[], &mut |_, _| Ok(Step::Pass)),
            other => Ok(Step::Fail {
                reason: format!("there is no step named {other}"),
            }),
        }
    }
}

impl Run<'_> {
    /// The first step: the shell is ready and the binary shows a stable screen.
    pub(crate) fn startup(&mut self, env: &[(&str, &str)]) -> Result<Step, Abort> {
        if let Err(abort) = self.wait_for_shell(&self.pane.clone(), SHELL_LIMIT) {
            return finish(abort);
        }
        let line = launch_line_env(
            env,
            &self.cfg.talaria,
            &[
                "--no-ensure",
                "--profile",
                &self.cfg.profile,
                "--model",
                &self.cfg.model,
                "--provider",
                &self.cfg.provider,
            ],
        );
        self.term
            .run(&self.pane, &line)
            .map_err(|error| Abort::Failed(error.to_string()))?;
        match self.wait_for_screen(
            &self.pane.clone(),
            STARTUP_LIMIT,
            QUIET_STARTUP,
            "talaria",
            "startup",
        ) {
            Ok(()) => Ok(Step::Pass),
            Err(abort) => finish(abort),
        }
    }

    /// The second step: the model joins the two words and the screen settles.
    pub(crate) fn plain_answer(&mut self) -> Result<Step, Abort> {
        let token = self.token();
        self.term
            .run(&self.pane, &token.prompt())
            .map_err(|error| Abort::Failed(error.to_string()))?;
        self.wait_for_reply(
            &self.pane.clone(),
            &token,
            REPLY_LIMIT,
            "the reply token did not appear and settle within",
        )
    }

    /// The third step: the approval card is answered and the command runs.
    fn approval(&mut self) -> Result<Step, Abort> {
        self.ensure_cache();
        for attempt in 1..=2 {
            let prompt = if attempt == 1 {
                APPROVAL_PROMPT
            } else {
                APPROVAL_RETRY_PROMPT
            };
            self.term
                .run(&self.pane, prompt)
                .map_err(|error| Abort::Failed(error.to_string()))?;
            match self.watch_approval()? {
                Approval::Answered => return self.wait_for_deletion(),
                Approval::Skipped => {
                    return Ok(Step::Skip {
                        reason: SKIP_REASON.to_owned(),
                    });
                }
                Approval::Failed(reason) => return Ok(Step::Fail { reason }),
                Approval::Retry => {}
            }
        }
        let visible = self
            .term
            .read(&self.pane, ReadSource::Visible, None)
            .map_err(|error| Abort::Failed(error.to_string()))?;
        if let Some(line) = withdrawn_line(&visible) {
            return Ok(Step::Fail {
                reason: format!("the approval was withdrawn or blocked: {line}"),
            });
        }
        Ok(Step::Fail {
            reason: "the model did not run the command in two attempts".to_owned(),
        })
    }

    /// The fourth step: the count is interrupted and the session still answers.
    fn interrupt(&mut self) -> Result<Step, Abort> {
        self.term
            .run(&self.pane, COUNT_PROMPT)
            .map_err(|error| Abort::Failed(error.to_string()))?;
        let limit = self.limit(COUNT_LIMIT);
        let started = self.clock.now();
        let mut watch = Watch::new(started);
        let mut visible;
        loop {
            self.check_stop()?;
            let now = self.clock.now();
            if now >= self.deadline {
                return Err(Abort::Deadline);
            }
            visible = self.watch_visible(&self.pane.clone(), &mut watch)?;
            if counting_values(&visible, COUNT_MARKER).len() >= 15 {
                break;
            }
            if now - started >= limit {
                return Ok(Step::Fail {
                    reason: format!(
                        "fewer than 15 counting lines appeared within {} s",
                        self.limit_secs(COUNT_LIMIT)
                    ),
                });
            }
            self.clock.sleep(POLL_INTERVAL);
        }

        // The count must be running when Esc is sent.
        let before = highest_number(&visible, COUNT_MARKER);
        self.clock.sleep(Duration::from_secs(1));
        let visible = self
            .term
            .read(&self.pane, ReadSource::Visible, None)
            .map_err(|error| Abort::Failed(error.to_string()))?;
        let after = highest_number(&visible, COUNT_MARKER);
        let (Some(before), Some(after)) = (before, after) else {
            return Ok(Step::Fail {
                reason: "the count was not running, or was nearly finished, when Esc was sent"
                    .to_owned(),
            });
        };
        if after <= before || after > 1500 {
            return Ok(Step::Fail {
                reason: "the count was not running, or was nearly finished, when Esc was sent"
                    .to_owned(),
            });
        }

        // Esc, and nothing else for a moment.
        self.term
            .send_keys(&self.pane, &["esc"])
            .map_err(|error| Abort::Failed(error.to_string()))?;
        self.clock.sleep(AFTER_ESC);

        let limit = self.limit(SETTLE_LIMIT);
        let started = self.clock.now();
        let mut watch = Watch::new(started);
        loop {
            self.check_stop()?;
            let now = self.clock.now();
            if now >= self.deadline {
                return Err(Abort::Deadline);
            }
            let visible = self.watch_visible(&self.pane.clone(), &mut watch)?;
            if now.saturating_sub(watch.changed_at) >= Duration::from_secs(QUIET_REPLY) {
                let final_number = highest_number(&visible, COUNT_MARKER);
                let Some(final_number) = final_number else {
                    return Ok(Step::Fail {
                        reason: "no counting output remained after Esc".to_owned(),
                    });
                };
                if final_number < after {
                    return Ok(Step::Fail {
                        reason: "counting output vanished after Esc".to_owned(),
                    });
                }
                if final_number >= 1900 {
                    return Ok(Step::Fail {
                        reason: "the count finished before the interrupt; use a slower prompt"
                            .to_owned(),
                    });
                }
                break;
            }
            if now - started >= limit {
                return Ok(Step::Fail {
                    reason: "output kept growing after Esc".to_owned(),
                });
            }
            self.clock.sleep(POLL_INTERVAL);
        }

        // The session still answers after the interrupt.
        let token = self.token();
        self.term
            .run(&self.pane, &token.prompt())
            .map_err(|error| Abort::Failed(error.to_string()))?;
        self.wait_for_reply(
            &self.pane.clone(),
            &token,
            REPLY_LIMIT,
            "the reply to the follow-up prompt did not appear and settle within",
        )
    }

    /// The fifth step: the session is closed and the transcript is still there.
    pub(crate) fn quit(&mut self) -> Result<Step, Abort> {
        self.term
            .run(&self.pane, QUIT_COMMAND)
            .map_err(|error| Abort::Failed(error.to_string()))?;
        let limit = self.limit(QUIT_LIMIT);
        let started = self.clock.now();
        let mut watch = Watch::new(started);
        let mut fallback = false;
        loop {
            self.check_stop()?;
            let now = self.clock.now();
            if now >= self.deadline {
                return Err(Abort::Deadline);
            }
            let visible = self.watch_visible(&self.pane.clone(), &mut watch)?;
            if let Some(code) = exit_code(&visible) {
                if code != 0 {
                    return Ok(Step::Fail {
                        reason: format!("/quit ended talaria with exit code {code}, expected 0"),
                    });
                }
                let foreground = self
                    .term
                    .foreground(&self.pane)
                    .map_err(|error| Abort::Failed(error.to_string()))?;
                let binary = self.binary();
                let shell_alone = foreground.len() == 1 && !foreground[0].contains(&binary);
                if !shell_alone {
                    return Ok(Step::Fail {
                        reason: "the shell did not return to the foreground after /quit".to_owned(),
                    });
                }
                let recent = self
                    .term
                    .read(&self.pane, ReadSource::RecentUnwrapped, Some(RECENT_LINES))
                    .map_err(|error| Abort::Failed(error.to_string()))?;
                let first = self.tokens.first().map(Token::join).unwrap_or_default();
                let second = self.tokens.get(1).map(Token::join).unwrap_or_default();
                let count_first = count_token(&recent, &first);
                let count_second = count_token(&recent, &second);
                if count_first == 0 {
                    return Ok(Step::Fail {
                        reason: "committed output vanished on exit (token A missing)".to_owned(),
                    });
                }
                if count_second == 0 {
                    return Ok(Step::Fail {
                        reason: "committed output vanished on exit (token B missing)".to_owned(),
                    });
                }
                self.counts = (count_first, count_second);
                return Ok(Step::Pass);
            }
            if !fallback
                && now - started <= limit
                && now.saturating_sub(watch.changed_at) >= Duration::from_secs(QUIET_REPLY)
            {
                self.term
                    .send_keys(&self.pane, &["enter"])
                    .map_err(|error| Abort::Failed(error.to_string()))?;
                fallback = true;
                self.fallbacks += 1;
                self.step_fallbacks += 1;
            }
            if now - started >= limit {
                return Ok(Step::Fail {
                    reason: format!(
                        "/quit did not end talaria within {} s",
                        self.limit_secs(QUIT_LIMIT)
                    ),
                });
            }
            self.clock.sleep(POLL_INTERVAL);
        }
    }

    /// The sixth step: the session is resumed in a second pane. `after_restore` runs in the
    /// resumed pane once the history check passed, before the third prompt.
    pub(crate) fn resume(
        &mut self,
        env: &[(&str, &str)],
        after_restore: &mut dyn FnMut(&mut Self, &PaneId) -> Result<Step, Abort>,
    ) -> Result<Step, Abort> {
        let second = self
            .term
            .split_pane(&self.pane, &self.scratch())
            .map_err(|error| Abort::Failed(error.to_string()))?;
        self.second = Some(second.clone());
        if let Err(abort) = self.wait_for_shell(&second, SHELL_LIMIT) {
            return finish(abort);
        }
        let line = launch_line_env(
            env,
            &self.cfg.talaria,
            &["--no-ensure", "-c", "--profile", &self.cfg.profile],
        );
        self.term
            .run(&second, &line)
            .map_err(|error| Abort::Failed(error.to_string()))?;
        if let Err(abort) = self.wait_for_screen(
            &second,
            RESUME_LIMIT,
            QUIET_RESUME,
            "resumed talaria",
            "resumed talaria",
        ) {
            return finish(abort);
        }

        // The history came back once, and no more often than it was shown. The header that
        // names the session is not history.
        let recent = self
            .term
            .read(&second, ReadSource::RecentUnwrapped, Some(RECENT_LINES))
            .map_err(|error| Abort::Failed(error.to_string()))?;
        let recent = without_resume_header(&recent);
        let first = self.tokens.first().map(Token::join).unwrap_or_default();
        let second_token = self.tokens.get(1).map(Token::join).unwrap_or_default();
        let count_first = count_token(&recent, &first);
        let count_second = count_token(&recent, &second_token);
        let (was_first, was_second) = self.counts;
        if count_first < 1
            || count_first > was_first
            || count_second < 1
            || count_second > was_second
        {
            return Ok(Step::Fail {
                reason: format!(
                    "token counts after resume: A {count_first} (first pane {was_first}), B \
                     {count_second} (first pane {was_second}); each must be at least 1 and at most \
                     the first pane's count"
                ),
            });
        }

        match after_restore(self, &second)? {
            Step::Pass => {}
            other => return Ok(other),
        }

        // A third prompt proves the resumed session works.
        let token = self.token();
        self.term
            .run(&second, &token.prompt())
            .map_err(|error| Abort::Failed(error.to_string()))?;
        match self.wait_for_reply(
            &second,
            &token,
            REPLY_LIMIT,
            "the reply to the third prompt did not appear and stay stable within",
        )? {
            Step::Pass => {}
            other => return Ok(other),
        }

        // And the resumed session is closed as well.
        self.term
            .run(&second, QUIT_COMMAND)
            .map_err(|error| Abort::Failed(error.to_string()))?;
        let limit = self.limit(QUIT_LIMIT);
        let started = self.clock.now();
        let mut watch = Watch::new(started);
        loop {
            self.check_stop()?;
            let now = self.clock.now();
            if now >= self.deadline {
                return Err(Abort::Deadline);
            }
            let visible = self.watch_visible(&second, &mut watch)?;
            if let Some(code) = exit_code(&visible) {
                if code == 0 {
                    return Ok(Step::Pass);
                }
                return Ok(Step::Fail {
                    reason: format!(
                        "/quit in the resumed pane did not end talaria with exit code 0 within {} s",
                        self.limit_secs(QUIT_LIMIT)
                    ),
                });
            }
            if now - started >= limit {
                return Ok(Step::Fail {
                    reason: format!(
                        "/quit in the resumed pane did not end talaria with exit code 0 within {} s",
                        self.limit_secs(QUIT_LIMIT)
                    ),
                });
            }
            self.clock.sleep(POLL_INTERVAL);
        }
    }
}

/// What the approval step met in one attempt.
enum Approval {
    /// The card was answered.
    Answered,
    /// The command ran without a card.
    Skipped,
    /// The attempt failed.
    Failed(String),
    /// The model did not run the command; another attempt follows.
    Retry,
}

impl Run<'_> {
    /// Waits for the shell of a pane: one foreground command and a non-blank screen.
    pub(crate) fn wait_for_shell(&mut self, pane: &PaneId, base: u64) -> Result<(), Abort> {
        let limit = self.limit(base);
        let started = self.clock.now();
        let mut ready_since: Option<Duration> = None;
        loop {
            self.check_stop()?;
            let now = self.clock.now();
            if now >= self.deadline {
                return Err(Abort::Deadline);
            }
            let foreground = self
                .term
                .foreground(pane)
                .map_err(|error| Abort::Failed(error.to_string()))?;
            let visible = self
                .term
                .read(pane, ReadSource::Visible, None)
                .map_err(|error| Abort::Failed(error.to_string()))?;
            if foreground.len() == 1 && !visible.trim().is_empty() {
                let since = ready_since.get_or_insert(now);
                if now.saturating_sub(*since) >= Duration::from_secs(QUIET_SHELL) {
                    return Ok(());
                }
            } else {
                ready_since = None;
            }
            if now.saturating_sub(started) >= limit {
                return Err(Abort::Failed(format!(
                    "the shell did not become ready within {} s",
                    self.limit_secs(base)
                )));
            }
            self.clock.sleep(POLL_INTERVAL);
        }
    }

    /// Waits for the binary to show a screen that stays still.
    fn wait_for_screen(
        &mut self,
        pane: &PaneId,
        base: u64,
        quiet: u64,
        exit_prefix: &str,
        stable_prefix: &str,
    ) -> Result<(), Abort> {
        let limit = self.limit(base);
        let started = self.clock.now();
        let mut watch = Watch::new(started);
        let binary = self.binary();
        loop {
            self.check_stop()?;
            let now = self.clock.now();
            if now >= self.deadline {
                return Err(Abort::Deadline);
            }
            let visible = self.watch_visible(pane, &mut watch)?;
            if let Some(code) = exit_code(&visible) {
                return Err(Abort::Failed(format!(
                    "{exit_prefix} exited with code {code} before showing its screen"
                )));
            }
            let foreground = self
                .term
                .foreground(pane)
                .map_err(|error| Abort::Failed(error.to_string()))?;
            if foreground.iter().any(|line| line.contains(&binary))
                && now.saturating_sub(watch.changed_at) >= Duration::from_secs(quiet)
            {
                return Ok(());
            }
            if now.saturating_sub(started) >= limit {
                return Err(Abort::Failed(format!(
                    "{stable_prefix} did not show a stable screen within {} s",
                    self.limit_secs(base)
                )));
            }
            self.clock.sleep(POLL_INTERVAL);
        }
    }

    /// Waits for a reply that holds the token, and for the screen to settle.
    pub(crate) fn wait_for_reply(
        &mut self,
        pane: &PaneId,
        token: &Token,
        base: u64,
        reason: &str,
    ) -> Result<Step, Abort> {
        let limit = self.limit(base);
        let started = self.clock.now();
        let mut watch = Watch::new(started);
        let mut fallback = false;
        loop {
            self.check_stop()?;
            let now = self.clock.now();
            if now >= self.deadline {
                return Err(Abort::Deadline);
            }
            let recent = self
                .term
                .read(pane, ReadSource::RecentUnwrapped, Some(RECENT_LINES))
                .map_err(|error| Abort::Failed(error.to_string()))?;
            let visible = self.watch_visible(pane, &mut watch)?;
            let quiet = now.saturating_sub(watch.changed_at);
            if count_token(&recent, &token.join()) > 0 {
                if quiet >= Duration::from_secs(QUIET_REPLY) {
                    return Ok(Step::Pass);
                }
            } else if !fallback
                && now.saturating_sub(started) <= FALLBACK_WINDOW
                && quiet >= Duration::from_secs(QUIET_REPLY)
                && !card_words(&visible)
                && counting_values(&visible, COUNT_MARKER).is_empty()
            {
                self.term
                    .send_keys(pane, &["enter"])
                    .map_err(|error| Abort::Failed(error.to_string()))?;
                fallback = true;
                self.fallbacks += 1;
                self.step_fallbacks += 1;
            }
            if now.saturating_sub(started) >= limit {
                return Ok(Step::Fail {
                    reason: format!("{reason} {} s", self.limit_secs(base)),
                });
            }
            self.clock.sleep(POLL_INTERVAL);
        }
    }

    /// One attempt of the approval step.
    fn watch_approval(&mut self) -> Result<Approval, Abort> {
        let limit = self.limit(APPROVAL_LIMIT);
        let started = self.clock.now();
        let mut watch = Watch::new(started);
        loop {
            self.check_stop()?;
            let now = self.clock.now();
            if now >= self.deadline {
                return Err(Abort::Deadline);
            }
            let visible = self.watch_visible(&self.pane.clone(), &mut watch)?;
            let quiet = now.saturating_sub(watch.changed_at);
            match approval_card(&visible, APPROVAL_COMMAND) {
                Card::Absent => {
                    if !self.cache_exists() && quiet >= Duration::from_secs(QUIET_REPLY) {
                        return Ok(Approval::Skipped);
                    }
                    if self.cache_exists() && quiet >= Duration::from_secs(QUIET_RETRY) {
                        return Ok(Approval::Retry);
                    }
                }
                Card::OtherCommand => {
                    return Ok(Approval::Failed(
                        "an approval card for another command is open; the script does not answer it"
                            .to_owned(),
                    ));
                }
                Card::Shown {
                    once_digit,
                    once_highlighted,
                } => {
                    let Some(digit) = once_digit else {
                        return Ok(Approval::Failed(
                            // The reason names the card unit by its planning code; the code is
                            // split so that no source file spells it out in one piece.
                            concat!(
                                "card layout differs from unit P1",
                                "-09; cannot find the number for 'once'"
                            )
                            .to_owned(),
                        ));
                    };
                    let answered = self.clock.now();
                    self.term
                        .send_text(&self.pane, &digit.to_string())
                        .map_err(|error| Abort::Failed(error.to_string()))?;
                    self.clock.sleep(AFTER_DIGIT);
                    let visible = self
                        .term
                        .read(&self.pane, ReadSource::Visible, None)
                        .map_err(|error| Abort::Failed(error.to_string()))?;
                    if matches!(
                        approval_card(&visible, APPROVAL_COMMAND),
                        Card::Shown { .. }
                    ) {
                        if !once_highlighted {
                            return Ok(Approval::Failed(
                                "the highlighted choice is not 'once'; Enter was not pressed"
                                    .to_owned(),
                            ));
                        }
                        self.term
                            .send_keys(&self.pane, &["enter"])
                            .map_err(|error| Abort::Failed(error.to_string()))?;
                        self.clock.sleep(AFTER_ENTER);
                        let visible = self
                            .term
                            .read(&self.pane, ReadSource::Visible, None)
                            .map_err(|error| Abort::Failed(error.to_string()))?;
                        if matches!(
                            approval_card(&visible, APPROVAL_COMMAND),
                            Card::Shown { .. }
                        ) {
                            return Ok(Approval::Failed(
                                "the approval card was still open 3 s after it was answered"
                                    .to_owned(),
                            ));
                        }
                    }
                    self.answered_at = Some(answered);
                    return Ok(Approval::Answered);
                }
            }
            if now.saturating_sub(started) >= limit {
                return Ok(Approval::Failed(format!(
                    "no approval card and no outcome within {} s",
                    self.limit_secs(APPROVAL_LIMIT)
                )));
            }
            self.clock.sleep(POLL_INTERVAL);
        }
    }

    /// Waits for the command to delete the cache folder and the screen to settle.
    fn wait_for_deletion(&mut self) -> Result<Step, Abort> {
        let limit = self.limit(APPROVAL_AFTER_ANSWER);
        let started = self.answered_at.unwrap_or_else(|| self.clock.now());
        let mut watch = Watch::new(started);
        loop {
            self.check_stop()?;
            let now = self.clock.now();
            if now >= self.deadline {
                return Err(Abort::Deadline);
            }
            self.watch_visible(&self.pane.clone(), &mut watch)?;
            let quiet = now.saturating_sub(watch.changed_at);
            if !self.cache_exists() && quiet >= Duration::from_secs(QUIET_REPLY) {
                return Ok(Step::Pass);
            }
            if now.saturating_sub(started) >= limit {
                return Ok(Step::Fail {
                    reason: format!(
                        "the command did not finish within {} s of the answer",
                        self.limit_secs(APPROVAL_AFTER_ANSWER)
                    ),
                });
            }
            self.clock.sleep(POLL_INTERVAL);
        }
    }

    /// Makes sure the cache folder is there: the command deletes it.
    fn ensure_cache(&self) {
        let cache = self.cache_path();
        let _ = fs::create_dir_all(&cache);
        let _ = fs::write(cache.join("CACHEDIR.TAG"), "");
    }

    /// The cache folder of the scratch project.
    fn cache_path(&self) -> PathBuf {
        self.scratch().join(".pytest_cache")
    }

    /// Whether the cache folder is still there.
    fn cache_exists(&self) -> bool {
        self.cache_path().is_dir()
    }
}

/// Whether the screen holds the words of a card.
pub(crate) fn card_words(visible: &str) -> bool {
    crate::screen::holds_word(visible, "once") || crate::screen::holds_word(visible, "deny")
}

/// The first visible line that holds the whole word `withdrawn` or `blocked`.
fn withdrawn_line(visible: &str) -> Option<String> {
    visible.lines().find_map(|line| {
        let trimmed = line.trim();
        let holds = crate::screen::holds_word(trimmed, "withdrawn")
            || crate::screen::holds_word(trimmed, "blocked");
        holds.then(|| trimmed.to_owned())
    })
}

/// The scratch project: a private folder holding only the cache marker.
fn make_scratch(scratch: &Path) {
    let _ = fs::create_dir_all(scratch.join(".pytest_cache"));
    let _ = fs::write(scratch.join(".pytest_cache").join("CACHEDIR.TAG"), "");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = fs::set_permissions(scratch, fs::Permissions::from_mode(0o700));
    }
}

/// The message of a panic, cut to a length a row can hold.
pub(crate) fn panic_message(panic: &Box<dyn std::any::Any + Send>) -> String {
    let message = if let Some(text) = panic.downcast_ref::<&str>() {
        (*text).to_owned()
    } else if let Some(text) = panic.downcast_ref::<String>() {
        text.clone()
    } else {
        "the step panicked".to_owned()
    };
    message.chars().take(200).collect()
}
