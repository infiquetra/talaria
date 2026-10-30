//! Sanitized HTTP probe capture.
use serde_json::Value;
pub fn sanitize(_method: &str, _url: &str, _headers: &[(String, String)], _body: &Value) -> Value {
    todo!("add remote gateway implementation")
}
