//! Runs profiles effects against Hermes and parses the answers. Issue #253.

use std::path::{Path, PathBuf};
use std::time::Duration;

use hermes_client::Client;
use serde_json::Value;
use talaria_core::profiles::{
    Description, GatewayView, HostTopology, ProfilesEffect, ProfilesInput, RosterEntry,
};

/// How long the roster call waits.
pub const LIST_TIMEOUT: Duration = Duration::from_secs(10);
/// How long a describe call waits.
pub const DESCRIBE_TIMEOUT: Duration = Duration::from_secs(15);
/// How long a create call waits.
pub const CREATE_TIMEOUT: Duration = Duration::from_secs(60);
/// How long a configure call waits.
pub const CONFIGURE_TIMEOUT: Duration = Duration::from_secs(15);

/// Runs one profiles effect and reports back as an input.
#[allow(dead_code)]
pub struct ProfilesService {
    /// The foreground connection.
    client: Client,
    /// The Hermes home the connection was opened with.
    home: PathBuf,
    /// The profile the connection's ticket is bound to.
    profile: String,
}

impl ProfilesService {
    /// Serve effects on one connection.
    #[must_use]
    pub fn new(client: Client, home: PathBuf, profile: String) -> Self {
        Self {
            client,
            home,
            profile,
        }
    }

    /// Run one effect; `None` when it reports nothing back.
    pub async fn execute(&self, _effect: ProfilesEffect) -> Option<ProfilesInput> {
        todo!("profiles stub")
    }
}

/// Parse a `profiles.list` result into roster rows, dropping paths.
pub fn parse_roster(_result: &Value) -> Result<Vec<RosterEntry>, String> {
    todo!("profiles stub")
}

/// Parse a `profiles.describe` result into a description.
pub fn parse_description(_result: &Value) -> Result<Description, String> {
    todo!("profiles stub")
}

/// Parse a status reply into a host topology, reading four fields only.
pub fn parse_topology(_reply: &Value, _home: &Path) -> Result<HostTopology, String> {
    todo!("profiles stub")
}

/// Split an endpoint's served and parked names for one home.
#[must_use]
pub fn gateway_view(_endpoint: &hermes_client::Endpoint, _home: &Path) -> GatewayView {
    todo!("profiles stub")
}
