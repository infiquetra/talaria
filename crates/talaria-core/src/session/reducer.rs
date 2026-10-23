//! The pure transition.

use super::{Effect, Event, SessionState};

/// Apply one event. Total: it does not panic on a well-typed [`Event`], and it reads no clock.
pub fn update(state: &mut SessionState, event: Event) -> Vec<Effect> {
    let _ = (state, event);
    todo!("P1-05")
}
