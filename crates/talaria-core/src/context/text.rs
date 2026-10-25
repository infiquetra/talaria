//! Read errors for the context and usage commands (#234).

/// Why a context or usage read failed. The core never sees the client error.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ReadError {
    /// The gateway does not serve the method.
    MethodMissing,
    /// The session is not loaded.
    NotFound,
    /// The connection may not read the session.
    PermissionDenied,
    /// Any other refusal, with Hermes's reason.
    Refused { reason: String },
    /// No answer before the deadline, rounded up to whole seconds.
    TimedOut { secs: u64 },
    /// Not connected, or the connection dropped.
    NotConnected,
    /// The reply arrived but does not decode.
    Unreadable,
    /// Nothing was sent: a Talaria bug.
    NotSent,
}

impl ReadError {
    /// The full error row for a command, with its two-space indent.
    pub fn row(&self, _command: &str, _method: &str, _sid: &str) -> String {
        unimplemented!("#234")
    }

    /// The short text used inside notes.
    pub fn short(&self) -> String {
        unimplemented!("#234")
    }
}
