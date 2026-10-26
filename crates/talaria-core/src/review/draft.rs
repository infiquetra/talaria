//! Commit-message drafting errors. Re-exported by the app. Issue #245.

/// How many Unicode scalar values of diff the draft prompt carries at most.
pub const DRAFT_DIFF_MAX_CHARS: usize = 12_000;

/// Why no draft text came back.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DraftError {
    /// The target has no session.
    NoSession,
    /// The session has no model of its own.
    NoOwnRoute,
    /// The session no longer exists on the gateway.
    SessionGone,
    /// Hermes is busy drafting other messages.
    Busy,
    /// This Hermes does not offer message drafting.
    Unsupported,
    /// Drafting failed with a message.
    Failed(String),
    /// No reply within the wait.
    NoReply,
    /// Hermes is shutting down.
    Draining,
}
