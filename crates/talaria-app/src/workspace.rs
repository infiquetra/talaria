//! The workspace service for #237: the local folder check and the move.
//!
//! Stubs: every behaviour panics with `unimplemented!()` until the green
//! implementation lands. Moves send through the shared mutation helper.

use talaria_core::workspace::{CdEffect, CdInput};

use crate::mutation::MutationClient;

/// Runs the `/cd` reducer's check and send effects.
pub struct WorkspaceService {
    mutations: MutationClient,
}

impl WorkspaceService {
    /// A service that sends moves through the given mutation sender.
    pub fn new(mutations: MutationClient) -> WorkspaceService {
        WorkspaceService { mutations }
    }

    /// Performs one effect and returns the input its reply feeds back, if any.
    pub async fn execute(&self, effect: CdEffect) -> Option<CdInput> {
        let _ = (&self.mutations, effect);
        unimplemented!("not yet implemented")
    }
}
