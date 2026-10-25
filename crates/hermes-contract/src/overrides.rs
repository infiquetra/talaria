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

/// Transcript row fields that Hermes sends but leaves out of its published schema.
/// Hermes `gateway/session_local_recovery.py:313-314` at commit
/// `4d04303818b61bf3aa09278af48356315515965b` sets `archived_truncated: true` on
/// the first row when the byte budget left older archived history out.
#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
pub struct TranscriptMessageOverride {
    #[serde(
        default,
        deserialize_with = "deserialize_archived_truncated",
        skip_serializing_if = "Option::is_none"
    )]
    pub archived_truncated: Option<bool>,
}

/// Keep a boolean `archived_truncated` and read any other value as absent, so an
/// unexpected shape never fails the whole transcript row.
pub fn deserialize_archived_truncated<'de, D>(deserializer: D) -> Result<Option<bool>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let value = <Option<serde_json::Value> as serde::Deserialize>::deserialize(deserializer)?;
    Ok(match value {
        Some(serde_json::Value::Bool(flag)) => Some(flag),
        _ => None,
    })
}

/// Whether Hermes left earlier archived history out before this row.
pub fn transcript_archived_truncated(message: &crate::schemas::TranscriptMessage) -> Option<bool> {
    message.archived_truncated
}
