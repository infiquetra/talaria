//! Step outcomes, the result line and the failure report.

use std::path::PathBuf;
use std::time::Duration;

use crate::terminal::TabId;

/// The screen of one pane when a step failed.
pub struct PaneCapture {
    /// `first` or `second`.
    pub pane: &'static str,
    pub visible: String,
    pub recent: String,
}

/// What one step ended with.
pub enum StepOutcome {
    Pass {
        step: &'static str,
        elapsed: Duration,
    },
    Skip {
        step: &'static str,
        reason: String,
    },
    Fail {
        step: &'static str,
        reason: String,
        capture: Vec<PaneCapture>,
    },
}

/// What the whole run ended with.
pub struct SmokeReport {
    pub outcomes: Vec<StepOutcome>,
    pub version: String,
    pub elapsed: Duration,
    pub fallbacks: u32,
    pub interrupted: bool,
    pub unclosed_tab: Option<TabId>,
    /// The tab the run opened.
    pub tab: Option<TabId>,
    /// The scratch project the run used.
    pub scratch: PathBuf,
    /// The steps in which a submit fallback was sent.
    pub fallback_steps: Vec<&'static str>,
}

impl SmokeReport {
    /// The exit code of the run.
    pub fn exit_code(&self) -> i32 {
        if self.unclosed_tab.is_some() || self.interrupted {
            return 1;
        }
        if self.outcomes.iter().any(Self::failed) {
            return 1;
        }
        if self.outcomes.iter().any(Self::skipped) {
            return 3;
        }
        0
    }

    /// The one result line of the run.
    pub fn result_line(&self) -> String {
        if self.unclosed_tab.is_some() {
            return "SMOKE_RESULT: FAILED (cleanup)".to_owned();
        }
        if self.interrupted {
            return "SMOKE_RESULT: FAILED (interrupted)".to_owned();
        }
        if let Some(step) = self.outcomes.iter().find_map(|outcome| match outcome {
            StepOutcome::Fail { step, .. } => Some(*step),
            _ => None,
        }) {
            return format!("SMOKE_RESULT: FAILED ({step})");
        }
        let skipped: Vec<&str> = self
            .outcomes
            .iter()
            .filter_map(|outcome| match outcome {
                StepOutcome::Skip { step, .. } => Some(*step),
                _ => None,
            })
            .collect();
        if !skipped.is_empty() {
            return format!("SMOKE_RESULT: PASSED WITH SKIPS ({})", skipped.join(", "));
        }
        format!(
            "SMOKE_RESULT: PASSED (elapsed {} s)",
            self.elapsed.as_secs()
        )
    }

    /// The one line per step, in order, with the note of every submit fallback.
    pub fn step_lines(&self) -> String {
        let mut lines: Vec<String> = Vec::new();
        for outcome in &self.outcomes {
            let step = match outcome {
                StepOutcome::Pass { step, elapsed } => {
                    lines.push(format!("PASS {step} {:.1} s", elapsed.as_secs_f64()));
                    step
                }
                StepOutcome::Skip { step, reason } => {
                    lines.push(format!("SKIP {step} ({reason})"));
                    step
                }
                StepOutcome::Fail { step, reason, .. } => {
                    lines.push(format!("FAIL {step} ({reason})"));
                    step
                }
            };
            if self.fallback_steps.contains(step) {
                lines.push(format!("NOTE submit fallback: sent Enter (step {step})"));
            }
        }
        lines.join("\n")
    }

    fn failed(outcome: &StepOutcome) -> bool {
        matches!(outcome, StepOutcome::Fail { .. })
    }

    fn skipped(outcome: &StepOutcome) -> bool {
        matches!(outcome, StepOutcome::Skip { .. })
    }
}
