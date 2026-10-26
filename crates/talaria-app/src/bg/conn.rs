//! The connection one launch uses, then closes.

use std::time::Duration;

use serde_json::Value;

use crate::bg::link::LaunchLink;
use crate::bg::machine::{CallFailure, DescribeSummary};
use crate::connection::OpenConnection;

/// An already-open foreground connection, or a short-lived link opened for this launch.
// The open command-line connection is a large type. Tests match the variant, so it stays unboxed.
#[allow(clippy::large_enum_variant)]
pub enum LaunchConn {
    /// The command line's open connection.
    Opened(OpenConnection),
    /// A link opened for this launch alone.
    Link(LaunchLink),
}

impl LaunchConn {
    /// Send one call and wait up to `timeout`. Step 0 reports that nothing was written.
    pub async fn call(
        &self,
        method: &'static str,
        params: Value,
        timeout: Duration,
    ) -> Result<Value, CallFailure> {
        let _ = (method, params, timeout);
        match self {
            Self::Opened(open) => {
                let _ = open;
            }
            Self::Link(link) => {
                let _ = link;
            }
        }
        Err(CallFailure::NotSent("stub".to_owned()))
    }

    /// `Some(target)` when this is a host link and `target` is not the link's own profile.
    #[must_use]
    pub fn routed_profile(&self, target: &str) -> Option<String> {
        let _ = target;
        match self {
            Self::Opened(open) => {
                let _ = open;
            }
            Self::Link(link) => {
                let _ = link;
            }
        }
        None
    }

    /// The summary the handshake already fetched.
    #[must_use]
    pub fn describe(&self) -> DescribeSummary {
        match self {
            Self::Opened(open) => DescribeSummary {
                sources: open.describe.session_create.sources.clone(),
                parameters: open.describe.session_create.parameters.clone(),
            },
            Self::Link(_) => DescribeSummary {
                sources: Vec::new(),
                parameters: Vec::new(),
            },
        }
    }

    /// Close the connection. Closing detaches a session this link still holds.
    pub async fn close(self) {
        match self {
            Self::Opened(open) => {
                let _ = open;
            }
            Self::Link(link) => {
                let _ = link;
            }
        }
    }
}
