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
    let _ = (session_id, text, submission_id);
    todo!("#235")
}

/// Build the receipt frame, always asking for the stored turn result.
pub fn receipt_params(session_id: &str, admission_id: &str) -> serde_json::Value {
    let _ = (session_id, admission_id);
    todo!("#235")
}

/// Build the cancel frame for one queued admission.
pub fn cancel_params(session_id: &str, admission_id: &str) -> serde_json::Value {
    let _ = (session_id, admission_id);
    todo!("#235")
}

/// Build the interrupt frame for one running generation.
pub fn interrupt_params(session_id: &str, execution_generation: i64) -> serde_json::Value {
    let _ = (session_id, execution_generation);
    todo!("#235")
}

/// Build the deny frame for one approval prompt.
pub fn approval_deny_params(
    session_id: &str,
    execution_generation: i64,
    prompt_id: &str,
) -> serde_json::Value {
    let _ = (session_id, execution_generation, prompt_id);
    todo!("#235")
}

/// Build the skip frame for one question prompt.
pub fn clarify_skip_params(
    session_id: &str,
    execution_generation: i64,
    prompt_id: &str,
) -> serde_json::Value {
    let _ = (session_id, execution_generation, prompt_id);
    todo!("#235")
}

/// Plan the session, then mark a new one as a headless run.
pub fn plan_headless(
    launch: &Launch,
    request_id: String,
    describe: &RuntimeDescribeResult,
) -> Result<SessionRequest, HeadlessStartError> {
    let _ = (launch, request_id, describe);
    todo!("#235")
}
