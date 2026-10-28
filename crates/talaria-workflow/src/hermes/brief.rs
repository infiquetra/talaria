//! Output-contract wording shared with the delegation helper.
use serde_json::Value;

pub fn brief(prompt: &str, schema: Option<&Value>) -> String {
    let _ = (prompt, schema);
    todo!("hermes binding stub")
}

pub fn correction_message(errors: &[String]) -> String {
    let _ = errors;
    todo!("hermes binding stub")
}
