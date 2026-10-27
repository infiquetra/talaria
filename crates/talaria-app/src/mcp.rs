use std::time::Duration;

use hermes_client::{
    CallError, Client,
    rest::{RestClient, RestError},
};
use talaria_core::mcp::{Failure, McpEffect, McpInput};
use zeroize::Zeroizing;

pub struct McpService;

impl McpService {
    pub fn new(_client: Client, _rest: RestClient, _profile_name: String) -> Self {
        todo!("mcp stub")
    }
    pub async fn execute(
        &self,
        _effect: McpEffect,
        _secret: Option<Zeroizing<String>>,
    ) -> Vec<McpInput> {
        todo!("mcp stub")
    }
    pub async fn close(&self) {
        todo!("mcp stub")
    }
}

pub fn timeout_for(_effect: &McpEffect) -> Option<Duration> {
    todo!("mcp stub")
}
pub fn failure_from_call(_error: &CallError) -> Failure {
    todo!("mcp stub")
}
pub fn failure_from_rest(_error: &RestError) -> Failure {
    todo!("mcp stub")
}
pub fn decode_mcp_runtime_rows(
    _value: &serde_json::Value,
) -> Vec<hermes_contract::schemas::McpServerRuntimeRow> {
    todo!("mcp stub")
}
