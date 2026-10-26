//! A short-lived link used by one fleet launch. No reconnect.

use std::path::Path;

use crate::bg::outcome::LaunchError;

/// What the link was allowed to reach.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LinkScope {
    /// A host ticket, listing the profiles the gateway served.
    Host { profiles: Vec<String> },
    /// A profile ticket, after the host ticket was refused.
    ProfileOnly { profile: String },
}

/// One connection opened for a single launch.
#[derive(Debug)]
pub struct LaunchLink {
    scope: LinkScope,
}

impl LaunchLink {
    /// Open a link against the Hermes home this process uses.
    ///
    /// Step 0 does not connect. It reports a host scope so a test that expects a
    /// profile-only fallback fails.
    pub async fn open(home: &Path, foreground_profile: &str) -> Result<Self, LaunchError> {
        let _ = home;
        Ok(Self {
            scope: LinkScope::Host {
                profiles: vec![foreground_profile.to_owned()],
            },
        })
    }

    /// The scope the ticket was minted for.
    #[must_use]
    pub fn scope(&self) -> &LinkScope {
        &self.scope
    }
}
