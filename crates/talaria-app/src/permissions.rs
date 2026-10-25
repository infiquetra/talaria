//! The permissions service for #237: the typed calls behind the reducer.
//!
//! Stubs: every behaviour panics with `unimplemented!()` until the green
//! implementation lands.

use std::sync::Arc;

use hermes_client::channel::Client;
use talaria_core::permissions::{CallFailure, PermissionsEffect, PermissionsInput};

/// Runs the permissions reducer's read and write effects.
pub struct PermissionsService {
    client: Arc<Client>,
}

impl PermissionsService {
    /// A service that calls through the given connection.
    pub fn new(client: Arc<Client>) -> PermissionsService {
        PermissionsService { client }
    }

    /// Performs one effect and returns the input its reply feeds back, if any.
    pub async fn execute(&self, effect: PermissionsEffect) -> Option<PermissionsInput> {
        let _ = (&self.client, effect);
        unimplemented!("not yet implemented")
    }

    /// Maps one call error to the failure the reducer handles.
    pub fn classify(&self, error: &hermes_client::CallError) -> CallFailure {
        let _ = error;
        unimplemented!("not yet implemented")
    }
}
