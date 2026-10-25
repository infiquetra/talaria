//! Cleaning untrusted text and wording shell and side outcomes.

use crate::side::{ShellOutcome, SideOutcome};

/// Strip escape sequences, resolve carriage returns, drop trailing blanks.
pub fn clean_output(text: &str) -> String {
    let _ = text;
    todo!("#225")
}

/// The refusal line for a shell reason, cut to 64 code points.
pub fn shell_refusal_text(reason: &str, detail: Option<&str>) -> String {
    let _ = (reason, detail);
    todo!("#225")
}

/// The refusal line for a side reason, cut to 64 code points.
pub fn side_refusal_text(reason: &str, detail: Option<&str>) -> String {
    let _ = (reason, detail);
    todo!("#225")
}

/// The cell text for a shell outcome. `None` means the streams render.
pub fn shell_outcome_text(outcome: &ShellOutcome) -> Option<String> {
    let _ = outcome;
    todo!("#225")
}

/// The cell text for a side outcome. `None` means the answer renders.
pub fn side_outcome_text(outcome: &SideOutcome) -> Option<String> {
    let _ = outcome;
    todo!("#225")
}

/// A duration for a cell header: one decimal below ten seconds, whole above.
pub fn format_duration(ms: u64) -> String {
    let _ = ms;
    todo!("#225")
}
