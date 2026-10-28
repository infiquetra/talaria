//! Runs the workflow screens and offline workflow commands.

use crate::steps::{Abort, REPORT_LINES, Run, Step, TabGuard, panic_message};
use crate::terminal::ReadSource;
use crate::{Clock, PaneCapture, SmokeConfig, SmokeReport, StepOutcome, Terminal};
use rand::{RngExt, SeedableRng, rngs::StdRng};
use std::{cell::RefCell, fs, path::PathBuf, sync::atomic::AtomicBool, time::Duration};

/// Expected profile, model and provider for the workflow suite.
pub struct Phase5Config {
    pub profile: String,
    pub model: String,
    pub provider: String,
}
impl Default for Phase5Config {
    fn default() -> Self {
        Self {
            profile: "fixture".into(),
            model: "minimax-m3.1-flash".into(),
            provider: "ollama-cloud".into(),
        }
    }
}
const STEPS: [&str; 10] = [
    "startup",
    "plain-answer",
    "workflows-empty",
    "ultracode-status",
    "quit",
    "resume",
    "builtins-cli",
    "roster-cli",
    "list-cli",
    "no-gateway",
];
const STEP_LIMITS: [u64; 10] = [30, 60, 30, 30, 20, 90, 40, 40, 30, 40];

struct Scratch(PathBuf);
impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

/// Runs the suite and cleans up its tab and temporary directories.
pub fn run_smoke_5(
    term: &dyn Terminal,
    clock: &dyn Clock,
    cfg: &SmokeConfig,
    cfg5: &Phase5Config,
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
    for (name, limit) in STEPS.into_iter().zip(STEP_LIMITS) {
        let started = clock.now();
        // Paste guards and quiet periods keep their real duration under a smaller scale.
        let fixed_allowance =
            Duration::from_secs_f32(10.0 * (1.0 - cfg.step_scale).clamp(0.0, 1.0));
        run.deadline = overall_deadline.min(started + run.limit(limit) + fixed_allowance);
        run.step_fallbacks = 0;
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            crate::steps5::step(&mut run, name, cfg5)
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
                    Err(Abort::Deadline) => "deadline reached".to_owned(),
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
