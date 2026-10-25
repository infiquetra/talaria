//! Running automation effects against Hermes, and the countdown ticker. Stubs.
//!
//! Behavior contract: issue 236.

use std::time::Duration;

use hermes_client::Client;
use talaria_core::automation::{AutomationEffect, AutomationEvent, AutomationState};
use talaria_core::commands::{CommandCatalog, CommandOutcome};

use crate::commands::AppAction;

/// Runs automation effects through a Hermes client.
#[derive(Debug)]
#[allow(dead_code)]
pub struct AutomationService {
    _private: (),
}

impl AutomationService {
    /// A service calling through `client`.
    pub fn new(client: Client) -> Self {
        let _ = client;
        todo!("#236")
    }

    /// Run one effect, answering the event its reply feeds back, if any.
    pub async fn execute(&self, effect: AutomationEffect) -> Option<AutomationEvent> {
        let _ = effect;
        todo!("#236")
    }
}

/// Map a finished presented command to the session actions that show it.
///
/// Empty when this unit does not present the typed line, so the existing
/// command path handles the outcome. Behavior contract: issue 236.
#[must_use]
pub fn automation_actions(
    typed: &str,
    outcome: &CommandOutcome,
    catalog: Option<&CommandCatalog>,
) -> Vec<AppAction> {
    let _ = (typed, outcome, catalog);
    todo!("#236")
}

/// Wall-clock epoch milliseconds; the only clock read of this unit.
pub fn wall_clock_ms() -> u64 {
    todo!("#236")
}

/// What a wake-up found.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Wake {
    /// True when the section's text changed since the last call.
    pub redraw: bool,
    /// How long until the next visible text change, if any.
    pub next_in: Option<Duration>,
}

/// Wakes the app only when a countdown's text changes.
#[derive(Debug, Default)]
#[allow(dead_code)]
pub struct AutomationTicker {
    last_text: Option<Vec<String>>,
}

impl AutomationTicker {
    /// Compare the rows' text at `now_ms` with the last call.
    pub fn on_wake(&mut self, state: &AutomationState, now_ms: u64) -> Wake {
        let _ = (state, now_ms);
        let _ = &self.last_text;
        todo!("#236")
    }
}
