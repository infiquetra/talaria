//! Dashboard routes for running a scheduled job and reading its history.

use std::time::Duration;

use serde_json::Value;

use crate::rest::{RequestOpts, RestClient, RestError};

pub const TRIGGER_DEADLINE: Duration = Duration::from_secs(24 * 60 * 60);

pub fn trigger_opts() -> RequestOpts {
    todo!()
}

pub fn encode_component(_text: &str) -> String {
    todo!()
}

pub async fn trigger_job(
    _rest: &RestClient,
    _profile: &str,
    _job_id: &str,
    _opts: RequestOpts,
) -> Result<(), RestError> {
    todo!()
}

pub async fn job_runs(
    _rest: &RestClient,
    _profile: &str,
    _job_id: &str,
    _limit: u32,
) -> Result<Value, RestError> {
    todo!()
}

pub async fn delivery_targets(_rest: &RestClient) -> Result<Value, RestError> {
    todo!()
}
