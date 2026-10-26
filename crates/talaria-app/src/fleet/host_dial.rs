//! A host-scoped dial, with a profile-only fallback chosen by the service.

use std::future::Future;
use std::path::PathBuf;

use hermes_client::liveness::{Dial, DialFailure, Dialed};

use crate::side_connection::SideScope;

/// Home, foreground profile, and the scope the next dial will ask for.
pub struct HostDial {
    home: PathBuf,
    foreground_profile: String,
    scope: SideScope,
}

impl HostDial {
    /// Start on host scope for the foreground profile.
    #[must_use]
    pub fn new(home: PathBuf, foreground_profile: String) -> Self {
        let scope = SideScope::Host {
            profiles: vec![foreground_profile.clone()],
        };
        Self {
            home,
            foreground_profile,
            scope,
        }
    }

    /// The scope the next dial will ask for.
    #[must_use]
    pub fn scope(&self) -> SideScope {
        self.scope.clone()
    }
}

impl Dial for HostDial {
    fn dial(
        &mut self,
        generation: u64,
    ) -> impl Future<Output = Result<Dialed, DialFailure>> + Send {
        let _ = (generation, &self.home, &self.foreground_profile);
        async move { todo!() }
    }
}
