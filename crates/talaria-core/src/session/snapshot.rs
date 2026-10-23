//! `SessionState::from_snapshot`.

use hermes_contract::schemas::CanonicalSnapshot;

use super::SessionState;

pub(crate) fn from_snapshot(snapshot: CanonicalSnapshot) -> SessionState {
    let _ = snapshot;
    todo!("P1-05")
}
