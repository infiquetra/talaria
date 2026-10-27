//! The exact words the screen shows for each state. Issue #253.

use super::ServeState;

/// The label, the full text and the short next step for one state.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Explanation {
    /// The list label, such as `served`.
    pub label: &'static str,
    /// The full explanation the detail shows.
    pub text: String,
    /// The short fix the list and the command show.
    pub next_step: String,
}

/// Explain one state for one profile, character for character. Issue #253.
#[must_use]
pub fn explain(_state: &ServeState, _name: &str) -> Explanation {
    todo!("profiles stub")
}
