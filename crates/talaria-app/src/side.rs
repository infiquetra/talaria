//! Running shell commands and side questions against the gateway.

use hermes_client::Client;
use hermes_client::rpc::CallError;
use hermes_contract::schemas::{ShellExecResult, TaskIdResult};
use talaria_core::session::{Effect, Event};
use talaria_core::side::{RequestId, ShellOutcome};

/// Runs the two side effects through the link's one client.
#[allow(dead_code)]
pub struct SideService {
    client: Client,
}

impl SideService {
    /// Hold the client the service calls through.
    ///
    /// Why this constructor exists instead of building the client inside:
    /// tests must hand the service a client attached to the mock gateway.
    pub fn new(client: Client) -> Self {
        Self { client }
    }

    /// Run one side effect and return the events to feed back.
    pub async fn execute(&self, effect: Effect, now_ms: impl Fn() -> u64) -> Vec<Event> {
        let _ = (effect, now_ms);
        todo!("#225")
    }
}

/// Map a shell call result onto its outcome.
pub fn shell_outcome(result: Result<ShellExecResult, CallError>) -> ShellOutcome {
    let _ = result;
    todo!("#225")
}

/// Map a side call result onto the event to reduce.
pub fn side_reply_event(
    id: RequestId,
    result: Result<TaskIdResult, CallError>,
    now_ms: u64,
) -> Event {
    let _ = (id, result, now_ms);
    todo!("#225")
}
