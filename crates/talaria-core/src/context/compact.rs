//! Compaction requests and replies (#234).

use crate::mutation::MutationOutcome;

/// Build the compress payload: trim the argument; `""` gives `{}`.
pub fn compact_payload(_args: &str) -> serde_json::Map<String, serde_json::Value> {
    unimplemented!("#234")
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

/// Read a mutation outcome as a compaction result.
pub fn interpret(_outcome: &MutationOutcome, _sid: &str) -> CompactResult {
    unimplemented!("#234")
}

/// The refusal text for a reason, with the session id for `not_found`.
pub fn refusal_text(_reason: &str, _field: Option<&str>, _sid: &str) -> String {
    unimplemented!("#234")
}

/// How the ledger should settle the outcome: a compression-lock refusal
/// settles as `Unknown` so the request stays kept for a resend.
pub fn settle_as(_outcome: &MutationOutcome) -> MutationOutcome {
    unimplemented!("#234")
}

/// How a compaction call ended, for the tracker.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CompactEnd {
    Committed { revision: i64 },
    Unknown,
    NothingChanged,
}

/// Read how the call ended from its outcome.
pub fn compact_end(_outcome: &MutationOutcome) -> CompactEnd {
    unimplemented!("#234")
}
