use crate::rest::{RestClient, RestError};
use hermes_contract::schemas::McpServerRuntimeRow;
use serde_json::Value;

/// Keep runtime rows understood by the pinned contract, ignoring future states.
pub fn decode_runtime_rows(value: &Value) -> Vec<McpServerRuntimeRow> {
    value
        .get("servers")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|row| serde_json::from_value(row.clone()).ok())
        .collect()
}

#[derive(Debug, serde::Deserialize)]
pub struct EnabledReply {
    pub ok: bool,
    pub name: String,
    pub enabled: bool,
}

pub async fn set_server_enabled(
    rest: &RestClient,
    name: &str,
    enabled: bool,
) -> Result<EnabledReply, RestError> {
    let mut encoded = String::with_capacity(name.len());
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    for byte in name.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~') {
            encoded.push(char::from(byte));
        } else {
            encoded.push('%');
            encoded.push(char::from(HEX[usize::from(byte >> 4)]));
            encoded.push(char::from(HEX[usize::from(byte & 15)]));
        }
    }
    let value = rest
        .put_json(
            &format!("/api/mcp/servers/{encoded}/enabled"),
            &serde_json::json!({ "enabled": enabled }),
        )
        .await?;
    serde_json::from_value(value).map_err(|_| RestError::BadBody("invalid enable reply".into()))
}
