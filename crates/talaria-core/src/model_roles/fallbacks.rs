//! Fallback chain patch and identity.

use serde_json::Value;

use super::FallbackEntry;

pub fn fallback_patch(entries: &[FallbackEntry]) -> Value {
    let _ = entries;
    todo!("model roles stub")
}

pub fn route_identity(provider: &str, model: &str, base_url: &str) -> (String, String, String) {
    let _ = (provider, model, base_url);
    todo!("model roles stub")
}
