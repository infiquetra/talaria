//! Voice registration on the session mock. No handler is installed.

/// True when the mock answers voice methods. The stub does not.
#[must_use]
pub fn voice_handlers_registered() -> bool {
    false
}
