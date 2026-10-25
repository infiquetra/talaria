//! Typed `session.mutate` calls.

use hermes_client::{CallError, CallOptions, Client};
use hermes_contract::{methods::canonical::session_mutate, schemas::SessionMutateParams};
use talaria_core::mutation::{MutationOutcome, MutationRequest, classify_error, classify_result};

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
        self.send_with(request, CallOptions::default()).await
    }

    /// `send` with an explicit timeout. Still no retry.
    pub async fn send_with(
        &self,
        request: &MutationRequest,
        options: CallOptions,
    ) -> MutationOutcome {
        let params = SessionMutateParams {
            session_id: request.session_id.clone(),
            request_id: request.request_id.clone(),
            expected_revision: request.expected_revision,
            expected_generation: request.expected_generation,
            operation: request.operation.clone(),
            payload: request.payload.clone(),
        };
        match self
            .client
            .call_with::<session_mutate::Marker>(params, options)
            .await
        {
            Ok(result) => match serde_json::to_value(result) {
                Ok(value) => classify_result(&value),
                Err(_) => MutationOutcome::Unreadable,
            },
            Err(error) => call_outcome(error),
        }
    }
}

/// Keep transport uncertainty distinct from an explicit gateway refusal.
pub(crate) fn call_outcome(error: CallError) -> MutationOutcome {
    match error {
        CallError::Unknown { .. } | CallError::TimedOut { .. } => MutationOutcome::Unknown,
        CallError::BadResult { .. } => MutationOutcome::Unreadable,
        CallError::Rpc(error) => classify_error(error.code, &error.message, error.data.as_ref()),
        error => MutationOutcome::NotSent {
            reason: error.to_string(),
        },
    }
}
