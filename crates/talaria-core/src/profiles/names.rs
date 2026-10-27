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
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Empty => formatter.write_str("Enter a name."),
            Self::BadShape => formatter.write_str("Use lower-case letters, digits, - or _, starting with a letter or digit, up to 64 characters."),
            Self::Reserved(name) => write!(formatter, "{name} is reserved by Hermes. Choose another name."),
            Self::Exists(name) => write!(formatter, "A profile named {name} already exists."),
        }
    }
}

/// Validate a typed name against the known names. Returns the normalised name.
/// Nothing is sent. Issue #253.
pub fn validate_new_name(input: &str, known: &[String]) -> Result<String, NameError> {
    let name = super::normalize_profile_name(input);
    if name.is_empty() {
        return Err(NameError::Empty);
    }
    if name.len() > 64
        || !name
            .bytes()
            .next()
            .is_some_and(|b| b.is_ascii_lowercase() || b.is_ascii_digit())
        || !name
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-' || b == b'_')
    {
        return Err(NameError::BadShape);
    }
    if ["hermes", "default", "test", "tmp", "root", "sudo"].contains(&name.as_str()) {
        return Err(NameError::Reserved(name));
    }
    if known
        .iter()
        .any(|value| super::normalize_profile_name(value) == name)
    {
        return Err(NameError::Exists(name));
    }
    Ok(name)
}
