//! Explicit, deterministic allowlists for canonical gateway requests.
use crate::journal::ResolvedLaunch;
use serde_json::{Map, Value, json};

/// The `max_turns` value that tells Hermes "no step cap". Hermes accepts a positive cap, or 0,
/// -1, "none" or "unlimited" for no cap (`gateway/session_policy.py`, `resolve_turn_limit`), in
/// the running build and at tag v0.23.1. A missing key is NOT the same: the session then inherits
/// the profile's own `agent.max_turns`, which an old profile may have pinned to a finite number.
/// A null value is refused, so the key must always carry a number.
pub const NO_STEP_CAP: i64 = 0;

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
    let n = request_id.chars().count();
    if !(1..=256).contains(&n) {
        return Err(FrameError::RequestIdLength(n));
    }
    let provider = launch
        .provider
        .as_ref()
        .filter(|p| !p.trim().is_empty())
        .ok_or(FrameError::MissingProvider)?;
    let mut params = Map::new();
    params.insert("request_id".into(), json!(request_id));
    params.insert("source".into(), json!("tool"));
    params.insert("model".into(), json!(launch.model));
    params.insert("provider".into(), json!(provider));
    if let Some(effort) = launch.effort {
        params.insert("reasoning".into(), json!(effort));
    }
    if let Some(cwd) = &launch.cwd {
        params.insert("cwd".into(), json!(cwd));
    }
    params.insert("cwd_explicit".into(), json!(true));
    params.insert("toolsets".into(), json!(launch.toolsets));
    if !launch.skills.is_empty() {
        params.insert("skills".into(), json!(launch.skills));
    }
    // `None` in the launch record means the script asked for no cap; say so on the wire instead
    // of leaving the profile's own cap to apply.
    match launch.max_turns {
        Some(max_turns) => params.insert("max_turns".into(), json!(max_turns)),
        None => params.insert("max_turns".into(), json!(NO_STEP_CAP)),
    };
    Ok(params)
}
pub fn submit_params(
    session_id: &str,
    submission_id: &str,
    text: &str,
) -> Result<Map<String, Value>, FrameError> {
    let n = submission_id.chars().count();
    if !(1..=1024).contains(&n) {
        return Err(FrameError::SubmissionIdLength(n));
    }
    Ok(
        json!({"session_id":session_id,"submission_id":submission_id,"text":text,"finite":true})
            .as_object()
            .cloned()
            .unwrap_or_default(),
    )
}
