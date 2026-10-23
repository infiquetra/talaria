//! Protocol rules enforced by Hermes but omitted from its published schema.

use crate::methods::canonical::prompt_submit::Params;
use std::{error::Error, fmt};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OverrideError {
    pub reason: String,
}

impl fmt::Display for OverrideError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.reason)
    }
}

impl Error for OverrideError {}

/// Supply the submission identifier required by Hermes
/// `gateway/session_controls.py:751-758` at commit
/// `4d04303818b61bf3aa09278af48356315515965b`.
/// `unattended` stays false: `gateway/session_finite.py:42-52` at that revision
/// makes it an explicit opt-in to automatic command approval.
pub fn prompt_submit_params(
    session_id: impl Into<String>,
    text: impl Into<String>,
    submission_id: impl Into<String>,
) -> Params {
    Params {
        profile: None,
        session_id: session_id.into(),
        text: text.into(),
        submission_id: Some(submission_id.into()),
        input_id: None,
        queued: false,
        attachments: None,
        finite: false,
        unattended: false,
        surface: None,
        voice_context: None,
        interrupted: false,
        voice_turn: false,
        display_kind: None,
        title_preview: None,
        turn_author: None,
        api_text: None,
        model_override: None,
        clarify_batch: false,
    }
}

/// Check the identifier rule in Hermes `gateway/session_controls.py:751-758`
/// and reject automatic command approval from `gateway/session_finite.py:42-52`,
/// both at commit `4d04303818b61bf3aa09278af48356315515965b`.
pub fn check_prompt_submit(params: &Params) -> Result<(), OverrideError> {
    if params.unattended {
        return Err(OverrideError {
            reason: "prompt.submit unattended must not be true".into(),
        });
    }
    if ![params.submission_id.as_deref(), params.input_id.as_deref()]
        .into_iter()
        .flatten()
        .any(|id| !id.is_empty())
    {
        return Err(OverrideError {
            reason: "prompt.submit requires a non-empty submission_id or input_id".into(),
        });
    }
    Ok(())
}
