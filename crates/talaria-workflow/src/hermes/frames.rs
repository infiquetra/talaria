//! Explicit allowlists for canonical gateway requests.
use crate::journal::ResolvedLaunch;
use serde_json::{Map, Value};

pub const CREATE_KEYS: [&str; 10] = [
    "request_id",
    "source",
    "model",
    "provider",
    "reasoning",
    "cwd",
    "cwd_explicit",
    "toolsets",
    "skills",
    "max_turns",
];

#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum FrameError {
    #[error("request identifier has invalid length {0}")]
    RequestIdLength(usize),
    #[error("submission identifier has invalid length {0}")]
    SubmissionIdLength(usize),
    #[error("launch needs an explicit provider")]
    MissingProvider,
}

pub fn create_params(
    launch: &ResolvedLaunch,
    request_id: &str,
) -> Result<Map<String, Value>, FrameError> {
    let _ = (launch, request_id);
    todo!("hermes binding stub")
}

pub fn submit_params(
    session_id: &str,
    submission_id: &str,
    text: &str,
) -> Result<Map<String, Value>, FrameError> {
    let _ = (session_id, submission_id, text);
    todo!("hermes binding stub")
}
