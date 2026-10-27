//! New profile name checks, in Hermes's order. Issue #253.

/// Why a typed name was refused. Checked in variant order.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum NameError {
    /// The trimmed input is empty.
    Empty,
    /// The normalised name misses the allowed shape.
    BadShape,
    /// The normalised name is reserved by Hermes.
    Reserved(String),
    /// The normalised name is already known.
    Exists(String),
}

impl std::fmt::Display for NameError {
    fn fmt(&self, _formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        todo!("profiles stub")
    }
}

/// Validate a typed name against the known names. Returns the normalised name.
/// Nothing is sent. Issue #253.
pub fn validate_new_name(_input: &str, _known: &[String]) -> Result<String, NameError> {
    todo!("profiles stub")
}
