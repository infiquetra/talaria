//! One queue call at a time. The reducer decides the next step.
//!
//! Issue 232. Queue and steer.

use hermes_client::Client;
use hermes_client::rpc::CallError;
use hermes_contract::methods::canonical::{
    prompt_cancel, prompt_replace_queued, prompt_resolve_unknown, prompt_steer_queued,
    session_steer,
};
use hermes_contract::schemas::{
    AdmissionStatus, CanonicalCorrectionParams, CanonicalReplaceQueuedParams,
    CanonicalSteerQueuedParams, PromptCancelParams, PromptResolveUnknownParams,
};
use serde_json::Value;
use talaria_core::queue::{AdmissionId, QueueAction, QueueIntent, SubmissionId};
use talaria_core::session::{Effect, Event, SessionId};

/// Runs a single queue effect and returns the outcome events.
pub struct QueueService {
    client: Client,
    session_id: SessionId,
}

impl QueueService {
    /// Hold the client that will make the one call.
    #[must_use]
    pub fn new(client: Client, session_id: SessionId) -> Self {
        Self { client, session_id }
    }

    /// Make exactly one call. No loop, no retry, and no wait for frames.
    ///
    /// A refusal, a lost reply, and a reply that does not decode each become one event. The
    /// reducer decides whether anything is sent again. Issue 232. Queue and steer.
    pub async fn run(&mut self, effect: Effect) -> Vec<Event> {
        // Effects name their own session; the service's copy is the fallback for none.
        let _ = &self.session_id;
        let event = match effect {
            Effect::CancelQueued {
                session_id,
                admission_id,
            } => {
                let params = PromptCancelParams {
                    admission_id: admission_id.0.clone(),
                    session_id: session_id.as_str().to_string(),
                };
                let reply = self.client.call::<prompt_cancel::Marker>(params).await;
                row_event(QueueAction::Remove, admission_id, None, reply)
            }
            Effect::ReplaceQueued {
                session_id,
                admission_id,
                submission_id,
                text,
            } => {
                // The effect does not say whether this is an edit save or a plan step, so the
                // event reports an edit. The reducer reads the real action from its in-flight
                // call.
                let params = CanonicalReplaceQueuedParams {
                    admission_id: admission_id.0.clone(),
                    attachments: None,
                    display_kind: None,
                    profile: None,
                    session_id: session_id.as_str().to_string(),
                    submission_id: submission_id.0.clone(),
                    text,
                    title_preview: None,
                };
                let reply = self
                    .client
                    .call::<prompt_replace_queued::Marker>(params)
                    .await;
                row_event(QueueAction::Edit, admission_id, Some(submission_id), reply)
            }
            Effect::SteerQueued {
                session_id,
                admission_id,
            } => {
                let params = CanonicalSteerQueuedParams {
                    admission_id: admission_id.0.clone(),
                    profile: None,
                    session_id: session_id.as_str().to_string(),
                };
                let reply = self
                    .client
                    .call::<prompt_steer_queued::Marker>(params)
                    .await;
                row_event(QueueAction::Steer, admission_id, None, reply)
            }
            Effect::Steer {
                session_id,
                execution_generation,
                text,
            } => {
                let params = CanonicalCorrectionParams {
                    execution_generation,
                    profile: None,
                    session_id: session_id.as_str().to_string(),
                    text,
                };
                match self.client.call::<session_steer::Marker>(params).await {
                    Ok(result) => Event::SteerResult(result),
                    Err(error) => failure_event(QueueAction::SteerText, None, None, error),
                }
            }
            Effect::ResolveUnknown {
                session_id,
                admission_id,
                execution_generation,
            } => {
                // Exactly three keys. Never a profile.
                let params = PromptResolveUnknownParams {
                    admission_id: admission_id.0.clone(),
                    execution_generation,
                    session_id: session_id.as_str().to_string(),
                };
                let reply = self
                    .client
                    .call::<prompt_resolve_unknown::Marker>(params)
                    .await;
                row_event(QueueAction::Discard, admission_id, None, reply)
            }
            // Not queue calls; the session service runs these.
            Effect::SendPrompt { .. }
            | Effect::RunShell { .. }
            | Effect::AskSide { .. }
            | Effect::Interrupt { .. }
            | Effect::RespondApproval { .. }
            | Effect::RespondClarify { .. }
            | Effect::RefuseAnswer { .. } => {
                return vec![Event::QueueCallRefused {
                    action: QueueAction::Submit,
                    admission_id: None,
                    reason: "not_a_queue_call".to_string(),
                    detail: None,
                }];
            }
        };
        vec![event]
    }
}

/// The event for a call that names a row and answers with an admission receipt.
fn row_event(
    action: QueueAction,
    admission_id: AdmissionId,
    submission_id: Option<SubmissionId>,
    reply: Result<AdmissionStatus, CallError>,
) -> Event {
    match reply {
        Ok(result) => Event::QueueCallResult {
            action,
            admission_id: Some(admission_id),
            submission_id,
            result,
        },
        Err(error) => failure_event(action, Some(admission_id), submission_id, error),
    }
}

/// Map a failed call to a refusal, or to an unknown outcome when the gateway may have applied it.
fn failure_event(
    action: QueueAction,
    admission_id: Option<AdmissionId>,
    submission_id: Option<SubmissionId>,
    error: CallError,
) -> Event {
    match error {
        CallError::Rpc(error) => Event::QueueCallRefused {
            action,
            admission_id,
            reason: error.reason.unwrap_or_else(|| "refused".to_string()),
            detail: error
                .data
                .as_ref()
                .and_then(|data| data.get("detail"))
                .and_then(Value::as_str)
                .map(str::to_string),
        },
        // A lost reply, or a reply that did not decode: the gateway may have applied the call.
        error if error.outcome_unknown() || matches!(error, CallError::BadResult { .. }) => {
            Event::QueueCallUnknown {
                action,
                admission_id,
                submission_id,
            }
        }
        // Nothing was sent.
        CallError::NotConnected => Event::QueueCallRefused {
            action,
            admission_id,
            reason: "not_connected".to_string(),
            detail: None,
        },
        _ => Event::QueueCallRefused {
            action,
            admission_id,
            reason: "not_sent".to_string(),
            detail: None,
        },
    }
}

/// What `/queue` asks the session to do.
#[derive(Clone, Debug, PartialEq)]
pub enum QueueCommand {
    /// A queue intent.
    Intent(QueueIntent),
    /// Submit this text.
    Submit(String),
    /// Show this notice and make no call.
    Notice(&'static str),
}

/// The notice for `/queue list`, `/queue edit` and the other forms Talaria does not have.
const QUEUE_EDIT_NOTICE: &str =
    "Talaria edits the queue with Up. Type /queue <text> to queue a prompt.";

/// Parse the text after `/queue`. Issue 232. Queue and steer.
#[must_use]
pub fn queue_command(arg: &str) -> QueueCommand {
    let arg = arg.trim();
    if arg.is_empty() {
        return QueueCommand::Intent(QueueIntent::EnterSelection);
    }
    let (first, rest) = arg
        .split_once(char::is_whitespace)
        .map_or((arg, ""), |(first, rest)| (first, rest.trim()));
    match first {
        "list" | "edit" | "rm" | "move" | "clear" => QueueCommand::Notice(QUEUE_EDIT_NOTICE),
        "add" if !rest.is_empty() => QueueCommand::Submit(rest.to_string()),
        _ => QueueCommand::Submit(arg.to_string()),
    }
}

/// Parse the text after `/steer`. Issue 232. Queue and steer.
#[must_use]
pub fn steer_command(arg: &str) -> QueueCommand {
    let arg = arg.trim();
    if arg.is_empty() {
        QueueCommand::Notice("usage: /steer <text>")
    } else {
        QueueCommand::Intent(QueueIntent::SteerText(arg.to_string()))
    }
}
