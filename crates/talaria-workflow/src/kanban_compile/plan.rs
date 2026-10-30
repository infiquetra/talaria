//! Compile identity and the card body. Both stand in until emission exists.

use serde_json::Value;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CallLine {
    pub digest: String,
    pub occurrence: u32,
    pub assignee: String,
    pub tenant: String,
    pub model: String,
    pub provider: String,
    pub effort: String,
    pub workspace_kind: String,
    pub workspace_path: String,
    pub workflow_path: String,
}

pub fn compile_id(
    script_source: &str,
    args: &Value,
    calls: &[CallLine],
    children: &[&str],
) -> String {
    let _ = (script_source, args, calls, children);
    String::new()
}

pub fn card_body(
    script_name: &str,
    compile_id: &str,
    label: &str,
    phase: &str,
    prompt: &str,
    schema: Option<&Value>,
) -> String {
    let _ = (script_name, compile_id, label, phase, prompt, schema);
    String::new()
}
