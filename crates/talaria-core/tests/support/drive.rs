//! Apply a sequence of events, starting from an empty session unless a snapshot is given.
//!
//! Each integration test is its own crate, so a helper used by one binary is dead in the others.

#![allow(dead_code)]

use hermes_contract::schemas::CanonicalSnapshot;
use talaria_core::session::{Effect, Event, SessionId, SessionState, update};

/// `SessionState::empty` then [`update`] for each event.
pub fn reduce(session_id: &str, events: Vec<Event>) -> (SessionState, Vec<Effect>) {
    let mut state = SessionState::empty(SessionId::new(session_id));
    apply_all(&mut state, events)
}

/// `SessionState::from_snapshot` then [`update`] for each event.
pub fn reduce_snapshot(
    snapshot: CanonicalSnapshot,
    events: Vec<Event>,
) -> (SessionState, Vec<Effect>) {
    let mut state = SessionState::from_snapshot(snapshot);
    apply_all(&mut state, events)
}

fn apply_all(state: &mut SessionState, events: Vec<Event>) -> (SessionState, Vec<Effect>) {
    let mut effects = Vec::new();
    for event in events {
        effects.extend(update(state, event));
    }
    (state.clone(), effects)
}
