//! Prompt registry transitions. [`super::update`] is the only caller.

use super::{Effect, Event, SessionState};

#[allow(dead_code)]
pub(crate) fn apply(state: &mut SessionState, event: &Event) -> Vec<Effect> {
    let _ = (state, event);
    todo!("P1-05")
}
