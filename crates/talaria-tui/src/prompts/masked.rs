//! The masked card: a value that is never shown.

use zeroize::Zeroizing;

/// A typed secret. No `Debug`, `Display`, `Clone` or `Serialize`.
pub struct MaskedValue(Zeroizing<String>);

impl MaskedValue {
    pub fn len_chars(&self) -> usize {
        let _ = &self.0;
        todo!("prompts")
    }

    pub fn into_inner(self) -> Zeroizing<String> {
        todo!("prompts")
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
