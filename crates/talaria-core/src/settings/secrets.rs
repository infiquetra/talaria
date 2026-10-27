use serde_json::Value;
use std::fmt;
use zeroize::Zeroizing;

/// Ephemeral sensitive text; formatting deliberately never exposes its contents.
#[derive(Clone, Default, PartialEq, Eq)]
pub struct SecretField(Zeroizing<String>);
impl SecretField {
    pub fn new(value: String) -> Self {
        Self(Zeroizing::new(value))
    }
    pub fn expose(&self) -> &str {
        &self.0
    }
    pub(crate) fn text_mut(&mut self) -> &mut String {
        &mut self.0
    }
}
impl fmt::Debug for SecretField {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("SecretField([redacted])")
    }
}
pub fn is_secret_key(key: &str) -> bool {
    if key.eq_ignore_ascii_case("voice.record_key") {
        return false;
    }
    let leaf = key
        .rsplit('.')
        .next()
        .unwrap_or(key)
        .to_lowercase()
        .replace('-', "_");
    [
        "api_key",
        "apikey",
        "key",
        "token",
        "access_token",
        "refresh_token",
        "id_token",
        "secret",
        "client_secret",
        "password",
        "passwd",
        "authorization",
        "private_key",
        "bearer",
        "jwt",
    ]
    .contains(&leaf.as_str())
        || [
            "_api_key",
            "_token",
            "_secret",
            "_password",
            "_key",
            "_access_key",
        ]
        .iter()
        .any(|suffix| leaf.ends_with(suffix))
}
pub fn contains_secret_key(value: &Value) -> bool {
    match value {
        Value::Object(map) => map
            .iter()
            .any(|(key, value)| is_secret_key(key) || contains_secret_key(value)),
        Value::Array(items) => items.iter().any(contains_secret_key),
        _ => false,
    }
}
pub fn env_reference_names(raw: &Value) -> Vec<String> {
    fn walk(value: &Value, names: &mut Vec<String>) {
        match value {
            Value::String(text) => {
                let mut rest = text.as_str();
                while let Some(start) = rest.find("${") {
                    rest = &rest[start + 2..];
                    let Some(end) = rest.find('}') else { break };
                    let name = rest[..end]
                        .trim()
                        .strip_prefix("env:")
                        .unwrap_or(rest[..end].trim())
                        .trim();
                    if !name.is_empty() && !names.iter().any(|n| n == name) {
                        names.push(name.to_owned());
                    }
                    rest = &rest[end + 1..];
                }
            }
            Value::Array(items) => {
                for value in items {
                    walk(value, names);
                }
            }
            Value::Object(map) => {
                for value in map.values() {
                    walk(value, names);
                }
            }
            _ => {}
        }
    }
    let mut names = Vec::new();
    walk(raw, &mut names);
    names
}
pub(crate) fn has_env_reference(value: &Value) -> bool {
    match value {
        Value::String(s) => s.contains("${"),
        Value::Array(v) => v.iter().any(has_env_reference),
        Value::Object(v) => v.values().any(has_env_reference),
        _ => false,
    }
}
