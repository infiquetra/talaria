//! Extraction and bounded JSON Schema validation.
use serde_json::Value;

pub fn extract_json_candidate(text: &str) -> &str {
    let _ = text;
    todo!("hermes binding stub")
}

pub fn check_schema(schema: &Value) -> Result<(), String> {
    let _ = schema;
    todo!("hermes binding stub")
}

pub fn validate_output(text: &str, schema: &Value) -> Result<Value, Vec<String>> {
    let _ = (text, schema);
    todo!("hermes binding stub")
}
