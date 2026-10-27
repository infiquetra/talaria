//! Profile control requests using discovery's authenticated socket exchange.

use std::path::Path;
use std::time::Duration;

use serde_json::json;

use crate::discovery::{DiscoveryError, Endpoint, NodeKind, NodeProblem, control_request};

/// How long a rescan waits for the gateway's answer.
pub const RESCAN_TIMEOUT: Duration = Duration::from_secs(10);

/// What a rescan answered. Absent lists are empty.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RescanOutcome {
    pub multiplex: bool,
    pub added: Vec<String>,
    pub removed: Vec<String>,
    pub parked: Vec<String>,
    pub served: Vec<String>,
    pub pending: bool,
}

/// Why a control verb did not return an outcome. Texts carry no path.
#[derive(Debug)]
pub enum ControlVerbError {
    UnknownVerb,
    Refused(String),
    Timeout,
    InvalidReply,
    Io(std::io::ErrorKind),
    UnsafeNode {
        node: NodeKind,
        problem: NodeProblem,
    },
}

/// Served names and scrubbed parked pairs of one endpoint.
#[must_use]
pub fn served_and_parked(endpoint: &Endpoint, home: &Path) -> (Vec<String>, Vec<(String, String)>) {
    (
        endpoint
            .profiles()
            .iter()
            .map(|entry| entry.name.clone())
            .collect(),
        endpoint
            .parked()
            .iter()
            .map(|(name, reason)| (name.clone(), scrub_reason(reason, home)))
            .collect(),
    )
}

/// Replace home and absolute paths in peer text and retain at most 500 characters.
#[must_use]
pub fn scrub_reason(text: &str, home: &Path) -> String {
    crate::discovery::scrub_peer_text(text, home, 500)
}

/// Ask the gateway to rescan through the same safety checks as discovery.
pub async fn rescan_profiles(
    home: &Path,
    timeout: Duration,
) -> Result<RescanOutcome, ControlVerbError> {
    let reply = control_request(
        home,
        json!({"protocol": 1, "verb": "rescan-profiles", "id": 1}),
        timeout,
    )
    .await
    .map_err(map_discovery_error)?;
    if reply["ok"] == false {
        let text = reply["error"]
            .as_str()
            .ok_or(ControlVerbError::InvalidReply)?;
        return Err(if text.starts_with("unknown verb:") {
            ControlVerbError::UnknownVerb
        } else {
            ControlVerbError::Refused(scrub_reason(text, home))
        });
    }
    let result = reply
        .get("result")
        .filter(|value| value.is_object())
        .ok_or(ControlVerbError::InvalidReply)?;
    let names = |key: &str| -> Result<Vec<String>, ControlVerbError> {
        let Some(value) = result.get(key) else {
            return Ok(Vec::new());
        };
        value
            .as_array()
            .ok_or(ControlVerbError::InvalidReply)?
            .iter()
            .map(|value| {
                value
                    .as_str()
                    .map(|name| crate::discovery::scrub_peer_text(name, home, 200))
                    .ok_or(ControlVerbError::InvalidReply)
            })
            .collect()
    };
    Ok(RescanOutcome {
        multiplex: result["multiplex"]
            .as_bool()
            .ok_or(ControlVerbError::InvalidReply)?,
        pending: match result.get("pending") {
            None => false,
            Some(value) => value.as_bool().ok_or(ControlVerbError::InvalidReply)?,
        },
        added: names("added")?,
        removed: names("removed")?,
        parked: names("parked")?,
        served: names("served_profiles")?,
    })
}

fn map_discovery_error(error: DiscoveryError) -> ControlVerbError {
    match error {
        DiscoveryError::ControlTimeout => ControlVerbError::Timeout,
        DiscoveryError::UnsafeNode { node, problem } => {
            ControlVerbError::UnsafeNode { node, problem }
        }
        DiscoveryError::BadPointer(_) => ControlVerbError::UnsafeNode {
            node: NodeKind::PointerFile,
            problem: NodeProblem::WrongType,
        },
        DiscoveryError::Io(kind) => ControlVerbError::Io(kind),
        DiscoveryError::NoGateway => ControlVerbError::Io(std::io::ErrorKind::NotFound),
        _ => ControlVerbError::InvalidReply,
    }
}
