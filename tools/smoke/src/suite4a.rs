//! Runs the configuration screens and read-only command checks.

use crate::steps::{Abort, RECENT_LINES, REPORT_LINES, Run, Step, TabGuard, panic_message};
use crate::terminal::{PaneId, ReadSource};
use crate::{Clock, PaneCapture, SmokeConfig, SmokeReport, StepOutcome, Terminal};
use rand::{RngExt, SeedableRng, rngs::StdRng};
use std::{
    cell::RefCell,
    fs,
    path::{Path, PathBuf},
    sync::atomic::AtomicBool,
    time::Duration,
};

/// Parent directory for the isolated home used by the no-gateway check.
pub struct Phase4aConfig {
    pub empty_home_parent: PathBuf,
}

const STEPS: [&str; 17] = [
    "startup",
    "plain-answer",
    "config-browse",
    "config-invalid-input",
    "config-secret-refused",
    "providers-tabs",
    "providers-key-masked",
    "models-page",
    "models-bad-preset",
    "profiles-list",
    "profiles-bad-names",
    "doctor-overlay",
    "quit",
    "resume",
    "profiles-cli",
    "doctor-cli",
    "no-gateway",
];
const STEP_LIMITS: [u64; 17] = [
    75, 60, 30, 20, 20, 30, 30, 40, 30, 30, 20, 30, 30, 150, 30, 45, 20,
];

struct Scratch(PathBuf);
impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

/// Runs the suite and cleans up its tab and temporary directories.
pub fn run_smoke_4a(
    term: &dyn Terminal,
    clock: &dyn Clock,
    cfg: &SmokeConfig,
    cfg4a: &Phase4aConfig,
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
    if let Err(error) = fs::create_dir(&scratch) {
        report.outcomes.push(StepOutcome::Fail {
            step: "startup",
            reason: format!("the scratch folder could not be created: {error}"),
            capture: Vec::new(),
        });
        return report;
    }
    let _scratch = Scratch(scratch.clone());
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if let Err(error) = fs::set_permissions(&scratch, fs::Permissions::from_mode(0o700)) {
            report.outcomes.push(StepOutcome::Fail {
                step: "startup",
                reason: format!("the scratch folder could not be secured: {error}"),
                capture: Vec::new(),
            });
            report.elapsed = clock.now().saturating_sub(start);
            return report;
        }
    }
    let tab = match term.create_tab(&label, &scratch) {
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
    let mut run = Run {
        term,
        clock,
        cfg,
        stop,
        deadline: start + cfg.deadline,
        rng,
        tokens: Vec::new(),
        scratch,
        answered_at: None,
        fallbacks: 0,
        step_fallbacks: 0,
        fallback_steps: Vec::new(),
        interrupted: false,
        pane: tab.pane,
        second: None,
        counts: (0, 0),
    };
    let overall_deadline = run.deadline;
    let mut empty_home = None;
    for (name, limit) in STEPS.into_iter().zip(STEP_LIMITS) {
        let started = clock.now();
        run.deadline = overall_deadline.min(started + run.limit(limit));
        run.step_fallbacks = 0;
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| match name {
            "startup" => run.startup(&[]),
            "plain-answer" => run.plain_answer(),
            "quit" => {
                if let Some(token) = run.tokens.first().copied() {
                    run.tokens.push(token);
                }
                run.quit()
            }
            "resume" => resume(&mut run),
            "profiles-cli" => profiles_cli(&mut run),
            "doctor-cli" => doctor_cli(&mut run, &mut report.notes),
            "no-gateway" => no_gateway(&mut run, &cfg4a.empty_home_parent, &label, &mut empty_home),
            _ => crate::steps4a::screen_step(&mut run, name),
        }))
        .unwrap_or_else(|panic| {
            Err(Abort::Failed(format!(
                "internal error: {}",
                panic_message(&panic)
            )))
        });
        if run.step_fallbacks > 0 {
            run.fallback_steps.push(name);
        }
        run.deadline = overall_deadline;
        match result {
            Ok(Step::Pass) => report.outcomes.push(StepOutcome::Pass {
                step: name,
                elapsed: clock.now().saturating_sub(started),
            }),
            Ok(Step::Skip { reason }) if !cfg.require => report
                .outcomes
                .push(StepOutcome::Skip { step: name, reason }),
            other => {
                let reason = match other {
                    Ok(Step::Skip { reason }) => {
                        format!("skipped: {reason}; TALARIA_LIVE_REQUIRE=1")
                    }
                    Ok(Step::Fail { reason }) | Err(Abort::Failed(reason)) => reason,
                    Err(Abort::Deadline) if clock.now() < overall_deadline => match name {
                        "plain-answer" => format!(
                            "the reply token did not appear and settle within {} s",
                            run.limit_secs(60)
                        ),
                        "providers-tabs" => format!(
                            "providers did not show \"Providers · profile fixture\" within {} s",
                            run.limit_secs(30)
                        ),
                        "models-page" => format!(
                            "model roles did not show its title within {} s",
                            run.limit_secs(40)
                        ),
                        "profiles-list" => "profiles header missing".to_owned(),
                        "doctor-overlay" => format!(
                            "doctor summary line missing within {} s",
                            run.limit_secs(30)
                        ),
                        _ => format!("deadline exceeded in step {name}"),
                    },
                    Err(Abort::Deadline) => format!("deadline exceeded in step {name}"),
                    Err(Abort::Interrupted) => {
                        report.interrupted = true;
                        "interrupted by a signal".to_owned()
                    }
                    Ok(Step::Pass) => unreachable!(),
                };
                report.outcomes.push(StepOutcome::Fail {
                    step: name,
                    reason,
                    capture: capture(&run),
                });
                break;
            }
        }
    }
    let _ = guard.close();
    report.unclosed_tab = cleanup.into_inner().map(|error| error.tab);
    if let Some(home) = empty_home
        && let Err(error) = fs::remove_dir_all(home)
    {
        report.outcomes.push(StepOutcome::Fail {
            step: "no-gateway",
            reason: format!("the empty home could not be removed: {error}"),
            capture: Vec::new(),
        });
    }
    report.elapsed = clock.now().saturating_sub(start);
    report.fallbacks = run.fallbacks;
    report.fallback_steps = run.fallback_steps;
    report
}

fn capture(run: &Run<'_>) -> Vec<PaneCapture> {
    let mut panes = vec![("first", &run.pane)];
    if let Some(pane) = &run.second {
        panes.push(("second", pane));
    }
    panes
        .into_iter()
        .map(|(name, pane)| PaneCapture {
            pane: name,
            visible: run
                .term
                .read(pane, ReadSource::Visible, None)
                .unwrap_or_default(),
            recent: run
                .term
                .read(pane, ReadSource::RecentUnwrapped, Some(REPORT_LINES))
                .unwrap_or_default(),
        })
        .collect()
}

fn resume(run: &mut Run<'_>) -> Result<Step, Abort> {
    let result = run.resume(&[], &mut |_, _| Ok(Step::Pass))?;
    match result {
        Step::Fail { reason } if reason.starts_with("token counts after resume:") => {
            let Some(pane) = run.second.as_ref() else {
                return Err(Abort::Failed("resume did not create a pane".to_owned()));
            };
            let text = run
                .term
                .read(pane, ReadSource::RecentUnwrapped, Some(RECENT_LINES))
                .map_err(err)?;
            let text = crate::screen::without_resume_header(&text);
            let token = run
                .tokens
                .first()
                .map(crate::screen::Token::join)
                .unwrap_or_default();
            let count = crate::screen::count_token(&text, &token);
            Ok(Step::Fail {
                reason: format!(
                    "token count after resume: A {count} (first pane {}); it must be at least 1 and at most the first pane's count",
                    run.counts.0
                ),
            })
        }
        Step::Fail { reason } if reason.starts_with("the reply to the third prompt") => {
            Ok(Step::Fail {
                reason: reason.replace(
                    "the reply to the third prompt",
                    "the reply to the token C prompt",
                ),
            })
        }
        other => Ok(other),
    }
}

fn err(error: impl std::fmt::Display) -> Abort {
    Abort::Failed(error.to_string())
}
fn fail(reason: impl Into<String>) -> Result<Step, Abort> {
    Ok(Step::Fail {
        reason: reason.into(),
    })
}
fn quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

// Capture only text appended by this command, and wait for its own completion marker.
fn command(run: &mut Run<'_>, line: &str, until: Duration) -> Result<String, Abort> {
    run.check_stop()?;
    if run.clock.now() >= run.deadline {
        return Err(Abort::Deadline);
    }
    if run.clock.now() >= until {
        return Err(Abort::Failed(
            "the command step did not finish within its time limit".into(),
        ));
    }
    let marker = format!("SMOKE_BEGIN_{:016x}", run.rng.random::<u64>());
    let read = |pane: &PaneId| {
        run.term
            .read(pane, ReadSource::RecentUnwrapped, Some(RECENT_LINES))
            .map(|text| crate::screen4a::plain(&text))
            .map_err(err)
    };
    let before = read(&run.pane)?;
    let wrapped = format!("printf '\\n{marker}\\n'; {line}");
    run.term.run(&run.pane, &wrapped).map_err(err)?;
    loop {
        run.check_stop()?;
        if run.clock.now() >= run.deadline {
            return Err(Abort::Deadline);
        }
        if run.clock.now() >= until {
            return Err(Abort::Failed(
                "the command step did not finish within its time limit".into(),
            ));
        }
        let text = read(&run.pane)?;
        let Some(delta) = appended(&before, &text) else {
            return Err(Abort::Failed(
                "command output could not be isolated from earlier output".into(),
            ));
        };
        let output = if let Some(index) = delta.lines().position(|row| row.trim() == marker) {
            delta.lines().skip(index + 1).collect::<Vec<_>>().join("\n")
        } else if delta.contains(&marker) {
            run.clock.sleep(crate::screen::POLL_INTERVAL);
            continue;
        } else {
            // Synthetic terminals do not echo the submitted shell command.
            delta.to_owned()
        };
        if let Some(end) = output.lines().position(|line| marker_exit(line).is_some()) {
            return Ok(output
                .lines()
                .take(end + 1)
                .filter(|row| {
                    !row.contains("SMOKE_EXIT:$?") && !row.trim().is_empty() && row.trim() != "$"
                })
                .collect::<Vec<_>>()
                .join("\n"));
        }
        run.clock.sleep(crate::screen::POLL_INTERVAL);
    }
}
fn appended<'a>(before: &str, after: &'a str) -> Option<&'a str> {
    if before.is_empty() {
        return Some(after);
    }
    if let Some(delta) = after.strip_prefix(before) {
        return Some(delta);
    }
    // A bounded terminal capture can discard whole lines from the front.
    // Accept an overlap only at a line boundary, and never fall back to old history.
    before
        .match_indices('\n')
        .filter_map(|(index, _)| {
            let suffix = &before[index + 1..];
            (!suffix.is_empty())
                .then(|| after.strip_prefix(suffix))
                .flatten()
        })
        .next()
}
fn binary(run: &Run<'_>) -> String {
    quote(&run.cfg.talaria.to_string_lossy())
}
fn exit(text: &str) -> Option<i32> {
    text.lines()
        .find_map(|line| line.trim().strip_prefix("exit=")?.parse().ok())
}
fn marker_exit(text: &str) -> Option<i32> {
    text.lines()
        .find_map(|line| line.trim().strip_prefix("SMOKE_EXIT:")?.parse().ok())
}

fn profiles_cli(run: &mut Run<'_>) -> Result<Step, Abort> {
    let until = run.clock.now() + run.limit(30);
    let bin = binary(run);
    let text = command(
        run,
        &format!("env -u HERMES_HOME {bin} profiles fixture default; echo SMOKE_EXIT:$?"),
        until,
    )?;
    if !text.lines().any(|line| {
        line.trim() == "Profiles"
            || line.split_whitespace().collect::<Vec<_>>().join(" ") == "NAME STATE MODEL NEXT STEP"
    }) {
        return fail("profiles command printed no header");
    }
    for name in ["fixture", "default"] {
        if !text
            .lines()
            .any(|line| line.split_whitespace().next() == Some(name))
        {
            return fail(format!("profiles command printed no row for {name}"));
        }
    }
    let json = command(
        run,
        &format!("env -u HERMES_HOME {bin} profiles --json fixture | jq -c .; echo SMOKE_EXIT:$?"),
        until,
    )?;
    if marker_exit(&json) != Some(0)
        || !json
            .lines()
            .any(|line| serde_json::from_str::<serde_json::Value>(line.trim()).is_ok())
    {
        return fail("profiles --json is not valid JSON");
    }
    let text = command(
        run,
        &format!(
            "env -u HERMES_HOME {bin} profiles nosuchprofile; echo exit=$?; echo SMOKE_EXIT:$?"
        ),
        until,
    )?;
    if !text.contains("talaria profiles: no profile named nosuchprofile") {
        return fail("the unknown-profile text is missing");
    }
    if exit(&text) != Some(2) {
        return fail("unknown profile did not exit 2");
    }
    Ok(Step::Pass)
}

fn doctor_cli(run: &mut Run<'_>, notes: &mut Vec<(&'static str, String)>) -> Result<Step, Abort> {
    let until = run.clock.now() + run.limit(45);
    let bin = binary(run);
    let text = command(
        run,
        &format!(
            "env -u HERMES_HOME {bin} doctor --profile fixture; echo exit=$?; echo SMOKE_EXIT:$?"
        ),
        until,
    )?;
    let rows: Vec<_> = text
        .lines()
        .filter_map(|line| {
            let line = line.trim();
            ["ok ", "warn ", "FAIL ", "skip "]
                .iter()
                .find_map(|prefix| line.strip_prefix(*prefix).map(|name| (*prefix, name)))
        })
        .collect();
    if rows.len() != 7 {
        return fail(format!("doctor printed {} rows, not 7", rows.len()));
    }
    if let Some((_, name)) = rows
        .iter()
        .find(|(status, _)| !matches!(*status, "ok " | "warn "))
    {
        return fail(format!("doctor row {name} is not ok or warn"));
    }
    if !text
        .lines()
        .any(|line| line.trim_start().starts_with("Result:"))
    {
        return fail("doctor printed no Result: line");
    }
    if exit(&text) != Some(0) {
        return fail(format!(
            "doctor exit was {}, not 0",
            exit(&text).map_or_else(|| "unknown".into(), |v| v.to_string())
        ));
    }
    let home = command(run, "echo $HOME; echo SMOKE_EXIT:$?", until)?;
    let home = home
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty() && !line.starts_with("SMOKE_EXIT:") && *line != "$")
        .unwrap_or("");
    if home.len() < 4 {
        notes.push((
            "doctor-cli",
            "home-directory check skipped because the home path is shorter than four characters"
                .into(),
        ));
    } else if text.contains(home) {
        return fail("doctor output contains the home directory");
    }
    let json = command(
        run,
        &format!(
            "env -u HERMES_HOME {bin} doctor --json --profile fixture | jq -c .; echo SMOKE_EXIT:$?"
        ),
        until,
    )?;
    if marker_exit(&json) != Some(0)
        || !json.lines().any(|line| {
            crate::screen4a::doctor_json(line.trim()) == Some(("talaria-doctor-v1".to_owned(), 7))
        })
    {
        return fail("doctor --json format is not talaria-doctor-v1 with 7 checks");
    }
    let unknown = command(
        run,
        &format!(
            "env -u HERMES_HOME {bin} doctor --profile nosuchprofile; echo exit=$?; echo SMOKE_EXIT:$?"
        ),
        until,
    )?;
    let normalized = unknown
        .lines()
        .map(|line| line.split_whitespace().collect::<Vec<_>>().join(" "))
        .collect::<Vec<_>>()
        .join("\n");
    if !normalized.contains("FAIL Profiles")
        || !unknown.contains("the gateway never discovered a profile named nosuchprofile")
        || !normalized.contains("skip Attach")
        || exit(&unknown) != Some(1)
    {
        return fail("unknown profile did not give FAIL Profiles and exit 1");
    }
    let blank = command(
        run,
        &format!("env -u HERMES_HOME {bin} doctor --profile ''; echo exit=$?; echo SMOKE_EXIT:$?"),
        until,
    )?;
    if exit(&blank) != Some(2)
        || !(blank.to_ascii_lowercase().contains("usage")
            || blank.contains(crate::wording4a::DOCTOR_ARGUMENT_ERROR))
    {
        return fail("blank profile did not exit 2");
    }
    Ok(Step::Pass)
}

fn no_gateway(
    run: &mut Run<'_>,
    parent: &Path,
    label: &str,
    created: &mut Option<PathBuf>,
) -> Result<Step, Abort> {
    let until = run.clock.now() + run.limit(20);
    let home = parent.join(format!("{label}-empty-home"));
    fs::create_dir(&home).map_err(err)?;
    *created = Some(home.clone());
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&home, fs::Permissions::from_mode(0o700)).map_err(err)?;
    }
    let home = quote(&home.to_string_lossy());
    let bin = binary(run);
    let doctor = command(
        run,
        &format!("HERMES_HOME={home} {bin} doctor; echo exit=$?; echo SMOKE_EXIT:$?"),
        until,
    )?;
    let lines: Vec<_> = doctor
        .lines()
        .map(|line| line.split_whitespace().collect::<Vec<_>>().join(" "))
        .collect();
    if !lines.windows(2).any(|lines| lines[0].starts_with("FAIL Gateway") && lines[1] == "Fix: Run `hermes gateway ensure --json` to start the Hermes gateway, then try again.") || exit(&doctor) != Some(1) { return fail("doctor without a gateway did not fail with the documented fix"); }
    let profiles = command(
        run,
        &format!("HERMES_HOME={home} {bin} profiles; echo exit=$?; echo SMOKE_EXIT:$?"),
        until,
    )?;
    if !profiles.contains("no Hermes gateway is running for this home")
        || !profiles.contains("Next step:")
        || exit(&profiles) != Some(1)
    {
        return fail("profiles without a gateway did not exit 1 with a next step");
    }
    let listing = command(run, &format!("ls -A {home}; echo SMOKE_EXIT:$?"), until)?;
    let count = listing
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with("SMOKE_EXIT:") && *line != "$")
        .count();
    if count != 0 {
        return fail(format!("the empty home is not empty ({count} entries)"));
    }
    Ok(Step::Pass)
}
