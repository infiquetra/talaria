//! The masked card: a value that is never shown.

use zeroize::Zeroizing;

/// A typed secret. No `Debug`, `Display`, `Clone` or `Serialize`.
pub struct MaskedValue(Zeroizing<String>);

impl MaskedValue {
    pub(super) fn new() -> Self {
        Self(Zeroizing::new(String::with_capacity(65_536)))
    }

    pub(super) fn push(&mut self, ch: char) {
        self.0.push(ch);
    }

    pub(super) fn pop(&mut self) {
        self.0.pop();
    }

    pub fn len_chars(&self) -> usize {
        self.0.chars().count()
    }

    pub fn into_inner(self) -> Zeroizing<String> {
        self.0
    }
}

/// What the card needs to know about one open masked request.
#[derive(Clone, Debug)]
pub struct MaskedView {
    pub request_id: String,
    pub session_id: Option<String>,
    pub method: String,
    /// `command`, `env_var`, `prompt`, `site`, `hint`, `backend`, `display_name`.
    pub fields: Vec<(String, String)>,
}
