//! Helper slots that no longer match the main provider.

use super::{AuxSlot, StaleSlot};

pub fn stale_helpers(slots: &[AuxSlot], main_provider: &str) -> Vec<StaleSlot> {
    let _ = (slots, main_provider);
    todo!("model roles stub")
}
