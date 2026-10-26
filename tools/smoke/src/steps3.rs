//! The phase 3 smoke suite of issue #248.
//!
//! Twelve steps, in order. The limit table is the one the deadline test reads. Key names are
//! the ones a later live check sends. A shell step runs one line in the first pane and waits
//! for a new `SMOKE_EXIT:<n>` line; a screen step reads the pane. A check that something went
//! away reads only what changed since the key was sent (see `changed`), so a screen that
//! replaces itself and a screen that only grows are read alike.

use std::cell::RefCell;
use std::collections::HashMap;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::AtomicBool;
use std::time::Duration;

use rand::rngs::StdRng;
use rand::{RngExt, SeedableRng};

use crate::config::launch_line;
use crate::screen::{POLL_INTERVAL, Token, count_token};
use crate::screen2a::normalised;
use crate::screen3::{
    deny_digit, fleet_headers, fleet_row_state, has_here_row, launch_result, launched_id,
    needs_you_ids, needs_you_segment, opened_notice, worktree_line,
};
use crate::steps::{
    AFTER_ESC, Abort, QUIET_REPLY, RECENT_LINES, REPORT_LINES, Run, SHELL_LIMIT, Step, TabGuard,
    Watch, finish, panic_message,
};
use crate::terminal::{PaneId, ReadSource};
use crate::wording2b::USER_LINE_PREFIX;
use crate::wording3::*;
use crate::{Clock, PaneCapture, SmokeConfig, SmokeReport, StepOutcome, Terminal};

/// The steps, in the order the suite runs them.
pub const STEP_ORDER: [&str; 12] = [
    "fleet-quiet",
    "bg-launch",
    "worktree-launch",
    "fleet-list",
    "attach",
    "startup",
    "plain-answer",
    "local-commands",
    "fleet-view",
    "open-in-pane",
    "quit",
    "needs-you",
];

/// Each step's own limit, in seconds. The sum is 1095.
pub const STEP_LIMITS: &[(&str, u64)] = &[
    ("fleet-quiet", 30),
    ("bg-launch", 60),
    ("worktree-launch", 90),
    ("fleet-list", 180),
    ("attach", 60),
    ("startup", 30),
    ("plain-answer", 90),
    ("local-commands", 30),
    ("fleet-view", 30),
    ("open-in-pane", 45),
    ("quit", 30),
    ("needs-you", 420),
];

/// Jump to the session that needs an answer.
pub const KEY_JUMP: &str = "alt+a";
/// Open the subagent panel. The suite records the name; it does not send it.
pub const KEY_SUBAGENTS: &str = "alt+s";
/// Push. The suite records the name; it does not send it.
pub const KEY_PUSH: &str = "shift+p";
/// Stop. The suite records the name; it does not send it.
pub const KEY_STOP: &str = "shift+x";
/// The submit fallback a Herdr build may refuse. The suite records the name; it does not send it.
pub const KEY_SUBMIT_FALLBACK: &str = "ctrl+s";

/// The prompt of both background launches.
const OK_PROMPT: &str = "Reply with only the word ok.";
/// The text of that prompt the resumed transcript shows.
const OK_MARKER: &str = "Reply with only the word ok";
/// The command the needs-you launch asks the model to run, and the folder it would delete.
const PROBE_COMMAND: &str = "rm -rf ./talaria-smoke-probe";
const PROBE_FOLDER: &str = "talaria-smoke-probe";
/// The prompt of the needs-you launch.
const PROBE_PROMPT: &str = "Run this exact shell command: rm -rf ./talaria-smoke-probe";
/// The filter that matches no session.
const NO_MATCH_FILTER: &str = "zzz-no-match";
/// The start of a line that says `/bg` started a task.
const TASK_STARTED: &str = "Background task";

/// The waits, in seconds, before the scale is applied.
const QUIET_LIST: u64 = 30;
const LAUNCH: u64 = 15;
const WORKTREE: u64 = 90;
const INSPECT: u64 = 15;
const LIST_POLL: u64 = 120;
const LIST_LINE: u64 = 15;
const REFUSAL: u64 = 15;
const NO_GATEWAY: u64 = 10;
const ATTACH: u64 = 15;
const ATTACH_QUIT: u64 = 15;
const NO_MATCH: u64 = 20;
const STARTUP: u64 = 30;
const LOCAL: u64 = 5;
const GONE: u64 = 3;
const VIEW: u64 = 5;
const OPEN: u64 = 10;
const QUIT: u64 = 15;
const QUIET_FLEET: u64 = 30;
const NOTICE: u64 = 20;
const PEEK: u64 = 5;
const CARD: u64 = 5;
const CLEARED: u64 = 10;
const FLAG: u64 = 5;

/// The time between two fleet polls; never scaled.
const LIST_EVERY: Duration = Duration::from_secs(5);
/// The age past which a request cannot be answered before Hermes denies it; never scaled.
const TOO_OLD: Duration = Duration::from_secs(150);
/// The time between two direct fleet reads while the deny is checked.
const DIRECT_EVERY: Duration = Duration::from_secs(2);
/// The quiet period of the start-up screen, in seconds; never scaled.
const QUIET_STARTUP: u64 = 1;
/// The wait after typed text, so the screen takes it before the next input.
const AFTER_TYPED: Duration = Duration::from_millis(300);

/// Every shell line the suite types, built from the binary, the scratch root and the run id.
struct Lines {
    talaria: String,
    profile: String,
    quiet: String,
    bg: String,
    bg_refused: String,
    worktree: String,
    inspect: String,
    list: String,
    unserved: String,
    empty_home: String,
    needs_you: String,
    needs_list: String,
    start: String,
}

impl Lines {
    fn new(
        talaria: &Path,
        root: &Path,
        run_id: &str,
        cfg: (&str, &str, &str),
        hermes_home: Option<&Path>,
    ) -> Self {
        let (profile, model, provider) = cfg;
        // A new Herdr tab starts from Herdr's environment, not the caller's, so the first line
        // carries the caller's Hermes home into the tab's shell for every later line.
        let home = hermes_home
            .map(|home| format!("export HERMES_HOME={}; ", quote(home)))
            .unwrap_or_default();
        // Each line names the scratch root once, so the lines stay short under a long
        // temporary folder.
        let set = format!("SMOKE_ROOT={}; ", quote(root));
        let file = |name: &str| format!("\"$SMOKE_ROOT/{name}\"");
        let binary = quote(talaria);
        let launch = format!("--profile {profile} --model {model} --provider {provider}");
        let session = file("a");
        let tree = file(&format!("a/.worktrees/talaria-smoke-{run_id}"));
        Lines {
            talaria: binary.clone(),
            profile: profile.to_owned(),
            quiet: format!(
                "{home}{set}printf '%s %s\\n' \"$(tput cols)\" \"$(tput lines)\" > {}; {binary} fleet list --profile {profile} > {} 2>&1; echo SMOKE_EXIT:$?",
                file("size.out"),
                file("q.out")
            ),
            bg: format!(
                "{set}{binary} --bg --no-ensure --output-format json {launch} \"{OK_PROMPT}\" > {} 2> {}; echo SMOKE_EXIT:$?",
                file("bg1.out"),
                file("bg1.err")
            ),
            bg_refused: format!(
                "{set}{binary} --bg --no-ensure -p \"x\" > {} 2> {}; echo SMOKE_EXIT:$?",
                file("bg2.out"),
                file("bg2.err")
            ),
            worktree: format!(
                "{set}{binary} --bg --worktree --name smoke-{run_id} --no-ensure {launch} \"{OK_PROMPT}\" > {} 2>&1; echo SMOKE_EXIT:$?",
                file("wt.out")
            ),
            inspect: format!(
                "{set}git -C {} config --get branch.talaria/smoke-{run_id}.talaria-session > {} 2>&1; git -C {} log -1 --format=%s > {} 2>&1; git -C {} status --porcelain > {} 2>&1; echo SMOKE_EXIT:$?",
                session,
                file("wt.id"),
                tree,
                file("wt.log"),
                session,
                file("wt.st")
            ),
            list: format!(
                "{set}{binary} fleet list --profile {profile} --json > {} 2>&1; echo SMOKE_EXIT:$?",
                file("f.json")
            ),
            unserved: format!(
                "{set}{binary} fleet list --profile nosuch > {} 2>&1; echo SMOKE_EXIT:$?",
                file("f2.out")
            ),
            empty_home: format!(
                "{set}HERMES_HOME={} {binary} fleet list > {} 2>&1; echo SMOKE_EXIT:$?",
                file("empty-home"),
                file("f3.out")
            ),
            needs_you: format!(
                "{set}( cd {} && {binary} --bg --no-ensure --output-format json {launch} \"{PROBE_PROMPT}\" ) > {} 2> {}; echo SMOKE_EXIT:$?",
                file("probe"),
                file("ny.out"),
                file("ny.err")
            ),
            needs_list: format!(
                "{set}{binary} fleet list --profile {profile} --json > {} 2>&1; echo SMOKE_EXIT:$?",
                file("f4.json")
            ),
            start: launch_line(
                talaria,
                &[
                    "--no-ensure",
                    "--profile",
                    profile,
                    "--model",
                    model,
                    "--provider",
                    provider,
                ],
            ),
        }
    }

    /// `talaria attach` for a session reference.
    fn attach(&self, reference: &str) -> String {
        format!(
            "{} attach --no-ensure --profile {} {reference}; echo SMOKE_EXIT:$?",
            self.talaria, self.profile
        )
    }

    fn all(&self) -> Vec<String> {
        vec![
            self.quiet.clone(),
            self.bg.clone(),
            self.bg_refused.clone(),
            self.worktree.clone(),
            self.inspect.clone(),
            self.list.clone(),
            self.unserved.clone(),
            self.empty_home.clone(),
            self.attach("00000000"),
            self.attach("zz-none-00000000"),
            self.needs_you.clone(),
            self.needs_list.clone(),
            self.start.clone(),
        ]
    }
}

/// Every phase 3 shell line, for the preflight's length bound.
pub fn shell_lines(
    talaria: &Path,
    root: &Path,
    run_id: &str,
    cfg: (&str, &str, &str),
    hermes_home: Option<&Path>,
) -> Vec<String> {
    Lines::new(talaria, root, run_id, cfg, hermes_home).all()
}

/// The caller's Hermes home, when one is set.
fn caller_hermes_home() -> Option<PathBuf> {
    std::env::var_os("HERMES_HOME")
        .filter(|home| !home.is_empty())
        .map(PathBuf::from)
}

/// The scratch root; removed on drop unless the captures are kept.
struct Scratch {
    root: PathBuf,
    keep: bool,
}

impl Drop for Scratch {
    fn drop(&mut self) {
        if self.keep {
            return;
        }
        // The worktree is registered in the session repository; unregister it first.
        let session = self.root.join("a");
        if let Ok(read) = fs::read_dir(session.join(".worktrees")) {
            for entry in read.flatten() {
                let _ = git(
                    &session,
                    &["worktree", "remove", "--force"],
                    &[&entry.path()],
                );
            }
        }
        let _ = fs::remove_dir_all(&self.root);
    }
}

/// Runs git with a clean environment.
fn git(dir: &Path, args: &[&str], paths: &[&Path]) -> std::io::Result<()> {
    let output = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args([
            "-c",
            "user.name=smoke",
            "-c",
            "user.email=smoke@example.invalid",
        ])
        .args(args)
        .args(paths)
        .env_clear()
        .output()?;
    if output.status.success() {
        Ok(())
    } else {
        Err(std::io::Error::other(
            String::from_utf8_lossy(&output.stderr).into_owned(),
        ))
    }
}

/// The bare origin, the session clone `a` one commit behind it, the clone `other` that made
/// that commit, the probe folder and the empty Hermes home.
fn make_scratch(root: &Path, keep: bool) -> std::io::Result<Scratch> {
    fs::create_dir(root)?;
    let guard = Scratch {
        root: root.to_path_buf(),
        keep,
    };
    fs::set_permissions(root, fs::Permissions::from_mode(0o700))?;
    let origin = root.join("origin.git");
    let session = root.join("a");
    let other = root.join("other");
    git(root, &["init", "--bare", "-b", "main", "origin.git"], &[])?;
    git(root, &["init", "-b", "main", "a"], &[])?;
    git(&session, &["commit", "--allow-empty", "-m", "init"], &[])?;
    git(&session, &["remote", "add", "origin"], &[&origin])?;
    git(&session, &["push", "-u", "origin", "main"], &[])?;
    git(root, &["clone", "origin.git", "other"], &[])?;
    git(&other, &["commit", "--allow-empty", "-m", "two"], &[])?;
    git(&other, &["push", "origin", "main"], &[])?;
    fs::create_dir_all(root.join("probe").join(PROBE_FOLDER))?;
    fs::create_dir(root.join("empty-home"))?;
    Ok(guard)
}

/// Runs the twelve checks, stopping at the first failure.
pub fn run_smoke_3(
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
    let run_id: String = (0..8)
        .map(|_| alphabet[rng.random_range(0..alphabet.len())] as char)
        .collect();
    let label = format!("talaria-smoke-{run_id}");
    let root = cfg.temp_root.join(&label);
    let mut report = SmokeReport {
        outcomes: Vec::new(),
        version: term.version(),
        elapsed: Duration::ZERO,
        fallbacks: 0,
        interrupted: false,
        unclosed_tab: None,
        unclosed_session: None,
        tab: None,
        scratch: root.clone(),
        fallback_steps: Vec::new(),
        notes: Vec::new(),
    };
    let setup_failed = |report: &mut SmokeReport, reason: String| {
        report.outcomes.push(StepOutcome::Fail {
            step: "startup",
            reason,
            capture: Vec::new(),
        });
        report.elapsed = clock.now().saturating_sub(start);
    };
    let _scratch = match make_scratch(&root, cfg.report_dir.is_some()) {
        Ok(guard) => guard,
        Err(error) => {
            setup_failed(
                &mut report,
                format!("the scratch folder could not be created: {error}"),
            );
            return report;
        }
    };
    let tab = match term.create_tab(&label, &root.join("a")) {
        Ok(tab) => tab,
        Err(error) => {
            setup_failed(
                &mut report,
                format!("the tab could not be created: {error}"),
            );
            return report;
        }
    };
    report.tab = Some(tab.tab.clone());
    let cleanup = RefCell::new(None);
    let guard = TabGuard::new(term, tab.tab, &cleanup);
    let lines = Lines::new(
        &cfg.talaria,
        &root,
        &run_id,
        (&cfg.profile, &cfg.model, &cfg.provider),
        caller_hermes_home().as_deref(),
    );
    let mut run = Run3 {
        base: Run {
            term,
            clock,
            cfg,
            stop,
            deadline: start + cfg.deadline,
            rng,
            tokens: Vec::new(),
            scratch: root.join("a"),
            answered_at: None,
            fallbacks: 0,
            step_fallbacks: 0,
            fallback_steps: Vec::new(),
            interrupted: false,
            pane: tab.pane,
            second: None,
            counts: (0, 0),
        },
        root,
        run_id,
        lines,
        id1: String::new(),
        id2: String::new(),
        launched_at: HashMap::new(),
        waiting: false,
        notes: Vec::new(),
    };
    for name in STEP_ORDER {
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
                    Ok(Step::Pass | Step::Skip { .. }) => {
                        unreachable!("passing and skipped outcomes were handled above")
                    }
                };
                if run.waiting {
                    run.notes.push((
                        name,
                        format!(
                            "{name}: a request may be waiting: it is denied by Hermes after 300 s"
                        ),
                    ));
                }
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
    report.notes = run.notes;
    report
}

/// The state of one phase 3 run.
struct Run3<'a> {
    base: Run<'a>,
    /// The scratch root.
    root: PathBuf,
    run_id: String,
    lines: Lines,
    /// The background session of `bg-launch`.
    id1: String,
    /// The worktree session.
    id2: String,
    /// When each background session was first launched.
    launched_at: HashMap<String, Duration>,
    /// An approval request the suite provoked is listed and not yet denied.
    waiting: bool,
    notes: Vec<(&'static str, String)>,
}

fn failed(error: impl std::fmt::Display) -> Abort {
    Abort::Failed(error.to_string())
}

fn fail(reason: impl Into<String>) -> Result<Step, Abort> {
    Ok(Step::Fail {
        reason: reason.into(),
    })
}

/// The path in single quotes, with each single quote written as `'\''`.
fn quote(path: &Path) -> String {
    format!("'{}'", path.to_string_lossy().replace('\'', "'\\''"))
}

/// The exit codes of the `SMOKE_EXIT:<n>` lines, in order.
fn exit_markers(text: &str) -> Vec<i32> {
    text.lines()
        .filter_map(|line| line.trim().strip_prefix("SMOKE_EXIT:")?.parse().ok())
        .collect()
}

/// The lines after the `count`-th exit marker (all lines when `count` is 0).
fn after_markers(text: &str, count: usize) -> Vec<&str> {
    let lines: Vec<&str> = text.lines().collect();
    if count == 0 {
        return lines;
    }
    let mut seen = 0;
    for (index, line) in lines.iter().enumerate() {
        let marker = line
            .trim()
            .strip_prefix("SMOKE_EXIT:")
            .is_some_and(|code| code.parse::<i32>().is_ok());
        if marker {
            seen += 1;
            if seen == count {
                return lines[index + 1..].to_vec();
            }
        }
    }
    Vec::new()
}

/// The part of the screen `after` that `before` did not already show.
///
/// A screen that only grew (new lines below, the oldest scrolled away) gives the new lines.
/// A screen that was replaced, or did not change at all, gives the whole of `after`, so a
/// check that something went away fails while the screen stays as it was.
fn changed(before: &str, after: &str) -> String {
    let lines = |text: &str| -> Vec<String> {
        text.lines()
            .map(|line| line.trim_end().to_owned())
            .filter(|line| !line.trim().is_empty())
            .collect()
    };
    let old = lines(before);
    let new = lines(after);
    if old == new {
        return after.to_owned();
    }
    for start in 0..old.len() {
        let tail = &old[start..];
        if tail.len() < new.len() && new[..tail.len()] == *tail {
            return new[tail.len()..].join("\n");
        }
    }
    after.to_owned()
}

/// Whether the line echoes the typed command, bare or after the user prefix.
fn command_echo(line: &str, command: &str) -> bool {
    normalised(line)
        .first()
        .is_some_and(|line| line == command || line.strip_prefix(USER_LINE_PREFIX) == Some(command))
}

/// Whether the fleet overlay is on the screen: its title line, or this session's row. A
/// filtered list can hide the row, so the title counts as well.
fn overlay_open(screen: &str) -> bool {
    has_here_row(screen)
        || normalised(screen)
            .iter()
            .any(|line| line.starts_with(FLEET_TITLE))
}

/// Whether a line starts with `── Resumed:` and a later line holds the session's prompt.
fn resumed(visible: &str) -> bool {
    let lines: Vec<&str> = visible.lines().collect();
    lines
        .iter()
        .rposition(|line| line.trim().starts_with(RESUMED_PREFIX))
        .is_some_and(|index| {
            lines[index + 1..]
                .iter()
                .any(|line| line.contains(OK_MARKER))
        })
}

impl Run3<'_> {
    fn pane(&self) -> PaneId {
        self.base.pane.clone()
    }

    fn check(&self) -> Result<(), Abort> {
        self.base.check_stop()?;
        if self.base.clock.now() >= self.base.deadline {
            return Err(Abort::Deadline);
        }
        Ok(())
    }

    fn now(&self) -> Duration {
        self.base.clock.now()
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

    /// How many exit markers the pane shows so far.
    fn marker_count(&self) -> Result<usize, Abort> {
        Ok(exit_markers(&self.read(ReadSource::RecentUnwrapped)?).len())
    }

    /// The exit code of a talaria that held the pane: a new exit marker, once the binary has
    /// left the foreground. Herdr's recent text can repeat older lines after talaria leaves
    /// its screen, so a new marker alone does not prove the exit.
    fn ended(&self, before: usize) -> Result<Option<i32>, Abort> {
        let markers = exit_markers(&self.read(ReadSource::RecentUnwrapped)?);
        if markers.len() <= before {
            return Ok(None);
        }
        let binary = self.base.cfg.talaria.to_string_lossy().into_owned();
        let foreground = self.base.term.foreground(&self.base.pane).map_err(failed)?;
        if foreground.iter().any(|line| line.contains(&binary)) {
            return Ok(None);
        }
        Ok(markers.last().copied())
    }

    /// A slash line or prompt, submitted with Enter and no fallback.
    fn submit(&mut self, text: &str) -> Result<(), Abort> {
        self.check()?;
        self.base.term.run(&self.base.pane, text).map_err(failed)
    }

    /// Text without Enter.
    fn typed(&mut self, text: &str) -> Result<(), Abort> {
        self.check()?;
        self.base
            .term
            .send_text(&self.base.pane, text)
            .map_err(failed)?;
        self.base.clock.sleep(AFTER_TYPED);
        Ok(())
    }

    fn key(&mut self, key: &str) -> Result<(), Abort> {
        self.check()?;
        self.base
            .term
            .send_keys(&self.base.pane, &[key])
            .map_err(failed)?;
        if key == "esc" {
            self.base.clock.sleep(AFTER_ESC);
        }
        Ok(())
    }

    /// Waits without reading, still answering the signal and the deadline.
    fn pause(&self, total: Duration) -> Result<(), Abort> {
        let end = self.now() + total;
        loop {
            self.check()?;
            let now = self.now();
            if now >= end {
                return Ok(());
            }
            self.base.clock.sleep(POLL_INTERVAL.min(end - now));
        }
    }

    /// Polls one source until `accepts` holds; false when the wait ran out.
    fn wait(
        &mut self,
        base: u64,
        source: ReadSource,
        mut accepts: impl FnMut(&str) -> bool,
    ) -> Result<bool, Abort> {
        let limit = self.base.limit(base);
        let started = self.now();
        loop {
            self.check()?;
            if accepts(&self.read(source)?) {
                return Ok(true);
            }
            if self.now().saturating_sub(started) >= limit {
                return Ok(false);
            }
            self.base.clock.sleep(POLL_INTERVAL);
        }
    }

    /// Polls the visible screen until `accepts` holds for what changed since `before`.
    fn wait_changed(
        &mut self,
        base: u64,
        before: &str,
        mut accepts: impl FnMut(&str) -> bool,
    ) -> Result<bool, Abort> {
        self.wait(base, ReadSource::Visible, |visible| {
            accepts(&changed(before, visible))
        })
    }

    /// Runs one shell line and waits for its exit marker; `None` when none came in time.
    fn shell(&mut self, line: &str, base: u64) -> Result<Option<i32>, Abort> {
        self.check()?;
        let before = self.marker_count()?;
        self.base.term.run(&self.base.pane, line).map_err(failed)?;
        let mut code = None;
        self.wait(base, ReadSource::RecentUnwrapped, |text| {
            let markers = exit_markers(text);
            if markers.len() > before {
                code = markers.last().copied();
            }
            code.is_some()
        })?;
        Ok(code)
    }

    /// A shell line whose marker must come; a missing marker ends the step.
    fn shell_done(&mut self, line: &str, base: u64, what: &str) -> Result<i32, Abort> {
        self.shell(line, base)?.ok_or_else(|| {
            Abort::Failed(format!(
                "{what} did not finish within {} s",
                self.base.limit_secs(base)
            ))
        })
    }

    /// A file the shell wrote under the scratch root; empty when it is missing.
    fn file(&self, name: &str) -> String {
        fs::read_to_string(self.root.join(name)).unwrap_or_default()
    }

    /// The screen of the first pane, for the failure report.
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
            "fleet-quiet" => self.fleet_quiet(),
            "bg-launch" => self.bg_launch(),
            "worktree-launch" => self.worktree_launch(),
            "fleet-list" => self.fleet_list(),
            "attach" => self.attach(),
            "startup" => self.startup(),
            "plain-answer" => self.base.plain_answer(),
            "local-commands" => self.local_commands(),
            "fleet-view" => self.fleet_view(),
            "open-in-pane" => self.open_in_pane(),
            "quit" => self.quit(),
            "needs-you" => self.needs_you(),
            other => fail(format!("there is no step named {other}")),
        }
    }
}

impl Run3<'_> {
    /// No leftover session needs an answer or is still working, and the pane is big enough.
    fn fleet_quiet(&mut self) -> Result<Step, Abort> {
        if let Err(abort) = self.base.wait_for_shell(&self.pane(), SHELL_LIMIT) {
            return finish(abort);
        }
        let line = self.lines.quiet.clone();
        let code = self.shell_done(&line, QUIET_LIST, "talaria fleet list")?;
        if code != 0 {
            return fail(format!(
                "talaria fleet list exited with code {code} (is the Hermes gateway running for profile {}?)",
                self.base.cfg.profile
            ));
        }
        let size = self.file("size.out");
        let mut numbers = size.split_whitespace().map(str::parse::<u32>);
        let (Some(Ok(columns)), Some(Ok(rows))) = (numbers.next(), numbers.next()) else {
            return fail(format!(
                "the size of the smoke tab could not be read: {:?}",
                size.trim()
            ));
        };
        if columns < 100 || rows < 24 {
            return fail(format!(
                "the smoke tab is {columns} columns by {rows} rows; phase 3 needs at least 100 by 24"
            ));
        }
        let headers = fleet_headers(&self.file("q.out"));
        let count = |name: &str| {
            headers
                .iter()
                .filter(|(header, _)| header == name)
                .map(|(_, count)| *count)
                .sum::<u32>()
        };
        let busy: Vec<String> = [HEADER_NEEDS_YOU, HEADER_WORKING]
            .into_iter()
            .filter(|name| count(name) > 0)
            .map(|name| format!("{name} ({})", count(name)))
            .collect();
        if !busy.is_empty() {
            return fail(format!(
                "the fleet is not quiet ({}); answer or stop those sessions and run again",
                busy.join(", ")
            ));
        }
        Ok(Step::Pass)
    }

    /// One background launch in JSON, and the refused `--bg -p` combination.
    fn bg_launch(&mut self) -> Result<Step, Abort> {
        let line = self.lines.bg.clone();
        let started = self.now();
        let Some(code) = self.shell(&line, LAUNCH)? else {
            return fail(format!(
                "talaria --bg took {} s to return, over the {} s limit",
                self.now().saturating_sub(started).as_secs(),
                self.base.limit_secs(LAUNCH)
            ));
        };
        if code != 0 {
            return fail(format!(
                "talaria --bg ended with exit code {code}, expected 0"
            ));
        }
        match launch_result(&self.file("bg1.out")) {
            Some(result) if result.status == "launched" && !result.session_id.is_empty() => {
                self.launched_at
                    .entry(result.session_id.clone())
                    .or_insert(started);
                self.id1 = result.session_id;
            }
            _ => {
                return fail(
                    "the output was not exactly one line with \"status\":\"launched\" and a session_id",
                );
            }
        }
        let line = self.lines.bg_refused.clone();
        let code = self.shell_done(&line, REFUSAL, "talaria --bg -p")?;
        if code != 2 {
            return fail(format!(
                "talaria --bg -p ended with exit code {code}, expected 2"
            ));
        }
        if !self.file("bg2.out").is_empty() || !self.file("bg2.err").contains(BG_CANNOT_COMBINE) {
            return fail(format!(
                "talaria --bg -p printed on standard output or did not say \"{BG_CANNOT_COMBINE}\" on standard error"
            ));
        }
        Ok(Step::Pass)
    }

    /// A worktree launch from the fetched origin that leaves the main checkout clean.
    fn worktree_launch(&mut self) -> Result<Step, Abort> {
        let name = format!("smoke-{}", self.run_id);
        let line = self.lines.worktree.clone();
        let code = self.shell_done(&line, WORKTREE, "talaria --bg --worktree")?;
        if code != 0 {
            return fail(format!(
                "talaria --bg --worktree ended with exit code {code}, expected 0"
            ));
        }
        let out = self.file("wt.out");
        let (Some(_), Some(printed)) = (worktree_line(&out, &name), launched_id(&out)) else {
            return fail(format!(
                "the output lacked \"{WORKTREE_PREFIX}{name}{}\" or \"{}\"",
                WORKTREE_FROM.trim_end(),
                LAUNCHED_PREFIX.trim_end()
            ));
        };
        let line = self.lines.inspect.clone();
        let code = self.shell_done(&line, INSPECT, "the worktree inspection")?;
        if code != 0 {
            return fail(format!(
                "git status in the main checkout ended with exit code {code}, expected 0"
            ));
        }
        let recorded = self.file("wt.id");
        let recorded = recorded.trim();
        self.id2 = if !recorded.is_empty() && !recorded.contains(char::is_whitespace) {
            recorded.to_owned()
        } else {
            self.notes.push((
                "worktree-launch",
                "worktree-launch: no talaria-session record; using the printed id".to_owned(),
            ));
            printed
        };
        let tree = format!(".worktrees/talaria-{name}");
        if !self.root.join("a").join(&tree).is_dir() {
            return fail(format!("the folder {tree} was not made"));
        }
        let subject = self.file("wt.log");
        let subject = subject.trim();
        if subject != "two" {
            return fail(format!(
                "the worktree started at \"{subject}\", not at the commit \"two\" that origin had and the checkout did not"
            ));
        }
        let status = self.file("wt.st");
        if let Some(first) = status.lines().find(|line| !line.trim().is_empty()) {
            return fail(format!("the main checkout is no longer clean: {first}"));
        }
        Ok(Step::Pass)
    }

    /// Both sessions are listed, and the two refusals hold.
    fn fleet_list(&mut self) -> Result<Step, Abort> {
        let limit = self.base.limit(LIST_POLL);
        let started = self.now();
        let line = self.lines.list.clone();
        let document = loop {
            let code = self.shell_done(&line, LIST_LINE, "talaria fleet list --json")?;
            if code != 0 {
                return fail(format!(
                    "talaria fleet list --json ended with exit code {code}, expected 0"
                ));
            }
            let document = self.file("f.json");
            let state = fleet_row_state(&document, &self.id1);
            if matches!(state.as_deref(), Some("done" | "idle")) {
                break document;
            }
            if self.now().saturating_sub(started) >= limit {
                let short: String = self.id1.chars().take(8).collect();
                return fail(format!(
                    "session {short} was not listed as done or idle within {} s (last state: {})",
                    self.base.limit_secs(LIST_POLL),
                    state.as_deref().unwrap_or("absent")
                ));
            }
            self.pause(LIST_EVERY)?;
        };
        if fleet_row_state(&document, &self.id2).is_none() {
            let short: String = self.id2.chars().take(8).collect();
            return fail(format!(
                "the worktree session {short} is not in the fleet list"
            ));
        }
        let line = self.lines.unserved.clone();
        let code = self.shell(&line, REFUSAL)?;
        if code != Some(2) || !self.file("f2.out").contains(PROFILE_NOT_SERVED) {
            return fail(format!(
                "--profile nosuch did not exit 2 with the sentence \"{PROFILE_NOT_SERVED}\""
            ));
        }
        let line = self.lines.empty_home.clone();
        let code = self.shell(&line, NO_GATEWAY)?;
        if code != Some(1) || !self.file("f3.out").contains(NO_GATEWAY_HOME) {
            return fail(format!(
                "with an empty Hermes home, talaria fleet list did not exit 1 with \"{NO_GATEWAY_HOME}\""
            ));
        }
        let untouched = fs::read_dir(self.root.join("empty-home"))
            .map(|mut read| read.next().is_none())
            .unwrap_or(false);
        if !untouched {
            return fail(
                "talaria fleet list wrote into the empty Hermes home (it must start nothing)",
            );
        }
        Ok(Step::Pass)
    }

    /// `talaria attach` opens the first session by its short id and refuses an unknown one.
    fn attach(&mut self) -> Result<Step, Abort> {
        let short: String = self
            .id1
            .strip_prefix("local-")
            .unwrap_or(&self.id1)
            .chars()
            .take(8)
            .collect();
        let line = self.lines.attach(&short);
        let before = self.marker_count()?;
        self.submit(&line)?;
        let limit = self.base.limit(ATTACH);
        let started = self.now();
        loop {
            self.check()?;
            if let Some(code) = self.ended(before)? {
                if code == 2 {
                    let recent = self.read(ReadSource::RecentUnwrapped)?;
                    let text = after_markers(&recent, before);
                    let first = text
                        .iter()
                        .map(|line| line.trim())
                        .find(|line| line.starts_with("talaria:"))
                        .or_else(|| text.iter().map(|line| line.trim()).find(|l| !l.is_empty()))
                        .unwrap_or_default()
                        .to_owned();
                    return fail(format!(
                        "talaria attach {short} matched several sessions (exit 2): {first}"
                    ));
                }
                break;
            }
            if resumed(&self.read(ReadSource::Visible)?) {
                break;
            }
            if self.now().saturating_sub(started) >= limit {
                break;
            }
            self.base.clock.sleep(POLL_INTERVAL);
        }
        if self.ended(before)?.is_some() || !resumed(&self.read(ReadSource::Visible)?) {
            return fail(format!(
                "talaria attach {short} did not show \"{}\" and the session's first prompt within {} s",
                RESUMED_PREFIX,
                self.base.limit_secs(ATTACH)
            ));
        }

        let before = self.marker_count()?;
        self.typed("/quit")?;
        self.key("enter")?;
        let limit = self.base.limit(ATTACH_QUIT);
        let started = self.now();
        loop {
            self.check()?;
            if self.ended(before)? == Some(0) {
                break;
            }
            if self.now().saturating_sub(started) >= limit {
                return fail(format!(
                    "/quit in the attached session did not end talaria with exit code 0 within {} s",
                    self.base.limit_secs(ATTACH_QUIT)
                ));
            }
            self.base.clock.sleep(POLL_INTERVAL);
        }

        let reference = format!("zz-none-{}", self.run_id);
        let line = self.lines.attach(&reference);
        let before = self.marker_count()?;
        self.submit(&line)?;
        if !self.wait(NO_MATCH, ReadSource::RecentUnwrapped, |text| {
            let markers = exit_markers(text);
            markers.len() > before
                && markers.last() == Some(&1)
                && after_markers(text, before).iter().any(|line| {
                    line.contains(NO_SESSION_MATCHES_PREFIX) && line.contains("zz-none-")
                })
        })? {
            return fail(format!(
                "talaria attach {reference} did not exit 1 with \"{}\"",
                NO_SESSION_MATCHES_PREFIX.trim_end_matches([' ', '"'])
            ));
        }
        Ok(Step::Pass)
    }

    /// Starts talaria in the session folder; the exit markers the pane showed before.
    fn launch_talaria(&mut self) -> Result<usize, Abort> {
        let before = self.marker_count()?;
        let line = self.lines.start.clone();
        self.submit(&line)?;
        Ok(before)
    }

    /// The shared start-up of the first suite: the shell is ready and talaria shows a stable
    /// screen. Exit markers from the earlier shell lines are not talaria's.
    fn startup(&mut self) -> Result<Step, Abort> {
        if let Err(abort) = self.base.wait_for_shell(&self.pane(), SHELL_LIMIT) {
            return finish(abort);
        }
        let before = self.launch_talaria()?;
        let limit = self.base.limit(STARTUP);
        let started = self.now();
        let mut watch = Watch::new(started);
        let binary = self.base.cfg.talaria.to_string_lossy().into_owned();
        loop {
            self.check()?;
            self.base.watch_visible(&self.pane(), &mut watch)?;
            if let Some(code) = self.ended(before)? {
                return fail(format!(
                    "talaria exited with code {code} before showing its screen"
                ));
            }
            let now = self.now();
            let foreground = self.base.term.foreground(&self.base.pane).map_err(failed)?;
            if foreground.iter().any(|line| line.contains(&binary))
                && now.saturating_sub(watch.changed_at) >= Duration::from_secs(QUIET_STARTUP)
            {
                return Ok(Step::Pass);
            }
            if now.saturating_sub(started) >= limit {
                return fail(format!(
                    "startup did not show a stable screen within {} s",
                    self.base.limit_secs(STARTUP)
                ));
            }
            self.base.clock.sleep(POLL_INTERVAL);
        }
    }

    /// Esc, then a short wait for `text` to leave the screen. The panels have no closing
    /// check of their own, so the wait never fails the step.
    fn close_panel(&mut self, text: &str) -> Result<(), Abort> {
        let before = self.read(ReadSource::Visible)?;
        self.key("esc")?;
        self.wait_changed(GONE, &before, |screen| !screen.contains(text))?;
        Ok(())
    }

    /// The empty subagent panel, the unknown option, the empty process list and `/bg` alone.
    fn local_commands(&mut self) -> Result<Step, Abort> {
        self.submit("/tasks")?;
        if !self.wait(LOCAL, ReadSource::Visible, |screen| {
            screen.contains(NO_SUBAGENTS)
        })? {
            return fail(format!(
                "/tasks did not say \"{NO_SUBAGENTS}\" within {} s",
                self.base.limit_secs(LOCAL)
            ));
        }
        self.close_panel(NO_SUBAGENTS)?;

        self.submit("/agents frobnicate")?;
        if !self.wait(LOCAL, ReadSource::RecentUnwrapped, |text| {
            text.contains(AGENTS_UNKNOWN_OPTION)
        })? {
            return fail(format!(
                "/agents frobnicate did not print the unknown-option sentence within {} s",
                self.base.limit_secs(LOCAL)
            ));
        }

        self.submit("/processes")?;
        if !self.wait(LOCAL, ReadSource::Visible, |screen| {
            screen.contains(NO_PROCESSES)
        })? {
            return fail(format!(
                "/processes did not say \"{NO_PROCESSES}\" within {} s",
                self.base.limit_secs(LOCAL)
            ));
        }
        self.close_panel(NO_PROCESSES)?;

        let mark = self.read(ReadSource::RecentUnwrapped)?.lines().count();
        self.submit("/bg")?;
        if !self.wait(LOCAL, ReadSource::RecentUnwrapped, |text| {
            let lines: Vec<&str> = text.lines().collect();
            let start = lines
                .iter()
                .rposition(|line| command_echo(line, "/bg"))
                .map_or(mark.min(lines.len()), |index| index + 1);
            let region = normalised(&lines[start..].join("\n"));
            normalised(text)
                .iter()
                .any(|line| line.starts_with(BG_USAGE_PREFIX))
                && !region.iter().any(|line| line.starts_with(TASK_STARTED))
        })? {
            return fail("/bg alone did not print its usage line, or started a task");
        }
        Ok(Step::Pass)
    }

    /// The fleet view opens on this session, filters to nothing and closes with Ctrl+C.
    fn fleet_view(&mut self) -> Result<Step, Abort> {
        self.submit("/fleet")?;
        if !self.wait(VIEW, ReadSource::Visible, has_here_row)? {
            return fail(format!(
                "/fleet did not open an overlay with this session's row marked \"(here)\" within {} s",
                self.base.limit_secs(VIEW)
            ));
        }
        self.typed("/")?;
        self.typed(NO_MATCH_FILTER)?;
        self.key("enter")?;
        if !self.wait(VIEW, ReadSource::Visible, |screen| {
            screen.contains(NO_SESSIONS_MATCH)
        })? {
            return fail(format!(
                "filtering for {NO_MATCH_FILTER} did not say \"No sessions match\" within {} s",
                self.base.limit_secs(VIEW)
            ));
        }
        let before = self.read(ReadSource::Visible)?;
        self.key("ctrl+c")?;
        let model = self.base.cfg.model.clone();
        if !self.wait_changed(VIEW, &before, |screen| {
            !overlay_open(screen) && screen.contains(&model)
        })? {
            return fail(format!(
                "Ctrl+C did not close the overlay within {} s",
                self.base.limit_secs(VIEW)
            ));
        }
        Ok(Step::Pass)
    }

    /// `o` on the first session opens it in a new Herdr pane.
    fn open_in_pane(&mut self) -> Result<Step, Abort> {
        self.submit("/fleet")?;
        if !self.wait(VIEW, ReadSource::Visible, has_here_row)? {
            return fail(format!(
                "/fleet did not open an overlay with this session's row marked \"(here)\" within {} s",
                self.base.limit_secs(VIEW)
            ));
        }
        self.typed("/")?;
        let id = self.id1.clone();
        self.typed(&id)?;
        self.key("enter")?;
        self.typed("o")?;
        let limit = self.base.limit(OPEN);
        let started = self.now();
        loop {
            self.check()?;
            let visible = self.read(ReadSource::Visible)?;
            let recent = self.read(ReadSource::RecentUnwrapped)?;
            if opened_notice(&visible).is_some() || opened_notice(&recent).is_some() {
                break;
            }
            if visible.contains(NOT_IN_HERDR) || recent.contains(NOT_IN_HERDR) {
                return fail(format!(
                    "talaria did not detect Herdr and printed: {NOT_IN_HERDR}"
                ));
            }
            if self.now().saturating_sub(started) >= limit {
                return fail(format!(
                    "no line \"{OPENED_PREFIX}<title>{OPENED_HERDR_SUFFIX}\" within {} s",
                    self.base.limit_secs(OPEN)
                ));
            }
            self.base.clock.sleep(POLL_INTERVAL);
        }
        // The overlay stays open after `o`. Ctrl+C closes it; on the main screen it would arm
        // the exit instead, so it is sent only while the overlay shows.
        if overlay_open(&self.read(ReadSource::Visible)?) {
            self.key("ctrl+c")?;
        }
        Ok(Step::Pass)
    }

    /// `/quit`, with the first suite's single Enter fallback; the exit code when it ended.
    fn quit_talaria(&mut self) -> Result<Option<i32>, Abort> {
        let before = self.marker_count()?;
        self.submit("/quit")?;
        let limit = self.base.limit(QUIT);
        let started = self.now();
        let mut watch = Watch::new(started);
        let mut fallback = false;
        loop {
            self.check()?;
            self.base.watch_visible(&self.pane(), &mut watch)?;
            if let Some(code) = self.ended(before)? {
                return Ok(Some(code));
            }
            let now = self.now();
            if !fallback
                && now.saturating_sub(started) <= limit
                && now.saturating_sub(watch.changed_at) >= Duration::from_secs(QUIET_REPLY)
            {
                self.base
                    .term
                    .send_keys(&self.base.pane, &["enter"])
                    .map_err(failed)?;
                fallback = true;
                self.base.fallbacks += 1;
                self.base.step_fallbacks += 1;
            }
            if now.saturating_sub(started) >= limit {
                return Ok(None);
            }
            self.base.clock.sleep(POLL_INTERVAL);
        }
    }

    /// The session is closed and the answer of token A is still in the pane.
    fn quit(&mut self) -> Result<Step, Abort> {
        match self.quit_talaria()? {
            None => {
                return fail(format!(
                    "/quit did not end talaria within {} s",
                    self.base.limit_secs(QUIT)
                ));
            }
            Some(0) => {}
            Some(code) => {
                return fail(format!(
                    "/quit ended talaria with exit code {code}, expected 0"
                ));
            }
        }
        let foreground = self.base.term.foreground(&self.base.pane).map_err(failed)?;
        let binary = self.base.cfg.talaria.to_string_lossy().into_owned();
        if foreground.len() != 1 || foreground[0].contains(&binary) {
            return fail("the shell did not return to the foreground after /quit");
        }
        let recent = self.read(ReadSource::RecentUnwrapped)?;
        let token = self
            .base
            .tokens
            .first()
            .map(Token::join)
            .unwrap_or_default();
        if count_token(&recent, &token) == 0 {
            return fail("committed output vanished on exit (token A missing)");
        }
        Ok(Step::Pass)
    }

    /// One `fleet list --json` through the pane, into `f4.json`.
    fn needs_list(&mut self) -> Result<String, Abort> {
        let line = self.lines.needs_list.clone();
        let code = self.shell_done(&line, LIST_LINE, "talaria fleet list --json")?;
        if code != 0 {
            return Err(Abort::Failed(format!(
                "talaria fleet list --json ended with exit code {code}, expected 0"
            )));
        }
        Ok(self.file("f4.json"))
    }

    /// Whether the request is still listed, read by running the fleet list directly: talaria
    /// holds the pane, so nothing can be typed into its shell.
    fn still_listed(&self, id: &str) -> bool {
        let cfg = self.base.cfg;
        let mut attempts = 0;
        let output = loop {
            match Command::new(&cfg.talaria)
                .args(["fleet", "list", "--profile", &cfg.profile, "--json"])
                .output()
            {
                Err(error) if error.raw_os_error() == Some(26) && attempts < 20 => {
                    attempts += 1;
                    std::thread::sleep(Duration::from_millis(5));
                }
                other => break other,
            }
        };
        output.is_ok_and(|output| {
            needs_you_ids(&String::from_utf8_lossy(&output.stdout))
                .iter()
                .any(|item| item == id)
        })
    }

    /// One launch that should ask for approval, polled until the fleet lists the request.
    /// `Ok(None)` is a miss; the state is the session's last listed state.
    fn needs_you_attempt(&mut self) -> Result<Result<String, Option<String>>, Abort> {
        if let Err(abort) = self.base.wait_for_shell(&self.pane(), SHELL_LIMIT) {
            return match finish(abort)? {
                Step::Fail { reason } => Err(Abort::Failed(reason)),
                _ => Err(Abort::Failed("the shell did not become ready".to_owned())),
            };
        }
        let line = self.lines.needs_you.clone();
        let started = self.now();
        let code = self.shell(&line, LAUNCH)?;
        let launched = launch_result(&self.file("ny.out"))
            .filter(|result| code == Some(0) && result.status == "launched");
        let Some(result) = launched else {
            return Err(Abort::Failed(format!(
                "the needs-you launch ended with exit code {} or printed no single \"status\":\"launched\" line",
                code.map_or_else(|| "none".to_owned(), |code| code.to_string())
            )));
        };
        let id = result.session_id;
        self.launched_at.entry(id.clone()).or_insert(started);
        let limit = self.base.limit(LIST_POLL);
        let polled = self.now();
        loop {
            let document = self.needs_list()?;
            if needs_you_ids(&document).contains(&id) {
                return Ok(Ok(id));
            }
            let state = fleet_row_state(&document, &id);
            if self.now().saturating_sub(polled) >= limit {
                return Ok(Err(state));
            }
            self.pause(LIST_EVERY)?;
        }
    }

    /// Before the repeat: the fleet list shows no working session, or 30 s passed.
    fn wait_for_quiet_fleet(&mut self) -> Result<(), Abort> {
        let limit = self.base.limit(QUIET_FLEET);
        let started = self.now();
        while self.now().saturating_sub(started) < limit {
            let document = self.needs_list()?;
            let working = serde_json::from_str::<serde_json::Value>(document.trim())
                .ok()
                .and_then(|value| value.get("rows").and_then(|rows| rows.as_array()).cloned())
                .is_some_and(|rows| {
                    rows.iter()
                        .any(|row| row.get("state").and_then(|s| s.as_str()) == Some("working"))
                });
            if !working {
                return Ok(());
            }
            self.pause(LIST_EVERY)?;
        }
        Ok(())
    }

    /// A request that waited while talaria was closed is answered from the fleet view by
    /// denying it, and only by denying it.
    fn needs_you(&mut self) -> Result<Step, Abort> {
        let mut found = None;
        let mut state = None;
        for attempt in 0..2 {
            match self.needs_you_attempt()? {
                Ok(id) => {
                    found = Some(id);
                    break;
                }
                Err(last) => state = last,
            }
            if attempt == 0 {
                self.wait_for_quiet_fleet()?;
            }
        }
        let probe = self.root.join("probe").join(PROBE_FOLDER);
        let Some(id) = found else {
            if state.as_deref() == Some("done") && !probe.exists() {
                return Ok(Step::Skip {
                    reason:
                        "no approval card was asked for (is approvals.mode manual on the profile?)"
                            .to_owned(),
                });
            }
            return fail(format!(
                "no approval request appeared in the fleet list within {} s on two tries (session state: {})",
                self.base.limit_secs(LIST_POLL),
                state.as_deref().unwrap_or("absent")
            ));
        };
        self.waiting = true;
        let age = self
            .now()
            .saturating_sub(self.launched_at.get(&id).copied().unwrap_or_default());
        if age > TOO_OLD {
            return fail(format!(
                "the request is {} s old, too old to answer before Hermes denies it at 300 s; run again",
                age.as_secs()
            ));
        }

        // Talaria again, in the session folder: the request waited while it was closed.
        let before = self.launch_talaria()?;
        let limit = self.base.limit(NOTICE);
        let started = self.now();
        let mut noticed = false;
        loop {
            self.check()?;
            let visible = self.read(ReadSource::Visible)?;
            let recent = self.read(ReadSource::RecentUnwrapped)?;
            if let Some(code) = self.ended(before)? {
                return fail(format!(
                    "talaria exited with code {code} before showing its screen"
                ));
            }
            noticed |= visible.contains(NEEDS_YOU_NOTICE) || recent.contains(NEEDS_YOU_NOTICE);
            if noticed && needs_you_segment(&visible) == Some(1) {
                break;
            }
            if self.now().saturating_sub(started) >= limit {
                if !noticed {
                    return fail(format!(
                        "the start-up notice \"{NEEDS_YOU_NOTICE}\" did not appear within {} s",
                        self.base.limit_secs(NOTICE)
                    ));
                }
                return fail(format!(
                    "the status line did not show \"{}1{NEEDS_YOU_SEGMENT_SUFFIX}\" within {} s",
                    NEEDS_YOU_MARK,
                    self.base.limit_secs(NOTICE)
                ));
            }
            self.base.clock.sleep(POLL_INTERVAL);
        }

        let before = self.read(ReadSource::Visible)?;
        self.key(KEY_JUMP)?;
        if !self.wait_changed(PEEK, &before, |screen| {
            fleet_headers(screen)
                .iter()
                .any(|(name, _)| name == HEADER_NEEDS_YOU)
                && screen.contains(PROBE_COMMAND)
        })? {
            return fail(format!(
                "alt+a did not open the fleet view on the request (no \"{PROBE_COMMAND}\") within {} s",
                self.base.limit_secs(PEEK)
            ));
        }

        let before = self.read(ReadSource::Visible)?;
        self.typed("a")?;
        let mut digit = None;
        self.wait_changed(CARD, &before, |screen| {
            digit = deny_digit(screen);
            digit.is_some()
        })?;
        let Some(digit) = digit else {
            return fail(format!(
                "pressing a did not show a line \"<n>{DENY_SUFFIX}\" within {} s",
                self.base.limit_secs(CARD)
            ));
        };

        let card = self.read(ReadSource::Visible)?;
        self.typed(&digit.to_string())?;
        self.waiting = false;
        let mut read_at: Option<Duration> = None;
        let mut listed = true;
        let limit = self.base.limit(CLEARED);
        let started = self.now();
        loop {
            self.check()?;
            let screen = changed(&card, &self.read(ReadSource::Visible)?);
            if !screen.contains(HEADER_NEEDS_YOU) {
                let now = self.now();
                if read_at.is_none_or(|at| now.saturating_sub(at) >= DIRECT_EVERY) {
                    read_at = Some(now);
                    listed = self.still_listed(&id);
                }
                if !listed {
                    break;
                }
            }
            if self.now().saturating_sub(started) >= limit {
                return fail(format!(
                    "the request was still listed {} s after deny",
                    self.base.limit_secs(CLEARED)
                ));
            }
            self.base.clock.sleep(POLL_INTERVAL);
        }

        for _ in 0..3 {
            let before = self.read(ReadSource::Visible)?;
            self.key("esc")?;
            if !overlay_open(&changed(&before, &self.read(ReadSource::Visible)?)) {
                break;
            }
        }
        let mark = NEEDS_YOU_MARK.trim();
        if !self.wait_changed(FLAG, &card, |screen| !screen.contains(mark))? {
            return fail(format!(
                "the status line still showed \"{mark}\" after the overlay closed"
            ));
        }
        if !probe.is_dir() {
            return fail(format!(
                "the folder {PROBE_FOLDER} is gone: the command ran although it was denied"
            ));
        }
        match self.quit_talaria()? {
            Some(0) => Ok(Step::Pass),
            _ => fail(format!(
                "/quit did not end talaria with exit code 0 within {} s",
                self.base.limit_secs(QUIT)
            )),
        }
    }
}
