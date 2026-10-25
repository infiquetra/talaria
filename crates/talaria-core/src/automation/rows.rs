//! The automation rows: modes, goal, loop, heartbeat. Stubs.
//!
//! Behavior contract: issue 236.

use super::state::AutomationState;

/// How a row is drawn.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tone {
    /// The accent colour.
    Accent,
    /// The warning colour.
    Warning,
}

/// One row of the automation section.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AutomationRow {
    /// How the label is drawn.
    pub tone: Tone,
    /// The fixed label.
    pub label: String,
    /// The computed detail.
    pub detail: String,
    /// The Hermes text tail.
    pub tail: String,
}

/// The rows for the state at `now_ms` (epoch milliseconds).
pub fn rows(state: &AutomationState, now_ms: u64) -> Vec<AutomationRow> {
    let _ = (state, now_ms);
    todo!("#236")
}

/// The next epoch-millisecond instant at which a row's text changes, if any.
pub fn next_tick(state: &AutomationState, now_ms: u64) -> Option<u64> {
    let _ = (state, now_ms);
    todo!("#236")
}

/// A countdown in Hermes's wording (`45m`, `1h05m`, `59s`).
pub fn countdown(remaining_ms: i64) -> String {
    let _ = remaining_ms;
    todo!("#236")
}

/// A cadence in Hermes's wording (`every 45m` is built by the caller).
pub fn format_interval(seconds: f64) -> String {
    let _ = seconds;
    todo!("#236")
}
