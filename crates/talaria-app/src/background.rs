//! Calls for background processes and detached prompt tasks.
//!
//! The service holds a client clone. It never runs on the input loop.

use hermes_client::Client;
use hermes_client::rpc::CallError;
use talaria_core::background::{
    BackgroundEffect, BackgroundEvent, CallFailure, KillOutcome, ProcessesIntent, StopOutcome,
};
use talaria_core::commands::LocalAction;
use talaria_core::session::SessionId;

/// Runs list, kill, stop, and background-task calls through one client.
pub struct BackgroundService {
    client: Client,
}

impl BackgroundService {
    /// Hold the client the service calls through.
    #[must_use]
    pub fn new(client: Client) -> Self {
        Self { client }
    }

    /// Run one background effect and return the event to feed back. Other effects return nothing.
    ///
    /// `now_ms` is the app clock, read when a reply is turned into an event.
    pub async fn execute(
        &self,
        session_id: &SessionId,
        effect: BackgroundEffect,
        now_ms: impl Fn() -> u64,
    ) -> Option<BackgroundEvent> {
        let _ = (&self.client, session_id, effect, now_ms());
        todo!()
    }
}

/// Map a call error onto a background failure. Total over the client's error enum.
#[must_use]
pub fn call_failure(error: &CallError) -> CallFailure {
    let _ = error;
    todo!()
}

/// Map a kill reply, or its call error, onto a kill outcome.
#[must_use]
pub fn kill_outcome(
    result: Result<hermes_contract::schemas::ProcessKillResult, CallError>,
) -> KillOutcome {
    let _ = result;
    todo!()
}

/// Map a stop reply, or its call error, onto a stop outcome.
#[must_use]
pub fn stop_outcome(
    result: Result<hermes_contract::schemas::ProcessStopResult, CallError>,
) -> StopOutcome {
    let _ = result;
    todo!()
}

/// The background event for one overlay intent, when the intent asks for one.
#[must_use]
pub fn intent_event(intent: &ProcessesIntent, now_ms: u64) -> Option<BackgroundEvent> {
    let _ = (intent, now_ms);
    todo!()
}

/// The background event for one local command, when the command is one of these three.
#[must_use]
pub fn command_event(
    action: &LocalAction,
    argument: &str,
    turn_running: bool,
    now_ms: u64,
) -> Option<BackgroundEvent> {
    let _ = (action, argument, turn_running, now_ms);
    todo!()
}
