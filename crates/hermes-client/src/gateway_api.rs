//! Gateway health and explicitly confirmed restart routes.

use std::time::Duration;

use crate::rest::{Method, RequestOpts, RestClient, RestError};

/// How long the public status read waits.
pub const STATUS_DEADLINE_SECS: u64 = 8;

/// Read the public status route without minting or sending a ticket.
pub async fn gateway_status(rest: &RestClient) -> Result<serde_json::Value, RestError> {
    rest.request(
        Method::Get,
        "/api/status",
        None,
        RequestOpts {
            ticket: false,
            deadline: Some(Duration::from_secs(STATUS_DEADLINE_SECS)),
        },
    )
    .await
}

/// Request a restart with no profile selector or body.
pub async fn restart_gateway(rest: &RestClient) -> Result<serde_json::Value, RestError> {
    rest.request(
        Method::Post,
        "/api/gateway/restart",
        None,
        RequestOpts {
            ticket: true,
            deadline: Some(Duration::from_secs(10)),
        },
    )
    .await
}
