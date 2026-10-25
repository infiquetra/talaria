//! Exact wire frames for the headless path.

use crate::failure::LaunchFailure;
use crate::launch::{Launch, SessionRequest};
use hermes_contract::schemas::RuntimeDescribeResult;

/// The session source headless runs are created with.
pub const ONESHOT_SOURCE: &str = "oneshot";

/// Why a headless session could not be planned.
#[derive(Debug)]
pub enum HeadlessStartError {
    OneshotMissing,
    Plan(LaunchFailure),
}

/// Build the submit frame for one finite turn.
pub fn submit_params_finite(
    session_id: &str,
    text: &str,
    submission_id: &str,
) -> serde_json::Value {
    serde_json::json!({"session_id": session_id, "text": text,
        "submission_id": submission_id, "finite": true})
}

/// Build the receipt frame, always asking for the stored turn result.
pub fn receipt_params(session_id: &str, admission_id: &str) -> serde_json::Value {
    serde_json::json!({"session_id": session_id, "admission_id": admission_id,
        "include_result": true})
}

/// Build the cancel frame for one queued admission.
pub fn cancel_params(session_id: &str, admission_id: &str) -> serde_json::Value {
    serde_json::json!({"session_id": session_id, "admission_id": admission_id})
}

/// Build the interrupt frame for one running generation.
pub fn interrupt_params(session_id: &str, execution_generation: i64) -> serde_json::Value {
    serde_json::json!({"session_id": session_id, "execution_generation": execution_generation})
}

/// Build the deny frame for one approval prompt.
pub fn approval_deny_params(
    session_id: &str,
    execution_generation: i64,
    prompt_id: &str,
) -> serde_json::Value {
    serde_json::json!({"session_id": session_id, "execution_generation": execution_generation,
        "prompt_id": prompt_id, "choice": "deny"})
}

/// Build the skip frame for one question prompt.
pub fn clarify_skip_params(
    session_id: &str,
    execution_generation: i64,
    prompt_id: &str,
) -> serde_json::Value {
    serde_json::json!({"session_id": session_id, "execution_generation": execution_generation,
        "prompt_id": prompt_id, "answer": ""})
}

/// Plan the session, then mark a new one as a headless run.
pub fn plan_headless(
    launch: &Launch,
    request_id: String,
    describe: &RuntimeDescribeResult,
) -> Result<SessionRequest, HeadlessStartError> {
    if launch.mode == crate::launch::StartMode::New
        && !describe
            .session_create
            .sources
            .iter()
            .any(|source| source == ONESHOT_SOURCE)
    {
        return Err(HeadlessStartError::OneshotMissing);
    }
    let mut request = crate::launch::plan(launch, request_id, describe)
        .map_err(|error| HeadlessStartError::Plan(error.into()))?;
    if let SessionRequest::New(session) = &mut request {
        session.source = ONESHOT_SOURCE.to_string();
    }
    Ok(request)
}
