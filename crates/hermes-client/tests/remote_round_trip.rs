//! Verify the terminal event of one submitted turn.
use serde_json::Value;
pub fn matches_complete(frame: &Value, session: &str, admission: &str, expected: &str) -> bool {
    frame["type"] == "message.complete"
        && frame["session_id"] == session
        && frame["admission_id"] == admission
        && frame["payload"]["status"] == "complete"
        && frame["payload"]["text"]
            .as_str()
            .is_some_and(|text| text.trim() == expected)
        && frame["payload"].get("error").is_none_or(Value::is_null)
}
