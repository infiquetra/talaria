//! Output-contract wording shared with the pinned delegation helper.
use serde_json::Value;
const CONTRACT: &str = "OUTPUT CONTRACT (machine-validated):\nYour FINAL response must be ONLY the JSON value that validates against this JSON Schema — no prose before or after it, no code fence, no explanation. Anything else costs a correction turn and, if it fails again, is handed to the caller unvalidated.";
pub fn brief(prompt: &str, schema: Option<&Value>) -> String {
    let Some(schema) = schema else {
        return prompt.into();
    };
    let schema = serde_json::to_string_pretty(schema).unwrap_or_default();
    let block = format!("{CONTRACT}\n{schema}\n");
    let prompt = prompt.trim_end();
    if prompt.is_empty() {
        block
    } else {
        format!("{prompt}\n\n{block}")
    }
}
pub fn correction_message(errors: &[String]) -> String {
    let errors = errors
        .iter()
        .map(|e| format!("- {e}"))
        .collect::<Vec<_>>()
        .join("\n");
    format!(
        "Your previous final response was rejected by the output contract validator. Validation errors:\n{errors}\n\nReply with ONLY the corrected JSON object matching the OUTPUT CONTRACT schema from your task context. No prose, no explanations.\n"
    )
}
