use crate::rest::{RestClient, RestError};
use serde_json::Value;

pub async fn gateway_status(_rest: &RestClient) -> Result<Value, RestError> {
    Err(RestError::NotFound {
        route: String::new(),
    })
}

pub async fn restart_gateway(_rest: &RestClient) -> Result<Value, RestError> {
    Err(RestError::NotFound {
        route: String::new(),
    })
}
