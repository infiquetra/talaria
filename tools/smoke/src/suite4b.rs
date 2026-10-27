//! Runs the capabilities and operations screen checks.

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
pub struct Phase4bConfig {
    pub empty_home_parent: PathBuf,
}

const STEPS: [&str; 14] = [
    "startup",
    "plain-answer",
    "mcp-browse",
    "capabilities-browse",
    "memory-open",
    "memory-commands",
    "cron-browse",
    "messaging-browse",
    "connectors-consent",
    "configure-secret-refused",
    "quit",
    "resume",
    "capabilities-cli",
    "no-gateway",
];
const STEP_LIMITS: [u64; 14] = [75, 60, 30, 40, 30, 40, 30, 40, 20, 20, 30, 150, 45, 20];

struct Scratch(PathBuf);
impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

/// Runs the suite and cleans up its tab and temporary directories.
pub fn run_smoke_4b(
    term: &dyn Terminal,
    clock: &dyn Clock,
    cfg: &SmokeConfig,
    cfg4b: &Phase4bConfig,
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
        // Fixed waits remain unscaled, including the two ten-second command waits.
        let fixed_allowance =
            Duration::from_secs_f32(25.0 * (1.0 - cfg.step_scale).clamp(0.0, 1.0));
        run.deadline = overall_deadline.min(started + run.limit(limit) + fixed_allowance);
        run.step_fallbacks = 0;
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| match name {
            "startup" => startup(&mut run),
            "plain-answer" => plain_answer(&mut run),
            "quit" => quit(&mut run),
            "resume" => resume(&mut run),
            "capabilities-cli" => capabilities_cli(&mut run, &mut report.notes),
            "no-gateway" => no_gateway(&mut run, &cfg4b.empty_home_parent, &label, &mut empty_home),
            _ => crate::steps4b::screen_step(&mut run, name),
        }))
        .unwrap_or_else(|panic| {
            Err(Abort::Failed(format!(
                "internal error: {}",
                panic_message(&panic)
            )))
        });
        let result =
            if clock.now() >= overall_deadline && !matches!(result, Err(Abort::Interrupted)) {
                Err(Abort::Deadline)
            } else {
                result
            };
        if run.step_fallbacks > 0 {
            run.fallback_steps.push(name);
        }
        run.deadline = overall_deadline;
        match result {
            Ok(Step::Pass) => report.outcomes.push(StepOutcome::Pass {
                step: name,
                elapsed: clock.now().saturating_sub(started),
            }),
            other => {
                let reason = match other {
                    Ok(Step::Skip { reason }) => {
                        format!("skipped: {reason}; TALARIA_LIVE_REQUIRE=1")
                    }
                    Ok(Step::Fail { reason }) | Err(Abort::Failed(reason)) => reason,
                    Err(Abort::Deadline)
                        if name == "plain-answer" && clock.now() < overall_deadline =>
                    {
                        format!(
                            "the reply token did not appear and settle within {} s",
                            run.limit_secs(60)
                        )
                    }
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
        let output = normalize_completion(&output);
        if let Some(output) = completed_output(&output) {
            return Ok(output);
        }
        run.clock.sleep(crate::screen::POLL_INTERVAL);
    }
}
fn normalize_completion(text: &str) -> String {
    let mut normalized = String::new();
    for line in text.lines() {
        if let Some((prefix, suffix)) = line.rsplit_once("SMOKE_EXIT:")
            && suffix.trim().parse::<i32>().is_ok()
            && serde_json::from_str::<serde_json::Value>(prefix.trim())
                .is_ok_and(|v| v.is_object() || v.is_array())
        {
            normalized.push_str(prefix);
            normalized.push('\n');
            normalized.push_str("SMOKE_EXIT:");
            normalized.push_str(suffix);
        } else {
            normalized.push_str(line);
        }
        normalized.push('\n');
    }
    normalized
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
fn completed_output(text: &str) -> Option<String> {
    let end = text.lines().position(|line| marker_exit(line).is_some())?;
    Some(
        text.lines()
            .take(end + 1)
            .filter(|row| {
                !row.contains("SMOKE_EXIT:$?") && !row.trim().is_empty() && row.trim() != "$"
            })
            .collect::<Vec<_>>()
            .join("\n"),
    )
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

fn timed_limit(run: &Run<'_>, seconds: u64, quiet: u64) -> Duration {
    run.limit(seconds)
        .max(Duration::from_secs(quiet) + crate::screen::POLL_INTERVAL)
}

fn shell(run: &mut Run<'_>, pane: &PaneId) -> Result<(), Abort> {
    let started = run.clock.now();
    let mut ready = None;
    loop {
        run.check_stop()?;
        let now = run.clock.now();
        if now >= run.deadline {
            return Err(Abort::Deadline);
        }
        let foreground = run.term.foreground(pane).map_err(err)?;
        let screen = run
            .term
            .read(pane, ReadSource::Visible, None)
            .map_err(err)?;
        if foreground.len() == 1 && !screen.trim().is_empty() {
            let since = *ready.get_or_insert(now);
            if now.saturating_sub(since) >= Duration::from_secs(1) {
                return Ok(());
            }
        } else {
            ready = None;
        }
        if now.saturating_sub(started) >= timed_limit(run, 45, 1) {
            return Err(Abort::Failed(format!(
                "the shell did not become ready within {} s",
                run.limit_secs(45)
            )));
        }
        run.clock.sleep(crate::screen::POLL_INTERVAL);
    }
}

fn stable_screen(run: &mut Run<'_>, pane: &PaneId, resumed: bool) -> Result<(), Abort> {
    let started = run.clock.now();
    let mut watch = crate::steps::Watch::new(started);
    let (quiet, prefix, stable) = if resumed {
        (2, "resumed talaria", "resumed talaria")
    } else {
        (1, "talaria", "startup")
    };
    loop {
        run.check_stop()?;
        let now = run.clock.now();
        if now >= run.deadline {
            return Err(Abort::Deadline);
        }
        let visible = run.watch_visible(pane, &mut watch)?;
        if let Some(code) = crate::screen::exit_code(&visible) {
            return Err(Abort::Failed(format!(
                "{prefix} exited with code {code} before showing its screen"
            )));
        }
        let foreground = run.term.foreground(pane).map_err(err)?;
        if foreground
            .iter()
            .any(|entry| entry.contains(&run.cfg.talaria.to_string_lossy().to_string()))
            && now.saturating_sub(watch.changed_at) >= Duration::from_secs(quiet)
        {
            return Ok(());
        }
        if now.saturating_sub(started) >= timed_limit(run, 30, quiet) {
            return Err(Abort::Failed(format!(
                "{stable} did not show a stable screen within {} s",
                run.limit_secs(30)
            )));
        }
        run.clock.sleep(crate::screen::POLL_INTERVAL);
    }
}

fn startup(run: &mut Run<'_>) -> Result<Step, Abort> {
    let pane = run.pane.clone();
    shell(run, &pane)?;
    let line = crate::config::launch_line(
        &run.cfg.talaria,
        &[
            "--no-ensure",
            "--profile",
            &run.cfg.profile,
            "--model",
            &run.cfg.model,
            "--provider",
            &run.cfg.provider,
        ],
    );
    run.term.run(&pane, &line).map_err(err)?;
    stable_screen(run, &pane, false)?;
    Ok(Step::Pass)
}

fn reply(
    run: &mut Run<'_>,
    pane: &PaneId,
    token: &crate::screen::Token,
    reason: &str,
) -> Result<Step, Abort> {
    let started = run.clock.now();
    let mut last_count = 0;
    let mut since = started;
    let mut fallback = false;
    loop {
        run.check_stop()?;
        let now = run.clock.now();
        if now >= run.deadline {
            return Err(Abort::Deadline);
        }
        let recent = run
            .term
            .read(pane, ReadSource::RecentUnwrapped, Some(RECENT_LINES))
            .map_err(err)?;
        let visible = run
            .term
            .read(pane, ReadSource::Visible, None)
            .map_err(err)?;
        let count = crate::screen::count_token(&recent, &token.join());
        if count != last_count {
            last_count = count;
            since = now;
        }
        if count > 0 && now.saturating_sub(since) >= Duration::from_secs(3) {
            return Ok(Step::Pass);
        }
        if !fallback
            && count == 0
            && now.saturating_sub(started) >= Duration::from_secs(3)
            && now.saturating_sub(started) <= Duration::from_secs(5)
            && crate::steps4b::safe_fallback(&visible, &token.prompt())
        {
            run.term.send_keys(pane, &["enter"]).map_err(err)?;
            fallback = true;
            run.fallbacks += 1;
            run.step_fallbacks += 1;
        }
        if now.saturating_sub(started) >= timed_limit(run, 60, 3) {
            return fail(format!("{reason} {} s", run.limit_secs(60)));
        }
        run.clock.sleep(crate::screen::POLL_INTERVAL);
    }
}
fn plain_answer(run: &mut Run<'_>) -> Result<Step, Abort> {
    let token = run.token();
    let pane = run.pane.clone();
    crate::steps4b::submit(run, &pane, &token.prompt())?;
    reply(
        run,
        &pane,
        &token,
        "the reply token did not appear and settle within",
    )
}

fn quit_pane(run: &mut Run<'_>, pane: &PaneId, resumed: bool) -> Result<Step, Abort> {
    crate::steps4b::submit(run, pane, "/quit")?;
    let started = run.clock.now();
    let mut fallback = false;
    loop {
        run.check_stop()?;
        let now = run.clock.now();
        if now >= run.deadline {
            return Err(Abort::Deadline);
        }
        let visible = run
            .term
            .read(pane, ReadSource::Visible, None)
            .map_err(err)?;
        if let Some(code) = crate::screen::exit_code(&visible) {
            if resumed {
                if code == 0 {
                    return Ok(Step::Pass);
                }
                return fail(format!(
                    "/quit in the resumed pane did not end talaria with exit code 0 within {} s",
                    run.limit_secs(15)
                ));
            }
            if code != 0 {
                return fail(format!(
                    "/quit ended talaria with exit code {code}, expected 0"
                ));
            }
            let foreground = run.term.foreground(pane).map_err(err)?;
            if foreground.len() != 1
                || foreground[0].contains(&run.cfg.talaria.to_string_lossy().to_string())
            {
                return fail("the shell did not return to the foreground after /quit");
            }
            let recent = run
                .term
                .read(pane, ReadSource::RecentUnwrapped, Some(RECENT_LINES))
                .map_err(err)?;
            let token = run
                .tokens
                .first()
                .map(crate::screen::Token::join)
                .unwrap_or_default();
            let count = crate::screen::count_token(&recent, &token);
            if count == 0 {
                return fail("committed output vanished on exit (token A missing)");
            }
            run.counts.0 = count;
            return Ok(Step::Pass);
        }
        if !fallback
            && now.saturating_sub(started) >= Duration::from_secs(3)
            && now.saturating_sub(started) <= Duration::from_secs(5)
            && crate::steps4b::safe_fallback(&visible, "/quit")
        {
            run.term.send_keys(pane, &["enter"]).map_err(err)?;
            fallback = true;
            run.fallbacks += 1;
            run.step_fallbacks += 1;
        }
        if now.saturating_sub(started) >= run.limit(15) {
            return if resumed {
                fail(format!(
                    "/quit in the resumed pane did not end talaria with exit code 0 within {} s",
                    run.limit_secs(15)
                ))
            } else {
                fail(format!(
                    "/quit did not end talaria within {} s",
                    run.limit_secs(15)
                ))
            };
        }
        run.clock.sleep(crate::screen::POLL_INTERVAL);
    }
}
fn quit(run: &mut Run<'_>) -> Result<Step, Abort> {
    let pane = run.pane.clone();
    quit_pane(run, &pane, false)
}
fn resume(run: &mut Run<'_>) -> Result<Step, Abort> {
    let pane = run.term.split_pane(&run.pane, &run.scratch).map_err(err)?;
    run.second = Some(pane.clone());
    shell(run, &pane)?;
    let line = crate::config::launch_line(
        &run.cfg.talaria,
        &["--no-ensure", "-c", "--profile", &run.cfg.profile],
    );
    run.term.run(&pane, &line).map_err(err)?;
    stable_screen(run, &pane, true)?;
    let recent = run
        .term
        .read(&pane, ReadSource::RecentUnwrapped, Some(RECENT_LINES))
        .map_err(err)?;
    let recent = crate::screen::without_resume_header(&recent);
    let token = run
        .tokens
        .first()
        .map(crate::screen::Token::join)
        .unwrap_or_default();
    let count = crate::screen::count_token(&recent, &token);
    if count == 0 || count > run.counts.0 {
        return fail(format!(
            "token count after resume: A {count} (first pane {}); it must be at least 1 and at most the first pane's count",
            run.counts.0
        ));
    }
    let token = run.token();
    crate::steps4b::submit(run, &pane, &token.prompt())?;
    match reply(
        run,
        &pane,
        &token,
        "the reply to the token C prompt did not appear and stay stable within",
    )? {
        Step::Pass => quit_pane(run, &pane, true),
        other => Ok(other),
    }
}

fn forbidden_key(value: &serde_json::Value) -> bool {
    match value {
        serde_json::Value::Object(fields) => fields.iter().any(|(key, value)| {
            matches!(key.as_str(), "path" | "install_dir" | "after_install_path")
                || forbidden_key(value)
        }),
        serde_json::Value::Array(items) => items.iter().any(forbidden_key),
        _ => false,
    }
}
fn contains_home(value: &serde_json::Value, home: &str) -> bool {
    match value {
        serde_json::Value::String(text) => text.contains(home),
        serde_json::Value::Object(fields) => fields
            .iter()
            .any(|(key, value)| key.contains(home) || contains_home(value, home)),
        serde_json::Value::Array(items) => items.iter().any(|value| contains_home(value, home)),
        _ => false,
    }
}

fn capabilities_cli(
    run: &mut Run<'_>,
    notes: &mut Vec<(&'static str, String)>,
) -> Result<Step, Abort> {
    let until = run.deadline;
    let bin = binary(run);
    let output = command(
        run,
        &format!(
            "env -u HERMES_HOME {bin} capabilities skills --profile fixture; smoke_status=$?; printf '\\nexit=%s\\nSMOKE_EXIT:%s\\n' \"$smoke_status\" \"$smoke_status\""
        ),
        until,
    )?;
    if !output.lines().any(|line| {
        line.split_whitespace()
            .eq(crate::wording4b::CAPABILITIES_CLI_HEADER.split_whitespace())
    }) {
        return fail("capabilities skills printed no header");
    }
    if exit(&output) != Some(0) {
        return fail(format!(
            "capabilities skills exit was {}, not 0",
            exit(&output).map_or_else(|| "unknown".into(), |v| v.to_string())
        ));
    }
    let home_output = command(run, "echo $HOME; echo SMOKE_EXIT:$?", until)?;
    let home = home_output
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty() && !line.starts_with("SMOKE_EXIT:") && *line != "$")
        .unwrap_or("")
        .to_owned();
    if home.chars().count() < 4 {
        notes.push((
            "capabilities-cli",
            "home-directory check skipped because the home path is shorter than four characters"
                .into(),
        ));
    }
    let mut values = Vec::new();
    let mut profiles_confirmed = true;
    for list in ["toolsets", "plugins"] {
        let line = format!(
            "data=$(env -u HERMES_HOME {bin} capabilities {list} --json --profile fixture); printf '%s\\n' \"$data\" | jq -c .; printf '%s\\n' \"$data\" | jq -e '.profile == \"fixture\"'; echo SMOKE_EXIT:$?"
        );
        let text = command(run, &line, until)?;
        profiles_confirmed &= text.lines().any(|line| line.trim() == "true");
        let value = text.lines().find_map(|line| {
            serde_json::from_str::<serde_json::Value>(line.trim())
                .ok()
                .filter(|v| v.is_object() || v.is_array())
        });
        let Some(value) = value else {
            return fail("capabilities --json is not valid JSON");
        };
        if marker_exit(&text) != Some(0)
            && value.get("profile").and_then(|v| v.as_str()) == Some("fixture")
        {
            return fail("capabilities --json is not valid JSON");
        }
        values.push(value);
    }
    if !profiles_confirmed
        || values
            .iter()
            .any(|value| value.get("profile").and_then(|v| v.as_str()) != Some("fixture"))
    {
        return fail("capabilities --json .profile is not fixture");
    }
    if values.iter().any(forbidden_key) {
        return fail("capabilities --json holds a forbidden key");
    }
    if home.chars().count() >= 4 && values.iter().any(|value| contains_home(value, &home)) {
        return fail("capabilities --json holds the home directory");
    }
    Ok(Step::Pass)
}
fn no_gateway(
    run: &mut Run<'_>,
    parent: &Path,
    label: &str,
    created: &mut Option<PathBuf>,
) -> Result<Step, Abort> {
    let home = parent.join(format!("{label}-empty-home"));
    fs::create_dir(&home).map_err(err)?;
    *created = Some(home.clone());
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&home, fs::Permissions::from_mode(0o700)).map_err(err)?;
    }
    let until = run.deadline;
    let home = quote(&home.to_string_lossy());
    let bin = binary(run);
    let output = command(
        run,
        &format!(
            "HERMES_HOME={home} {bin} capabilities skills; smoke_status=$?; printf '\\nexit=%s\\nSMOKE_EXIT:%s\\n' \"$smoke_status\" \"$smoke_status\""
        ),
        until,
    )?;
    if !output.contains(crate::wording4b::NO_GATEWAY_FMT)
        || !output.contains(crate::wording4b::NEXT_STEP)
        || exit(&output) != Some(1)
    {
        return fail("capabilities without a gateway did not fail with a next step and exit 1");
    }
    let output = command(
        run,
        &format!(
            "HERMES_HOME={home} {bin} capabilities nothing; smoke_status=$?; printf '\\nexit=%s\\nSMOKE_EXIT:%s\\n' \"$smoke_status\" \"$smoke_status\""
        ),
        until,
    )?;
    if !output.contains(&crate::wording4b::CAPABILITIES_UNKNOWN_FMT.replace("<name>", "nothing"))
        || exit(&output) != Some(2)
    {
        return fail("the unknown list text or exit 2 is missing");
    }
    let output = command(run, &format!("ls -A {home}; echo SMOKE_EXIT:$?"), until)?;
    let count = output
        .lines()
        .filter(|line| {
            !line.trim().is_empty() && !line.trim().starts_with("SMOKE_EXIT:") && line.trim() != "$"
        })
        .count();
    if count != 0 {
        return fail(format!("the empty home is not empty ({count} entries)"));
    }
    Ok(Step::Pass)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn completed_commands_exclude_the_following_shell_prompt() {
        assert_eq!(
            completed_output("answer\nexit=1\nSMOKE_EXIT:1\nfixture-shell$"),
            Some("answer\nexit=1\nSMOKE_EXIT:1".into())
        );
        assert_eq!(completed_output("SMOKE_EXIT:$?\nstill running"), None);
        assert_eq!(
            completed_output("SMOKE_EXIT:0\nfixture-shell$"),
            Some("SMOKE_EXIT:0".into())
        );
    }

    #[test]
    fn json_safety_checks_nested_keys_and_values() {
        let home = "fixture-home-marker";
        assert!(contains_home(
            &json!({"nested": {"fixture-home-marker": []}}),
            home
        ));
        assert!(contains_home(&json!({"nested": [home]}), home));
        assert!(!contains_home(&json!({"nested": ["public"]}), home));
        assert!(forbidden_key(
            &json!({"nested": [{"install_dir": "hidden"}]})
        ));
        assert!(!forbidden_key(&json!({"nested": [{"pathname": "public"}]})));
    }
}
