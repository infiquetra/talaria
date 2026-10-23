//! Protocol corrections the schema cannot state.
//!
//! Intended signatures, once `methods::canonical::prompt_submit::Params` exists:
//! `prompt_submit_params(...) -> methods::canonical::prompt_submit::Params`
//! `check_prompt_submit(&methods::canonical::prompt_submit::Params) -> Result<(), OverrideError>`
//!
//! The skeleton returns JSON so this crate compiles before that alias exists.
//! Hermes `gateway/session_controls.py` rejects a canonical `prompt.submit` that
//! has neither `submission_id` nor `input_id`, and `unattended: true` auto-approves
//! dangerous commands.

#[derive(Debug)]
pub struct OverrideError {
    pub reason: String,
}

pub fn prompt_submit_params(
    _session_id: impl Into<String>,
    _text: impl Into<String>,
    _submission_id: impl Into<String>,
) -> serde_json::Value {
    todo!("prompt_submit_params")
}

pub fn check_prompt_submit(_params: &serde_json::Value) -> Result<(), OverrideError> {
    todo!("check_prompt_submit")
}
