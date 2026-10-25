//! Keys while a queued row is selected.
//!
//! Issue 232. Queue and steer.

use crossterm::event::KeyEvent;
use talaria_core::queue::{QueueIntent, QueueView};

use crate::keys::KeyMap;

/// The intent for one key in selection, or none when the key leaves selection to be typed.
///
/// Issue 232. Queue and steer.
#[must_use]
pub fn handle_selection_key(key: KeyEvent, view: &QueueView, keys: &KeyMap) -> Option<QueueIntent> {
    let _ = (key, view, keys);
    todo!("Issue 232. Queue and steer.")
}
