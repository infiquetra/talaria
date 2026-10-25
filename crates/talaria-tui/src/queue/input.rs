//! Keys while a queued row is selected.
//!
//! Issue 232. Queue and steer.

use crossterm::event::KeyEvent;
use talaria_core::queue::{Direction, QueueIntent, QueueView};

use crate::keys::{Action, Key, KeyCode, KeyContext, KeyKind, KeyMap};

/// The intent for one key in selection, or none when the key leaves selection to be typed.
///
/// Issue 232. Queue and steer. With no row selected only Up starts selection, and the caller
/// checks that the draft is empty and no card has focus first. Up past the oldest row and Down
/// past the newest give no intent: selection ends and the composer takes the key.
#[must_use]
pub fn handle_selection_key(key: KeyEvent, view: &QueueView, keys: &KeyMap) -> Option<QueueIntent> {
    let key = Key::from(key);
    if key.kind == KeyKind::Release {
        return None;
    }
    let action = keys.resolve_in(&key, KeyContext::QueueSelection);
    if view.editing.is_some() {
        // An edit owns the keys: only Esc is a queue key, and arrows move the caret.
        return (action == Some(Action::QueueLeave)).then_some(QueueIntent::CancelEdit);
    }
    let Some(selected) = view.selected else {
        let starts = key.code == KeyCode::Up && key.mods == Default::default();
        return (starts && !view.rows.is_empty()).then_some(QueueIntent::EnterSelection);
    };
    if let Some(action) = action {
        return match action {
            Action::QueueEdit => Some(QueueIntent::BeginEdit),
            Action::QueueRemove => Some(QueueIntent::Remove),
            Action::QueueSteer => Some(QueueIntent::Steer),
            Action::QueueSendNow => Some(QueueIntent::SendNow),
            Action::QueueMoveUp => Some(QueueIntent::Move(Direction::Up)),
            Action::QueueMoveDown => Some(QueueIntent::Move(Direction::Down)),
            Action::QueueLeave => Some(QueueIntent::Leave),
            _ => None,
        };
    }
    if key.mods != Default::default() {
        return None;
    }
    match key.code {
        KeyCode::Up if selected > 0 => Some(QueueIntent::Select(Direction::Up)),
        KeyCode::Down if selected + 1 < view.rows.len() => {
            Some(QueueIntent::Select(Direction::Down))
        }
        _ => None,
    }
}
