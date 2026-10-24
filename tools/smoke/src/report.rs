//! Step outcomes, the result line and the failure report.

use std::time::Duration;

use crate::terminal::TabId;

pub struct PaneCapture {
    /// `first` or `second`.
    pub pane: &'static str,
    pub visible: String,
    pub recent: String,
}

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

pub struct SmokeReport {
    pub outcomes: Vec<StepOutcome>,
    pub version: String,
    pub elapsed: Duration,
    pub fallbacks: u32,
    pub interrupted: bool,
    pub unclosed_tab: Option<TabId>,
}

impl SmokeReport {
    pub fn exit_code(&self) -> i32 {
        todo!("smoke")
    }

    pub fn result_line(&self) -> String {
        todo!("smoke")
    }

    pub fn step_lines(&self) -> String {
        todo!("smoke")
    }
}
