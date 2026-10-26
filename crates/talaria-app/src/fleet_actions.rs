//! Run one fleet-view effect. The stub returns no inputs and opens nothing.

use talaria_core::directory::rows::RowKey;
use talaria_core::fleet_view::{FleetViewEffect, FleetViewInput};

use crate::app::AppState;

/// Run one effect. A reply or a stop whose peek is still opening waits for that open.
/// This stub runs nothing and returns no inputs.
pub async fn execute(app: &mut AppState, effect: FleetViewEffect) -> Vec<FleetViewInput> {
    let _ = (app, effect);
    Vec::new()
}

/// Open the overlay the way `/fleet` does, then place the cursor on `key`.
/// This stub does nothing.
pub async fn open_fleet_view_at(app: &mut AppState, key: RowKey) {
    let _ = (app, key);
}
