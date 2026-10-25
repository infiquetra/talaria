//! Running shell commands and side questions against the gateway.
//!
//! Neither call is queued behind a turn or retried. The two mappings from a call result are pure,
//! so every `CallError` can be checked without a server.

use hermes_client::Client;
use hermes_client::rpc::{CallError, RpcError};
use hermes_contract::methods::{canonical, legacy};
use hermes_contract::schemas::{
    CanonicalShellExecParams, ShellExecResult, SideAgentParams, TaskIdResult,
};
use talaria_core::session::{Effect, Event};
use talaria_core::side::{RequestId, ShellOutcome};

/// Runs the two side effects through the link's one client.
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

    /// Run one side effect and return the events to feed back. Other effects return nothing.
    ///
    /// `now_ms` is the app clock: read before and after a shell call for its duration, and when
    /// a side question is accepted.
    pub async fn execute(&self, effect: Effect, now_ms: impl Fn() -> u64) -> Vec<Event> {
        match effect {
            Effect::RunShell {
                session_id,
                request_id,
                command,
            } => {
                let started = now_ms();
                let params = CanonicalShellExecParams {
                    session_id: session_id.as_str().to_string(),
                    command,
                    profile: None,
                };
                let result = self
                    .client
                    .call::<canonical::shell_exec::Marker>(params)
                    .await;
                vec![Event::ShellFinished {
                    request_id,
                    outcome: shell_outcome(result),
                    duration_ms: now_ms().saturating_sub(started),
                }]
            }
            Effect::AskSide {
                session_id,
                request_id,
                question,
            } => {
                let params = SideAgentParams {
                    session_id: session_id.as_str().to_string(),
                    text: question,
                    profile: None,
                };
                let result = self.client.call::<legacy::prompt_btw::Marker>(params).await;
                vec![side_reply_event(request_id, result, now_ms())]
            }
            Effect::SendPrompt { .. }
            | Effect::CancelQueued { .. }
            | Effect::ReplaceQueued { .. }
            | Effect::SteerQueued { .. }
            | Effect::Steer { .. }
            | Effect::ResolveUnknown { .. }
            | Effect::Interrupt { .. }
            | Effect::RespondApproval { .. }
            | Effect::RespondClarify { .. }
            | Effect::RefuseAnswer { .. } => Vec::new(),
        }
    }
}

/// The events for a side effect that could not be sent because no client is held.
#[must_use]
pub fn not_sent(effect: &Effect) -> Vec<Event> {
    match effect {
        Effect::RunShell { request_id, .. } => vec![Event::ShellFinished {
            request_id: *request_id,
            outcome: ShellOutcome::NotSent {
                not_connected: true,
            },
            duration_ms: 0,
        }],
        Effect::AskSide { request_id, .. } => vec![Event::SideNotSent {
            request_id: *request_id,
            not_connected: true,
        }],
        _ => Vec::new(),
    }
}

/// The reason a refusal is shown with: `unknown_method` for a missing method, else `data.reason`,
/// else the message, else the code.
fn refusal_reason(error: &RpcError) -> String {
    if error.is_method_not_found() {
        return "unknown_method".to_string();
    }
    error
        .reason
        .clone()
        .filter(|reason| !reason.is_empty())
        .or_else(|| Some(error.message.clone()).filter(|message| !message.is_empty()))
        .unwrap_or_else(|| error.code.to_string())
}

/// `data.detail` when it is a string.
fn refusal_detail(error: &RpcError) -> Option<String> {
    error
        .data
        .as_ref()
        .and_then(|data| data.get("detail"))
        .and_then(serde_json::Value::as_str)
        .map(str::to_string)
}

/// Map a shell call result onto its outcome.
pub fn shell_outcome(result: Result<ShellExecResult, CallError>) -> ShellOutcome {
    match result {
        Ok(result) => ShellOutcome::Ok {
            stdout: result.stdout,
            stderr: result.stderr,
            code: result.code,
        },
        Err(CallError::Rpc(error)) => ShellOutcome::Refused {
            reason: refusal_reason(&error),
            detail: refusal_detail(&error),
            rpc_code: error.code,
        },
        Err(CallError::TimedOut { .. } | CallError::Unknown { .. }) => ShellOutcome::Unknown,
        Err(CallError::BadResult { .. }) => ShellOutcome::Unreadable,
        Err(CallError::NotConnected) => ShellOutcome::NotSent {
            not_connected: true,
        },
        Err(CallError::Encode | CallError::InvalidTimeout) => ShellOutcome::NotSent {
            not_connected: false,
        },
    }
}

/// Map a side call result onto the event to reduce.
///
/// A lost or unreadable reply is `SideUnknown`: Hermes may have accepted the question, so its
/// answer can still match by question, and it is never asked again.
pub fn side_reply_event(
    id: RequestId,
    result: Result<TaskIdResult, CallError>,
    now_ms: u64,
) -> Event {
    match result {
        Ok(result) => Event::SideAccepted {
            request_id: id,
            task_id: result.task_id,
            accepted_ms: now_ms,
        },
        Err(CallError::Rpc(error)) => Event::SideRefused {
            request_id: id,
            reason: refusal_reason(&error),
            detail: refusal_detail(&error),
        },
        Err(CallError::NotConnected) => Event::SideNotSent {
            request_id: id,
            not_connected: true,
        },
        Err(CallError::Encode | CallError::InvalidTimeout) => Event::SideNotSent {
            request_id: id,
            not_connected: false,
        },
        Err(
            CallError::TimedOut { .. } | CallError::Unknown { .. } | CallError::BadResult { .. },
        ) => Event::SideUnknown { request_id: id },
    }
}
