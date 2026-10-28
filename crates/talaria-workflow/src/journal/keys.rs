use super::{ResolvedLaunch, RunId};
use crate::engine::CallId;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
pub(crate) fn sha(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}
pub(crate) fn canonical(value: &Value) -> String {
    match value {
        Value::Object(map) => {
            let sorted: std::collections::BTreeMap<_, _> = map.iter().collect();
            format!(
                "{{{}}}",
                sorted
                    .into_iter()
                    .map(|(k, v)| format!("{}:{}", json!(k), canonical(v)))
                    .collect::<Vec<_>>()
                    .join(",")
            )
        }
        Value::Array(a) => format!(
            "[{}]",
            a.iter().map(canonical).collect::<Vec<_>>().join(",")
        ),
        _ => value.to_string(),
    }
}
pub fn journal_key(call: &CallId, launch: &ResolvedLaunch) -> String {
    let mut launch = json!(launch);
    if let Some(map) = launch.as_object_mut() {
        map.remove("cwd");
    }
    sha(canonical(&json!({"v":1,"call":call,"launch":launch})).as_bytes())[..32].into()
}
pub fn request_id(run: &RunId, key: &str, attempt: u32) -> String {
    let base = format!("wf:{}:{key}", run.as_str());
    if attempt == 0 {
        base
    } else {
        format!("{base}:a{attempt}")
    }
}
pub fn submission_id(run: &RunId, key: &str, attempt: u32, turn: u32) -> String {
    format!("{}:{turn}", request_id(run, key, attempt))
}
