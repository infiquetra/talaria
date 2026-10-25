//! Notice text for a refused queue call, and the short preview of a prompt.
//!
//! Issue 232. Queue and steer.

use super::QueueAction;

/// The notice for a refusal, or none when this layer does not map that pair.
///
/// Issue 232. Queue and steer.
#[must_use]
pub fn refusal_notice(action: QueueAction, reason: &str) -> Option<&'static str> {
    let _ = (action, reason);
    todo!("Issue 232. Queue and steer.")
}

/// First line of `text`, cut to 60 characters with an ellipsis.
///
/// Issue 232. Queue and steer.
#[must_use]
pub fn preview(text: &str) -> String {
    let _ = text;
    todo!("Issue 232. Queue and steer.")
}
