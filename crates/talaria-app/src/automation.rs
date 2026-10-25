//! Running automation effects against Hermes, and the countdown ticker.
//!
//! Behavior contract: issue 236. Only `session.control.read` and `config.get` are called here:
//! never `session.automation`, whose accepted call turns the gateway's goal judge off for the
//! session, and never `config.set` or `session.control`.

use std::time::{Duration, SystemTime, UNIX_EPOCH};

use hermes_client::{CallError, CallOptions, Client};
use hermes_contract::methods::{canonical::config_get, legacy::session_control_read};
use talaria_core::automation::{
    AutomationEffect, AutomationEvent, AutomationRow, AutomationState, ControlView, ModeKey,
    Presentation, ReadFailure, canonical_name, next_tick, present_command, rows,
};
use talaria_core::commands::{CommandCatalog, CommandOutcome};
use talaria_core::session::Event;

use crate::commands::{AppAction, fresh_submission_id};

/// Every read waits this long for its reply and is never retried.
const READ_TIMEOUT: Duration = Duration::from_secs(5);

/// Runs automation effects through a Hermes client.
pub struct AutomationService {
    client: Client,
}

impl std::fmt::Debug for AutomationService {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("AutomationService")
            .finish_non_exhaustive()
    }
}

impl AutomationService {
    /// A service calling through `client`.
    pub fn new(client: Client) -> Self {
        Self { client }
    }

    /// Run one effect, answering the event its reply feeds back, if any.
    ///
    /// `Notice` and `Redraw` are the app's own work, so they answer nothing here.
    pub async fn execute(&self, effect: AutomationEffect) -> Option<AutomationEvent> {
        let options = CallOptions {
            timeout: Some(READ_TIMEOUT),
        };
        match effect {
            AutomationEffect::ReadControl { session_id, token } => {
                let params = session_control_read::Params {
                    session_id,
                    profile: None,
                };
                let result = self
                    .client
                    .call_with::<session_control_read::Marker>(params, options)
                    .await
                    .map_err(read_failure)
                    .and_then(|reply| {
                        ControlView::from_snapshot(reply.control)
                            .map_err(|_| ReadFailure::BadResult)
                    });
                Some(AutomationEvent::ControlRead { token, result })
            }
            AutomationEffect::ReadMode {
                session_id,
                key,
                token,
            } => {
                let params = config_get::Params {
                    key: mode_name(key).to_owned(),
                    session_id: Some(session_id.clone()),
                    profile: None,
                    cwd: None,
                };
                let result = self
                    .client
                    .call_with::<config_get::Marker>(params, options)
                    .await
                    .map(|reply| reply.value)
                    .map_err(read_failure);
                Some(AutomationEvent::ModeRead {
                    session_id,
                    key,
                    token,
                    result,
                })
            }
            AutomationEffect::Notice { .. } | AutomationEffect::Redraw => None,
        }
    }
}

fn mode_name(key: ModeKey) -> &'static str {
    match key {
        ModeKey::Yolo => "yolo",
        ModeKey::Fast => "fast",
    }
}

fn read_failure(error: CallError) -> ReadFailure {
    match error {
        CallError::Rpc(error) if error.code == -32601 => ReadFailure::MethodMissing,
        CallError::Rpc(error) => ReadFailure::Refused(error.reason.unwrap_or(error.message)),
        CallError::TimedOut { .. } => ReadFailure::TimedOut,
        CallError::BadResult { .. } => ReadFailure::BadResult,
        CallError::Unknown { .. }
        | CallError::NotConnected
        | CallError::Encode
        | CallError::InvalidTimeout => ReadFailure::Lost,
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
    let Some(canonical) = canonical_name(typed, catalog) else {
        return Vec::new();
    };
    let Some(presentation) = present_command(&canonical, typed, outcome) else {
        return Vec::new();
    };
    match presentation {
        Presentation::Output(cell) => vec![AppAction::Event(Event::HermesCommand { cell })],
        Presentation::Submit {
            cell,
            text,
            display,
        } => {
            let mut actions = Vec::new();
            if let Some(cell) = cell {
                actions.push(AppAction::Event(Event::HermesCommand { cell }));
            }
            actions.push(AppAction::Event(Event::SubmitCommand {
                text,
                display,
                submission_id: fresh_submission_id(),
            }));
            actions
        }
    }
}

/// Wall-clock epoch milliseconds; the only clock read of this unit.
pub fn wall_clock_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |since| {
            u64::try_from(since.as_millis()).unwrap_or(u64::MAX)
        })
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
pub struct AutomationTicker {
    last_text: Option<Vec<String>>,
}

impl AutomationTicker {
    /// Compare the rows' text at `now_ms` with the last call.
    pub fn on_wake(&mut self, state: &AutomationState, now_ms: u64) -> Wake {
        let text: Vec<String> = rows(state, now_ms).iter().map(row_text).collect();
        let redraw = self.last_text.as_ref() != Some(&text);
        self.last_text = Some(text);
        let next_in =
            next_tick(state, now_ms).map(|due| Duration::from_millis(due.saturating_sub(now_ms)));
        Wake { redraw, next_in }
    }
}

/// A row's text as drawn: label, detail and tail joined by their separators.
fn row_text(row: &AutomationRow) -> String {
    let mut text = row.label.clone();
    for part in [&row.detail, &row.tail] {
        if !part.is_empty() {
            text.push_str(" · ");
            text.push_str(part);
        }
    }
    text
}
