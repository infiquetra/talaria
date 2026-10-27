use crate::rest::{RestClient, RestError};

#[derive(Debug, serde::Deserialize)]
pub struct EnabledReply {
    pub ok: bool,
    pub name: String,
    pub enabled: bool,
}

pub async fn set_server_enabled(
    _rest: &RestClient,
    _name: &str,
    _enabled: bool,
) -> Result<EnabledReply, RestError> {
    todo!("mcp stub")
}
