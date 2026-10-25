//! Gateway calls for checkpoint rewind. Issue 233. Checkpoint rewind and diff.

use hermes_client::{CallError, Client};
use talaria_core::rewind::{Call, CallFailure, RewindInput};

/// Performs one rewind call and returns the reply for that ticket.
pub struct RewindService {
    client: Client,
    session_id: String,
}

impl RewindService {
    /// Calls go through `client` for `session_id`.
    #[must_use]
    pub fn new(client: Client, session_id: impl Into<String>) -> Self {
        Self {
            client,
            session_id: session_id.into(),
        }
    }

    /// Run `call` and return a reply carrying `ticket`.
    pub async fn execute(&self, ticket: u64, call: Call) -> RewindInput {
        let _ = (&self.client, &self.session_id, ticket, call);
        todo!("Issue 233. Checkpoint rewind and diff.")
    }
}

/// A fresh 32-character lowercase hex id for one `session.mutate`.
#[must_use]
pub fn new_request_id() -> String {
    todo!("Issue 233. Checkpoint rewind and diff.")
}

/// Map a client error from a resume or rollback call.
#[must_use]
pub fn failure_from(e: &CallError) -> CallFailure {
    let _ = e;
    todo!("Issue 233. Checkpoint rewind and diff.")
}
