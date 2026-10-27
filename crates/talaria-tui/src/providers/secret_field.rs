//! One typed secret. This stub keeps the reserved buffer and stores nothing.

use talaria_core::providers::ProvidersKey;
use zeroize::Zeroizing;

/// What the secret field did with a key.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SecretOutcome {
    /// The text changed.
    Edited {
        /// Whether the field is empty.
        empty: bool,
    },
    /// Enter with at least one character.
    Submit,
    /// The field was cleared.
    Clear,
    /// The field did not take the key.
    NotHandled,
}

/// A secret buffer. It has no `Debug`, `Clone` or `Display`.
pub struct SecretField {
    buf: Zeroizing<String>,
}

impl SecretField {
    /// Reserve 65,536 bytes. The stub never grows the buffer and never stores input.
    #[must_use]
    pub fn new() -> Self {
        Self {
            buf: Zeroizing::new(String::with_capacity(65_536)),
        }
    }

    /// Take a key. The stub stores nothing.
    pub fn handle_key(&mut self, key: &ProvidersKey) -> SecretOutcome {
        let _ = key;
        SecretOutcome::NotHandled
    }

    /// Take pasted text. The stub stores nothing.
    pub fn handle_paste(&mut self, text: &str) -> SecretOutcome {
        let _ = text;
        SecretOutcome::NotHandled
    }

    /// Character count.
    #[must_use]
    pub fn len(&self) -> usize {
        self.buf.chars().count()
    }

    /// Whether the field is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.buf.is_empty()
    }

    /// Borrow the buffer.
    #[must_use]
    pub fn value(&self) -> &Zeroizing<String> {
        &self.buf
    }

    /// Zero the stored bytes and keep the capacity.
    pub fn clear(&mut self) {
        self.buf.clear();
    }
}

impl Default for SecretField {
    fn default() -> Self {
        Self::new()
    }
}
