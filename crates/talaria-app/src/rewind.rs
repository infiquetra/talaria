//! Gateway calls for checkpoint rewind. Issue #233.
//!
//! One call per [`Call`], answered with a reply that carries the caller's ticket. Nothing here
//! retries: a lost reply is reported as unknown and the state machine re-reads instead.

use std::time::Duration;

use hermes_client::{CallError, CallOptions, Client};
use hermes_contract::methods::canonical::rollback_session_diff;
use hermes_contract::methods::legacy::{rollback_diff, rollback_list, rollback_restore};
use hermes_contract::schemas::{
    CanonicalSnapshot, RollbackDiffParams, RollbackListParams, RollbackRestoreParams,
    RollbackRestoreResult, RollbackSessionDiffParams,
};
use serde_json::{Map, Value, json};
use talaria_core::mutation::{MutationOutcome, MutationRequest};
use talaria_core::rewind::{
    Call, CallFailure, CheckpointList, MutateOutcome, Reply, RestoreOutcome, RewindInput,
    SessionDiffOutcome, SessionRead, checkpoint_diff_doc, session_diff_doc,
};

use crate::mutation::MutationClient;

/// Both diff calls run up to six git steps of 30 seconds, one of them doubled.
const DIFF_TIMEOUT: Duration = Duration::from_secs(240);

/// Pending entries that block a rewind or a whole-folder restore.
const BLOCKING_PENDING: [&str; 3] = ["queued", "started", "unknown"];

/// Performs one rewind call and returns the reply for that ticket.
pub struct RewindService {
    client: Client,
    mutation: MutationClient,
    session_id: String,
}

impl RewindService {
    /// Calls go through `client` for `session_id`.
    #[must_use]
    pub fn new(client: Client, session_id: impl Into<String>) -> Self {
        Self {
            mutation: MutationClient::new(client.clone()),
            client,
            session_id: session_id.into(),
        }
    }

    /// Run `call` and return a reply carrying `ticket`.
    pub async fn execute(&self, ticket: u64, call: Call) -> RewindInput {
        let reply = match call {
            Call::ReadSession => self.read_session().await,
            Call::ListCheckpoints => self.list_checkpoints().await,
            Call::CheckpointDiff { hash } => self.checkpoint_diff(hash).await,
            Call::SessionDiff => self.session_diff().await,
            Call::Mutate {
                row_id,
                expected_revision,
                expected_generation,
            } => {
                self.mutate(row_id, expected_revision, expected_generation)
                    .await
            }
            Call::Restore {
                hash,
                file_path,
                all,
            } => self.restore(hash, file_path, all).await,
        };
        RewindInput::Reply { ticket, reply }
    }

    /// `session.resume` with the raw result kept: the rows are read raw, and the same value
    /// decodes into a snapshot for a rebuild.
    async fn read_session(&self) -> Reply {
        let params = json!({
            "session_id": self.session_id,
            "display_projection": true,
        });
        match self
            .client
            .call_raw("session.resume", params, CallOptions::default())
            .await
        {
            Ok(raw) => session_read(raw).map_or(Reply::Failed(CallFailure::Unreadable), |read| {
                Reply::SessionRead(read)
            }),
            Err(error) => Reply::Failed(failure_from(&error)),
        }
    }

    async fn list_checkpoints(&self) -> Reply {
        let params = RollbackListParams {
            profile: None,
            session_id: self.session_id.clone(),
        };
        match self.client.call::<rollback_list::Marker>(params).await {
            Ok(result) => Reply::Checkpoints(CheckpointList::from_result(
                result.enabled,
                result
                    .checkpoints
                    .into_iter()
                    .map(|row| (row.hash, row.timestamp, row.message))
                    .collect(),
            )),
            Err(error) => Reply::Failed(failure_from(&error)),
        }
    }

    /// `rollback.diff`. Its `rendered` field is never read.
    async fn checkpoint_diff(&self, hash: String) -> Reply {
        let short: String = hash.chars().take(8).collect();
        let params = RollbackDiffParams {
            hash,
            profile: None,
            session_id: self.session_id.clone(),
        };
        match self
            .client
            .call_with::<rollback_diff::Marker>(params, diff_options())
            .await
        {
            Ok(result) => {
                Reply::DiffLoaded(checkpoint_diff_doc(&short, &result.stat, &result.diff))
            }
            Err(error) => Reply::Failed(failure_from(&error)),
        }
    }

    /// `rollback.session_diff`. The state machine applies `--stat` to the document.
    async fn session_diff(&self) -> Reply {
        let params = RollbackSessionDiffParams {
            profile: None,
            session_id: self.session_id.clone(),
        };
        match self
            .client
            .call_with::<rollback_session_diff::Marker>(params, diff_options())
            .await
        {
            Ok(result) => {
                let outcome = SessionDiffOutcome {
                    enabled: result.enabled,
                    success: result.success,
                    stat: result.stat,
                    diff: result.diff,
                    diff_lines: u64::try_from(result.diff_lines).unwrap_or(0),
                    empty: result.empty,
                    error: result.error,
                };
                Reply::DiffLoaded(session_diff_doc(&outcome, false))
            }
            Err(error) => Reply::Failed(failure_from(&error)),
        }
    }

    /// `session.mutate` operation `rewind`, through the one mutation sender, with a fresh id.
    async fn mutate(&self, row_id: i64, expected_revision: i64, expected_generation: i64) -> Reply {
        let request = MutationRequest {
            session_id: self.session_id.clone(),
            request_id: new_request_id(),
            expected_revision,
            expected_generation: Some(expected_generation),
            operation: "rewind".to_string(),
            payload: Map::from_iter([("target_message_id".to_string(), Value::from(row_id))]),
        };
        match failure_from_mutation(self.mutation.send(&request).await) {
            Ok(outcome) => Reply::Mutated(outcome),
            Err(failure) => Reply::Failed(failure),
        }
    }

    /// `rollback.restore`: `file_path` only when given, `all: true` only when set.
    async fn restore(&self, hash: String, file_path: Option<String>, all: bool) -> Reply {
        let params = RollbackRestoreParams {
            all: all.then_some(true),
            file_path,
            hash,
            profile: None,
            session_id: self.session_id.clone(),
        };
        match self.client.call::<rollback_restore::Marker>(params).await {
            Ok(result) => Reply::Restored(restore_outcome(result)),
            Err(error) => Reply::Failed(failure_from(&error)),
        }
    }
}

fn diff_options() -> CallOptions {
    CallOptions {
        timeout: Some(DIFF_TIMEOUT),
    }
}

/// Read the fences, the idle check and the rows from a resume result. `None` when the value
/// is not a snapshot, so a rebuild from it could not decode either.
fn session_read(raw: Value) -> Option<SessionRead> {
    serde_json::from_value::<CanonicalSnapshot>(raw.clone()).ok()?;
    let revision = raw.get("revision").and_then(Value::as_i64)?;
    let execution_generation = raw.get("execution_generation").and_then(Value::as_i64)?;
    let running = raw.get("running").and_then(Value::as_bool)?;
    let pending = raw
        .get("pending")
        .and_then(Value::as_array)
        .map_or(0, |entries| {
            entries
                .iter()
                .filter(|entry| {
                    entry
                        .get("status")
                        .and_then(Value::as_str)
                        .is_some_and(|status| BLOCKING_PENDING.contains(&status))
                })
                .count()
        });
    let messages = raw
        .get("messages")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    Some(SessionRead {
        raw,
        revision,
        execution_generation,
        running,
        pending,
        messages,
    })
}

/// Everything but the absolute `directory` and the `debug` block.
fn restore_outcome(result: RollbackRestoreResult) -> RestoreOutcome {
    RestoreOutcome {
        success: result.success,
        restored_to: result.restored_to,
        reason: result.reason,
        file: result.file,
        restored_files: result.restored_files,
        skipped_user_edits: result.skipped_user_edits,
        skipped_oversize: result.skipped_oversize,
        failed_deletes: result.failed_deletes,
        history_removed: result.history_removed,
        error: result.error,
    }
}

/// A fresh 32-character lowercase hex id for one `session.mutate`.
#[must_use]
pub fn new_request_id() -> String {
    let mut bytes = [0_u8; 16];
    getrandom::fill(&mut bytes).expect("system randomness is available");
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

/// Map a client error from a resume or rollback call.
#[must_use]
pub fn failure_from(e: &CallError) -> CallFailure {
    match e {
        CallError::Rpc(error) => {
            let data = error.data.as_ref();
            let text = |key: &str| {
                data.and_then(|value| value.get(key))
                    .and_then(Value::as_str)
                    .map(str::to_string)
            };
            let number = |key: &str| {
                data.and_then(|value| value.get(key))
                    .and_then(Value::as_i64)
            };
            let reason = text("reason")
                .or_else(|| error.reason.clone())
                .unwrap_or_else(|| match error.code {
                    -32601 => "unknown_method".to_string(),
                    5001 => "storage_unavailable".to_string(),
                    _ => error.message.clone(),
                });
            let fields = data
                .and_then(|value| value.get("fields"))
                .and_then(Value::as_array)
                .map(|fields| {
                    fields
                        .iter()
                        .filter_map(Value::as_str)
                        .collect::<Vec<_>>()
                        .join(", ")
                })
                .filter(|joined| !joined.is_empty());
            CallFailure::Refused {
                reason,
                field: text("field"),
                detail: text("detail").or(fields),
                revision: number("revision"),
                execution_generation: number("execution_generation"),
            }
        }
        CallError::BadResult { .. } => CallFailure::Unreadable,
        CallError::TimedOut { .. } | CallError::Unknown { .. } => CallFailure::Unknown,
        CallError::NotConnected | CallError::Encode | CallError::InvalidTimeout => {
            CallFailure::NotSent(e.to_string())
        }
    }
}

/// Map the mutation sender's outcome for a rewind.
///
/// A rewind never asks for confirmation or defers at the pinned Hermes, so those outcomes are
/// unreadable here, which leads to a verifying read rather than a guess.
pub fn failure_from_mutation(o: MutationOutcome) -> Result<MutateOutcome, CallFailure> {
    match o {
        MutationOutcome::Committed(value) => {
            let rewound_count = value
                .get("rewound_count")
                .and_then(Value::as_u64)
                .ok_or(CallFailure::Unreadable)?;
            Ok(MutateOutcome {
                revision: value.get("revision").and_then(Value::as_i64),
                rewound_count,
                target_content: value
                    .get("target_message")
                    .and_then(|message| message.get("content"))
                    .cloned(),
            })
        }
        MutationOutcome::Refused {
            reason,
            field,
            detail,
            rpc_code,
        } => Err(CallFailure::Refused {
            reason: match rpc_code {
                -32601 => "unknown_method".to_string(),
                5001 => "storage_unavailable".to_string(),
                _ => reason,
            },
            field,
            detail,
            revision: None,
            execution_generation: None,
        }),
        MutationOutcome::FenceRefused {
            reason,
            revision,
            execution_generation,
        } => Err(CallFailure::Refused {
            reason,
            field: None,
            detail: None,
            revision,
            execution_generation,
        }),
        MutationOutcome::Unknown => Err(CallFailure::Unknown),
        MutationOutcome::NotSent { reason } => Err(CallFailure::NotSent(reason)),
        MutationOutcome::Unreadable
        | MutationOutcome::Deferred(_)
        | MutationOutcome::NeedsConfirmation(_)
        | MutationOutcome::OtherStatus { .. } => Err(CallFailure::Unreadable),
    }
}
