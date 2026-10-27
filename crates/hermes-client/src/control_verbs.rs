//! Extra control-socket verbs beyond discovery. Issue #253.

use std::path::Path;
use std::time::Duration;

use crate::discovery::{Endpoint, NodeKind, NodeProblem};

/// How long a rescan waits for the gateway's answer.
pub const RESCAN_TIMEOUT: Duration = Duration::from_secs(10);

/// What a rescan answered. Absent lists are empty.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RescanOutcome {
    /// Whether the gateway multiplexes.
    pub multiplex: bool,
    /// Profiles added by this reconcile.
    pub added: Vec<String>,
    /// Profiles removed by this reconcile.
    pub removed: Vec<String>,
    /// Profiles parked by this reconcile.
    pub parked: Vec<String>,
    /// Served profile names.
    pub served: Vec<String>,
    /// The gateway is still rescanning.
    pub pending: bool,
}

/// Why a control verb did not return an outcome. Texts carry no path.
#[derive(Debug)]
pub enum ControlVerbError {
    /// The gateway predates the verb.
    UnknownVerb,
    /// The gateway refused the verb, with scrubbed words.
    Refused(String),
    /// No answer within the deadline.
    Timeout,
    /// The reply did not decode.
    InvalidReply,
    /// The socket read or write failed.
    Io(std::io::ErrorKind),
    /// A node check failed before anything was sent.
    UnsafeNode {
        /// The node that failed.
        node: NodeKind,
        /// How it failed.
        problem: NodeProblem,
    },
}

/// Served names and scrubbed `(name, reason)` parked pairs of one endpoint.
#[must_use]
pub fn served_and_parked(
    _endpoint: &Endpoint,
    _home: &Path,
) -> (Vec<String>, Vec<(String, String)>) {
    todo!("profiles stub")
}

/// Scrub one reason the way peer text is scrubbed: a home path reads `<home>`,
/// any other absolute path `<path>`, at most 500 characters.
#[must_use]
pub fn scrub_reason(_text: &str, _home: &Path) -> String {
    todo!("profiles stub")
}

/// Ask the gateway to rescan profiles over the control socket. Issue #253.
pub async fn rescan_profiles(
    _home: &Path,
    _timeout: Duration,
) -> Result<RescanOutcome, ControlVerbError> {
    todo!("profiles stub")
}
