//! Two capturing runs. The comparison is not available yet.

use serde_json::Value;

use crate::engine::CallId;

#[derive(Clone, Debug, PartialEq)]
pub struct CapturedCall {
    pub call_id: CallId,
    pub label: String,
    pub phase: Option<String>,
    pub prompt: String,
    pub schema: Option<Value>,
    pub tier: Option<String>,
    pub model: Option<String>,
    pub provider: Option<String>,
    pub effort: Option<String>,
    pub profile: Option<String>,
    pub toolsets: Option<Vec<String>>,
    pub skills: Option<Vec<String>>,
    pub isolation: Option<String>,
    pub max_turns: Option<u32>,
    pub workflow_path: Vec<String>,
    pub outstanding: bool,
}

pub async fn capture_twice(source: &str) -> Result<(Vec<CapturedCall>, Vec<CapturedCall>), String> {
    let _ = source;
    Err("capture is not available".into())
}
