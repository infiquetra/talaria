//! Compaction requests and replies (#234).

use serde_json::Value;

use super::text::{T_C8, T_C9, T_C13, t_c14};
use crate::mutation::MutationOutcome;

/// Build the compress payload: trim the argument; `""` gives `{}`.
pub fn compact_payload(args: &str) -> serde_json::Map<String, serde_json::Value> {
    let trimmed = args.trim();
    let mut payload = serde_json::Map::new();
    if !trimmed.is_empty() {
        payload.insert("focus".to_owned(), Value::String(trimmed.to_owned()));
    }
    payload
}

/// The summary Hermes returns with a compaction result.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Summary {
    pub headline: String,
    pub token_line: Option<String>,
    pub note: Option<String>,
    pub noop: bool,
    pub aborted: bool,
    pub refused_would_grow: bool,
    pub fallback_used: bool,
}

/// What a compaction call means for the view.
#[derive(Clone, Debug, PartialEq)]
pub enum CompactResult {
    Preview {
        lines: Vec<String>,
    },
    NoChange {
        summary: Option<Summary>,
    },
    Compacted {
        summary: Option<Summary>,
        message_count: Option<u64>,
        revision: i64,
    },
    Refused {
        text: String,
    },
    Unknown,
    NotSent {
        text: String,
    },
    UnknownStatus {
        status: String,
    },
    Unreadable,
}

/// The summary of a reply, when it carries one.
fn summary_of(result: &Value) -> Option<Summary> {
    let summary = result.get("summary")?;
    let headline = summary.get("headline")?.as_str()?.to_owned();
    Some(Summary {
        headline,
        token_line: summary
            .get("token_line")
            .and_then(Value::as_str)
            .filter(|line| !line.is_empty())
            .map(str::to_owned),
        note: summary
            .get("note")
            .and_then(Value::as_str)
            .filter(|note| !note.is_empty())
            .map(str::to_owned),
        noop: summary
            .get("noop")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        aborted: summary
            .get("aborted")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        refused_would_grow: summary
            .get("refused_would_grow")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        fallback_used: summary
            .get("fallback_used")
            .and_then(Value::as_bool)
            .unwrap_or(false),
    })
}

/// The preview lines of a reply.
fn preview_lines(result: &Value) -> Vec<String> {
    result
        .get("lines")
        .and_then(Value::as_array)
        .map(|lines| {
            lines
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_owned)
                .collect()
        })
        .unwrap_or_default()
}

/// Read a mutation outcome as a compaction result.
pub fn interpret(outcome: &MutationOutcome, sid: &str) -> CompactResult {
    match outcome {
        MutationOutcome::Committed(receipt) => match receipt["revision"].as_i64() {
            Some(revision) => CompactResult::Compacted {
                summary: summary_of(receipt),
                message_count: receipt["message_count"]
                    .as_i64()
                    .filter(|count| *count >= 0)
                    .and_then(|count| u64::try_from(count).ok()),
                revision,
            },
            None => CompactResult::Unreadable,
        },
        MutationOutcome::Deferred(_) => CompactResult::UnknownStatus {
            status: "deferred".to_owned(),
        },
        MutationOutcome::NeedsConfirmation(_) => CompactResult::UnknownStatus {
            status: "confirmation_required".to_owned(),
        },
        MutationOutcome::OtherStatus { status, result } => match status.as_str() {
            "preview" => CompactResult::Preview {
                lines: preview_lines(result),
            },
            "noop" => CompactResult::NoChange {
                summary: summary_of(result),
            },
            other => CompactResult::UnknownStatus {
                status: other.to_owned(),
            },
        },
        MutationOutcome::Refused { reason, field, .. } => CompactResult::Refused {
            text: refusal_text(reason, field.as_deref(), sid),
        },
        MutationOutcome::FenceRefused { .. } => CompactResult::Refused {
            text:
                "The session changed while the compaction was being prepared. Run /compact again."
                    .to_owned(),
        },
        MutationOutcome::Unknown => CompactResult::Unknown,
        MutationOutcome::NotSent { reason } => CompactResult::NotSent {
            text: if reason == "fences unknown" {
                T_C9.to_owned()
            } else {
                t_c14(reason)
            },
        },
        MutationOutcome::Unreadable => CompactResult::Unreadable,
    }
}

/// The refusal text for a reason, with the session id for `not_found`.
pub fn refusal_text(reason: &str, field: Option<&str>, sid: &str) -> String {
    match reason {
        "session_busy" if field == Some("compression") => {
            "Hermes is already compacting this conversation. Wait for it to finish, then run the \
             same /compact line again."
                .to_owned()
        }
        "session_busy" => {
            "Hermes cannot compact while the agent is working. Wait for the reply to finish, or \
             press Esc to stop it, then run /compact again."
                .to_owned()
        }
        "unknown_execution" => {
            "Hermes cannot compact while the outcome of the last turn is unknown. Nothing was \
             changed."
                .to_owned()
        }
        "compression_checkpoint_required" => {
            "This profile requires a memory checkpoint before compaction \
             (compression.checkpoint_required), and none is available. Nothing was changed."
                .to_owned()
        }
        "runtime_coordination_required" => {
            "Hermes cannot compact this session from this connection. Nothing was changed."
                .to_owned()
        }
        "unsupported_compress_options" => {
            "Hermes does not support --aggressive. Nothing was changed.".to_owned()
        }
        "revision_conflict" | "stale_generation" => {
            "The session changed while the compaction was being prepared. Run /compact again."
                .to_owned()
        }
        "not_found" => {
            format!(
                "Hermes does not have this session loaded. Resume it with talaria --resume {sid}."
            )
        }
        "permission_denied" => {
            "Hermes refused the compaction: this connection may not change this session.".to_owned()
        }
        other => format!("Hermes refused the compaction: {other}."),
    }
}

/// How the ledger should settle the outcome: a compression-lock refusal
/// settles as `Unknown` so the request stays kept for a resend.
pub fn settle_as(outcome: &MutationOutcome) -> MutationOutcome {
    match outcome {
        MutationOutcome::Refused {
            reason,
            field: Some(field),
            ..
        } if reason == "session_busy" && field == "compression" => MutationOutcome::Unknown,
        other => other.clone(),
    }
}

/// How a compaction call ended, for the tracker.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CompactEnd {
    Committed { revision: i64 },
    Unknown,
    NothingChanged,
}

/// Read how the call ended from its outcome.
pub fn compact_end(outcome: &MutationOutcome) -> CompactEnd {
    if matches!(settle_as(outcome), MutationOutcome::Unknown) {
        return CompactEnd::Unknown;
    }
    match interpret(outcome, "") {
        CompactResult::Compacted { revision, .. } => CompactEnd::Committed { revision },
        CompactResult::Unreadable => CompactEnd::Unknown,
        _ => CompactEnd::NothingChanged,
    }
}

/// The text of a refusal that never reached Hermes.
pub fn already_running_text() -> String {
    T_C8.to_owned()
}

/// The text of a call whose answer never came.
pub fn unknown_text() -> String {
    T_C13.to_owned()
}
