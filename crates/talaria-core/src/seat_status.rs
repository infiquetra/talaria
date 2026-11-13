//! Foreground seat state, independent of terminal reporting.

/// What the foreground session is doing or how its last turn ended.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum SeatState {
    /// No completed turn, or the last turn was interrupted.
    Idle,
    /// A turn is running without an open need.
    Working,
    /// An open need is waiting for a response, with its kind when known.
    Blocked(Option<BlockKind>),
    /// The last turn completed normally.
    Done,
    /// The last turn failed.
    Error,
}

/// What response an open need requires.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum BlockKind {
    /// Approval of an action.
    Permission,
    /// An answer to a clarification question.
    Question,
    /// A password, secret, vault code, or vault unlock.
    Auth,
}
