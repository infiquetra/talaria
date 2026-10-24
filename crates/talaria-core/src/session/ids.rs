//! Session, cell and prompt identifiers.

/// Hermes session id carried on an event and on locally built prompt keys.
#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub struct SessionId(String);

impl SessionId {
    /// Build an id from the wire string. An empty string is a different session from any real one.
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }

    /// Borrow the wire string.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl From<&str> for SessionId {
    fn from(id: &str) -> Self {
        Self::new(id)
    }
}

impl From<String> for SessionId {
    fn from(id: String) -> Self {
        Self::new(id)
    }
}

/// Cell identity inside one [`super::SessionState`]. Never reused.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub struct CellId(u64);

impl CellId {
    /// The reducer's next id. Tests do not mint ids.
    pub const fn new(id: u64) -> Self {
        Self(id)
    }

    /// The numeric id.
    pub const fn get(self) -> u64 {
        self.0
    }
}

/// Approval and clarification identity. The generation is part of equality.
#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub struct PromptKey {
    /// Session the prompt belongs to.
    pub session_id: SessionId,
    /// Hermes `prompt_id`.
    pub prompt_id: String,
    /// Hermes `execution_generation` of that prompt.
    pub execution_generation: i64,
}
