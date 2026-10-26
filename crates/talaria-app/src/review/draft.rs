//! Commit-message drafting with the session's own model. Step 0 stubs. Issue #245.

use serde_json::Value;
use talaria_core::directory::rows::RowKey;

pub use talaria_core::review::DraftError;

use crate::bg::LaunchConn;

/// Draft a commit message over `conn`. Sends nothing without a session or a
/// route of its own.
pub async fn draft_message(
    conn: &LaunchConn,
    row: &RowKey,
    diff: &str,
    recent: &str,
    avoid: &str,
) -> Result<String, DraftError> {
    let _ = (conn, row, diff, recent, avoid);
    Err(DraftError::NoSession)
}

/// The closed `llm.oneshot` parameters for a draft.
pub fn oneshot_params(
    session_id: &str,
    profile: Option<&str>,
    diff: &str,
    recent: &str,
    avoid: &str,
) -> Value {
    let _ = (session_id, profile, diff, recent, avoid);
    todo!()
}
