//! Dashboard routes for running a scheduled job and reading its history.

use std::time::Duration;

use serde_json::Value;

use crate::rest::{Method, RequestOpts, RestClient, RestError};

pub const TRIGGER_DEADLINE: Duration = Duration::from_secs(24 * 60 * 60);

pub fn trigger_opts() -> RequestOpts {
    RequestOpts {
        ticket: true,
        deadline: Some(TRIGGER_DEADLINE),
    }
}

pub fn encode_component(text: &str) -> String {
    use std::fmt::Write;
    let mut encoded = String::new();
    for byte in text.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~') {
            encoded.push(char::from(byte));
        } else {
            let _ = write!(encoded, "%{byte:02X}");
        }
    }
    encoded
}

pub async fn trigger_job(
    rest: &RestClient,
    profile: &str,
    job_id: &str,
    opts: RequestOpts,
) -> Result<(), RestError> {
    let path = format!(
        "/api/cron/jobs/{}/trigger?profile={}",
        encode_component(job_id),
        encode_component(profile)
    );
    rest.request(Method::Post, &path, None, opts)
        .await
        .map(|_| ())
}

pub async fn job_runs(
    rest: &RestClient,
    profile: &str,
    job_id: &str,
    limit: u32,
) -> Result<Value, RestError> {
    let path = format!(
        "/api/cron/jobs/{}/runs?profile={}&limit={limit}",
        encode_component(job_id),
        encode_component(profile)
    );
    rest.request(Method::Get, &path, None, RequestOpts::default())
        .await
}

pub async fn delivery_targets(rest: &RestClient) -> Result<Value, RestError> {
    rest.request(
        Method::Get,
        "/api/cron/delivery-targets",
        None,
        RequestOpts::default(),
    )
    .await
}
