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
    let Some(at) = current.iter().position(|row| row == id) else {
        return current.to_vec();
    };
    let last = current.len() - 1;
    let destination = match to {
        Placement::Up => at.saturating_sub(1),
        Placement::Down => (at + 1).min(last),
        Placement::Head => 0,
        Placement::Index(index) => index.min(last),
    };
    let mut target = current.to_vec();
    let row = target.remove(at);
    target.insert(destination, row);
    target
}

/// Ids to re-admit, in order, so the queue reaches `target`.
///
/// Issue 232. Queue and steer. Each re-admission sends that row to the back.
#[must_use]
pub fn plan_reorder(current: &[AdmissionId], target: &[AdmissionId]) -> Vec<AdmissionId> {
    // The rows already in target order, front to back, can stay where they are. Every
    // other row goes to the back in target order, which builds the rest of the target.
    let mut rest = current;
    let mut kept = 0;
    for wanted in target {
        match rest.iter().position(|row| row == wanted) {
            Some(index) => {
                rest = &rest[index + 1..];
                kept += 1;
            }
            None => break,
        }
    }
    target[kept..].to_vec()
}
