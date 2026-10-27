//! Provider calls. `execute` returns no inputs and does not set an expiry.

use std::time::Duration;

use hermes_client::Client;
use hermes_client::rest::RestClient;
use serde_json::Value;
use talaria_core::providers::{ProvidersEffect, ProvidersInput};
use zeroize::Zeroizing;

/// Runs provider effects for one connection.
pub struct ProviderService {
    profile_name: String,
}

impl ProviderService {
    /// Keep the client, the loopback client and the profile name. Nothing is called.
    #[must_use]
    pub fn new(client: Client, rest: RestClient, profile_name: String) -> Self {
        let _ = (client, rest);
        Self { profile_name }
    }

    /// Run one effect. The stub returns no inputs and drops the secret.
    pub async fn execute(
        &self,
        effect: ProvidersEffect,
        secret: Option<Zeroizing<String>>,
    ) -> Vec<ProvidersInput> {
        let _ = (&self.profile_name, effect, secret);
        Vec::new()
    }

    /// Cancel sign-ins that are still open. The stub has none.
    pub async fn close(&self) {
        let _ = &self.profile_name;
    }
}

/// Browser command for an https link. The stub returns nothing.
#[must_use]
pub fn opener_argv(url: &str) -> Option<Vec<String>> {
    let _ = url;
    None
}

/// Seconds from `now_unix` until a status expiry. The stub never sets one.
#[must_use]
pub fn expires_in_s(value: &Value, now_unix: i64) -> Option<i64> {
    let _ = (value, now_unix);
    None
}

/// Outer timeout the service would use for a key check or a sign-in start.
pub const CHECK_TIMEOUT: Duration = Duration::from_secs(20);

/// Outer timeout the service would use for a poll.
pub const POLL_TIMEOUT: Duration = Duration::from_secs(10);

/// Outer timeout the service would use for a cancel.
pub const CANCEL_TIMEOUT: Duration = Duration::from_secs(5);
