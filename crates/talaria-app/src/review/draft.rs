//! Commit-message drafting with the session's own model. Issue #245.

use std::time::Duration;

use serde_json::{Value, json};
use talaria_core::directory::rows::RowKey;
use talaria_core::review::DRAFT_DIFF_MAX_CHARS;

pub use talaria_core::review::DraftError;

use crate::bg::{CallFailure, LaunchConn};
use crate::worktree::git::safe_text;

/// Hermes's model timeout is 60 seconds; the client waits longer for the reply.
const DRAFT_TIMEOUT: Duration = Duration::from_secs(90);

/// The code Hermes gives a refusal that carries a reason.
const REFUSED: i64 = 4001;

/// The code for a method Hermes does not have.
const METHOD_NOT_FOUND: i64 = -32601;

/// Draft a commit message over `conn`. Sends nothing without a session or a
/// route of its own.
pub async fn draft_message(
    conn: &LaunchConn,
    row: &RowKey,
    diff: &str,
    recent: &str,
    avoid: &str,
) -> Result<String, DraftError> {
    if row.session_id.is_empty() {
        return Err(DraftError::NoSession);
    }
    if !conn.reaches(&row.profile) {
        return Err(DraftError::NoOwnRoute);
    }
    let profile = conn.routed_profile(&row.profile);
    let params = oneshot_params(&row.session_id, profile.as_deref(), diff, recent, avoid);
    match conn.call("llm.oneshot", params, DRAFT_TIMEOUT).await {
        Ok(reply) => text_of(&reply),
        Err(failure) => Err(error_of(failure)),
    }
}

/// The closed `llm.oneshot` parameters for a draft.
pub fn oneshot_params(
    session_id: &str,
    profile: Option<&str>,
    diff: &str,
    recent: &str,
    avoid: &str,
) -> Value {
    let diff: String = diff.chars().take(DRAFT_DIFF_MAX_CHARS).collect();
    let mut params = json!({
        "session_id": session_id,
        "template": "commit_message",
        "temperature": 0.8,
        "variables": {"diff": diff, "recent_commits": recent, "avoid": avoid},
    });
    if let Some(profile) = profile {
        params["profile"] = Value::String(profile.to_owned());
    }
    params
}

/// The trimmed text of a reply. An empty text is a failure of its own.
fn text_of(reply: &Value) -> Result<String, DraftError> {
    let Some(text) = reply.get("text").and_then(Value::as_str) else {
        return Err(DraftError::Failed(
            "Hermes sent a reply without text".to_owned(),
        ));
    };
    let text = text.trim();
    if text.is_empty() {
        Err(DraftError::Failed("the model returned nothing".to_owned()))
    } else {
        Ok(text.to_owned())
    }
}

fn error_of(failure: CallFailure) -> DraftError {
    match failure {
        CallFailure::Refused {
            code,
            reason,
            fields,
        } => {
            if code == METHOD_NOT_FOUND {
                return DraftError::Unsupported;
            }
            let reason = reason.unwrap_or_else(|| format!("Hermes refused with code {code}"));
            if code == REFUSED {
                match reason.as_str() {
                    "not_found" => return DraftError::SessionGone,
                    "oneshot_busy" => return DraftError::Busy,
                    "runtime_draining" => return DraftError::Draining,
                    _ => {}
                }
            }
            let text = if fields.is_empty() {
                reason
            } else {
                format!("{reason} ({})", fields.join(", "))
            };
            DraftError::Failed(safe_text(&text))
        }
        CallFailure::Unknown => DraftError::NoReply,
        CallFailure::NotSent(reason) | CallFailure::Malformed(reason) => {
            DraftError::Failed(safe_text(&reason))
        }
    }
}
