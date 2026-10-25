//! Pure reorder plans. Callers pass the ids of queued rows only.
//!
//! Issue 232. Queue and steer.

use super::AdmissionId;

/// Where one row should land.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Placement {
    /// One place toward the head.
    Up,
    /// One place toward the tail.
    Down,
    /// The first place.
    Head,
    /// This index. Past the end means last.
    Index(usize),
}

/// The order after moving `id`. The input is returned when the move would not change it.
///
/// Issue 232. Queue and steer.
#[must_use]
pub fn move_target(current: &[AdmissionId], id: &AdmissionId, to: Placement) -> Vec<AdmissionId> {
    let _ = (current, id, to);
    todo!("Issue 232. Queue and steer.")
}

/// Ids to re-admit, in order, so the queue reaches `target`.
///
/// Issue 232. Queue and steer. Each re-admission sends that row to the back.
#[must_use]
pub fn plan_reorder(current: &[AdmissionId], target: &[AdmissionId]) -> Vec<AdmissionId> {
    let _ = (current, target);
    todo!("Issue 232. Queue and steer.")
}
