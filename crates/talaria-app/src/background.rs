//! Calls for background processes and detached prompt tasks.
//!
//! The service holds a client clone. It never runs on the input loop.

use std::time::Duration;

use hermes_client::Client;
use hermes_client::rpc::{CallError, CallOptions};
use hermes_contract::methods::legacy;
use hermes_contract::schemas::{
    ProcessKillParams, ProcessKillResult, ProcessKillStatus, ProcessListParams, ProcessStopParams,
    SideAgentParams,
};
use serde_json::Value;
use talaria_core::background::{
    BackgroundEffect, BackgroundEvent, CallFailure, KillOutcome, ListedProcess, ProcessesIntent,
    StopOutcome,
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
        match effect {
            BackgroundEffect::ListProcesses { request } => {
                let result = match ProcessListParams::builder()
                    .session_id(session_id.as_str().to_string())
                    .try_into()
                {
                    Ok(params) => {
                        self.client
                            .call_with::<legacy::process_list::Marker>(params, timeout(10))
                            .await
                    }
                    Err(_) => Err(CallError::Encode),
                };
                Some(match result {
                    Ok(result) => BackgroundEvent::Listed {
                        request,
                        processes: result
                            .processes
                            .iter()
                            .map(ListedProcess::from_entry)
                            .collect(),
                        now_ms: now_ms(),
                    },
                    Err(error) => BackgroundEvent::ListFailed {
                        request,
                        failure: call_failure(&error),
                        now_ms: now_ms(),
                    },
                })
            }
            BackgroundEffect::KillProcess { request, id } => {
                let params = ProcessKillParams::builder()
                    .session_id(session_id.as_str().to_string())
                    .process_id(id.0.clone())
                    .try_into();
                Some(BackgroundEvent::KillReplied {
                    request,
                    process_id: id,
                    outcome: match params {
                        Ok(params) => self.kill(params).await,
                        Err(_) => kill_outcome(Err(CallError::Encode)),
                    },
                    now_ms: now_ms(),
                })
            }
            BackgroundEffect::StopProcesses { request } => {
                let result = match ProcessStopParams::builder()
                    .session_id(Some(session_id.as_str().to_string()))
                    .try_into()
                {
                    Ok(params) => {
                        self.client
                            .call_with::<legacy::process_stop::Marker>(params, timeout(60))
                            .await
                    }
                    Err(_) => Err(CallError::Encode),
                };
                Some(BackgroundEvent::StopReplied {
                    request,
                    outcome: stop_outcome(result),
                    now_ms: now_ms(),
                })
            }
            BackgroundEffect::StartBackground { request, text } => {
                let result = match SideAgentParams::builder()
                    .session_id(session_id.as_str().to_string())
                    .text(text)
                    .try_into()
                {
                    Ok(params) => {
                        self.client
                            .call_with::<legacy::prompt_background::Marker>(params, timeout(15))
                            .await
                    }
                    Err(_) => Err(CallError::Encode),
                };
                Some(match result {
                    Ok(result) => BackgroundEvent::BgAccepted {
                        request,
                        task_id: result.task_id,
                        now_ms: now_ms(),
                    },
                    Err(error) => BackgroundEvent::BgFailed {
                        request,
                        failure: call_failure(&error),
                        now_ms: now_ms(),
                    },
                })
            }
            BackgroundEffect::Interrupt
            | BackgroundEffect::InsertNotice { .. }
            | BackgroundEffect::Log { .. }
            | BackgroundEffect::InsertResult { .. }
            | BackgroundEffect::Signal(_)
            | BackgroundEffect::Redraw => None,
        }
    }

    async fn kill(&self, params: ProcessKillParams) -> KillOutcome {
        let params = match serde_json::to_value(params) {
            Ok(params) => params,
            Err(_) => return kill_outcome(Err(CallError::Encode)),
        };
        // The pinned registry returns scope_stop_failed, which its open result schema omits.
        // Keep that field beside the generated result so a failed process-group stop is visible.
        let raw = match self
            .client
            .call_raw("process.kill", params, timeout(30))
            .await
        {
            Ok(raw) => raw,
            Err(error) => return kill_outcome(Err(error)),
        };
        let scope_stop_failed = raw.get("scope_stop_failed").is_some_and(json_truthy);
        let result =
            serde_json::from_value::<ProcessKillResult>(raw).map_err(|_| CallError::BadResult {
                method: "process.kill".to_string(),
                reason: "result did not decode",
            });
        match kill_outcome(result) {
            KillOutcome::AlreadyExited { .. } => KillOutcome::AlreadyExited { scope_stop_failed },
            outcome => outcome,
        }
    }
}

fn timeout(seconds: u64) -> CallOptions {
    CallOptions {
        timeout: Some(Duration::from_secs(seconds)),
    }
}

fn json_truthy(value: &Value) -> bool {
    match value {
        Value::Null => false,
        Value::Bool(flag) => *flag,
        Value::Number(number) => number.as_f64().is_some_and(|number| number != 0.0),
        Value::String(text) => !text.is_empty(),
        Value::Array(items) => !items.is_empty(),
        Value::Object(fields) => !fields.is_empty(),
    }
}

/// Map a call error onto a background failure. Total over the client's error enum.
#[must_use]
pub fn call_failure(error: &CallError) -> CallFailure {
    match error {
        CallError::NotConnected | CallError::Encode | CallError::InvalidTimeout => {
            CallFailure::NotSent
        }
        CallError::Rpc(error) if error.code == 4001 => CallFailure::Refused {
            reason: error.reason.clone().unwrap_or_default(),
            detail: error.reason.as_ref().and_then(|_| {
                error
                    .data
                    .as_ref()?
                    .get("detail")?
                    .as_str()
                    .map(str::to_owned)
            }),
        },
        CallError::Rpc(error) if error.code == 5001 => CallFailure::Storage,
        CallError::Rpc(error) if error.is_method_not_found() => CallFailure::MethodMissing,
        CallError::Rpc(error) => CallFailure::Refused {
            reason: error
                .reason
                .clone()
                .unwrap_or_else(|| format!("error {}", error.code)),
            detail: None,
        },
        CallError::BadResult { .. } => CallFailure::Unreadable,
        CallError::TimedOut { .. } | CallError::Unknown { .. } => CallFailure::Unknown,
    }
}

/// Map a kill reply, or its call error, onto a kill outcome.
#[must_use]
pub fn kill_outcome(
    result: Result<hermes_contract::schemas::ProcessKillResult, CallError>,
) -> KillOutcome {
    match result {
        Ok(result) => match result.status {
            ProcessKillStatus::Killed => KillOutcome::Killed,
            ProcessKillStatus::AlreadyExited => KillOutcome::AlreadyExited {
                scope_stop_failed: false,
            },
            ProcessKillStatus::NotFound => KillOutcome::NotFound,
            ProcessKillStatus::Error => KillOutcome::Error {
                text: result.error.unwrap_or_default(),
            },
        },
        Err(error) => KillOutcome::Failed(call_failure(&error)),
    }
}

/// Map a stop reply, or its call error, onto a stop outcome.
#[must_use]
pub fn stop_outcome(
    result: Result<hermes_contract::schemas::ProcessStopResult, CallError>,
) -> StopOutcome {
    match result {
        Ok(result) => StopOutcome::Stopped {
            killed: result.killed,
            delegations: result.delegations,
        },
        Err(error) => StopOutcome::Failed(call_failure(&error)),
    }
}

/// The background event for one overlay intent, when the intent asks for one.
#[must_use]
pub fn intent_event(intent: &ProcessesIntent, now_ms: u64) -> Option<BackgroundEvent> {
    Some(match intent {
        ProcessesIntent::Kill(id) => BackgroundEvent::KillRequested {
            id: id.clone(),
            now_ms,
        },
        ProcessesIntent::StopAll => BackgroundEvent::StopAllRequested { now_ms },
        ProcessesIntent::Dismiss(id) => BackgroundEvent::Dismiss { id: id.clone() },
        ProcessesIntent::Refresh => BackgroundEvent::RefreshRequested { now_ms },
        ProcessesIntent::Close => BackgroundEvent::OverlayClosed,
        ProcessesIntent::Select(id) => BackgroundEvent::Selected { id: id.clone() },
        ProcessesIntent::None => return None,
    })
}

/// The background event for one local command, when the command is one of these three.
#[must_use]
pub fn command_event(
    action: &LocalAction,
    argument: &str,
    turn_running: bool,
    now_ms: u64,
) -> Option<BackgroundEvent> {
    match action {
        LocalAction::Bg => Some(BackgroundEvent::BgCommand {
            text: argument.to_string(),
            now_ms,
        }),
        LocalAction::Stop => Some(BackgroundEvent::StopCommand {
            turn_running,
            now_ms,
        }),
        LocalAction::Processes => Some(BackgroundEvent::OverlayOpened { now_ms }),
        _ => None,
    }
}
