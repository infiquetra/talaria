//! Typed `session.mutate` calls. Issue 230.

use hermes_client::{CallOptions, Client};
use talaria_core::mutation::{MutationOutcome, MutationRequest};

/// Sends one fenced change and does not retry.
pub struct MutationClient {
    client: Client,
}

impl MutationClient {
    pub fn new(client: Client) -> Self {
        Self { client }
    }

    pub fn client(&self) -> &Client {
        &self.client
    }

    /// The default timeout. The same mapping as [`Self::send_with`].
    pub async fn send(&self, request: &MutationRequest) -> MutationOutcome {
        let _ = (request, self.client());
        todo!()
    }

    /// `send` with an explicit timeout. Still no retry.
    pub async fn send_with(
        &self,
        request: &MutationRequest,
        options: CallOptions,
    ) -> MutationOutcome {
        let _ = (request, options, self.client());
        todo!()
    }
}
