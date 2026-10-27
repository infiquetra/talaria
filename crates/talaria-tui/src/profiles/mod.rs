//! Profiles overlay rendering and key translation.

mod form;
mod render;

pub use form::cursor_position;
pub use render::{ProfilesRenderer, draw, list_page_size};

use talaria_core::profiles::KeyAction;

use crate::keys::{Action, Key, KeyCode, KeyContext, KeyKind, KeyMap};

/// Translate a key for the profiles overlay. Text fields reserve editing keys.
#[must_use]
pub fn key_action(map: &KeyMap, key: &Key, text_focus: bool) -> Option<KeyAction> {
    if key.kind == KeyKind::Release {
        return None;
    }
    let action = map.resolve(key, KeyContext::Profiles);
    if action == Some(Action::ProfilesBack) {
        return Some(KeyAction::Back);
    }
    if text_focus {
        if key.mods.alt || key.mods.super_key || key.mods.other {
            return None;
        }
        return match (key.code, key.mods.ctrl, key.mods.shift) {
            (KeyCode::Enter, true, _) => Some(KeyAction::CtrlEnter),
            (KeyCode::Enter, false, _) => Some(KeyAction::Enter),
            (KeyCode::Char('u' | 'U'), true, _) => Some(KeyAction::ClearLine),
            (KeyCode::Char(' '), false, _) => Some(KeyAction::Space),
            (KeyCode::Char(c), false, _) if !c.is_control() => Some(KeyAction::Char(c)),
            (KeyCode::Backspace, false, _) => Some(KeyAction::Backspace),
            (KeyCode::Left, false, _) => Some(KeyAction::Left),
            (KeyCode::Right, false, _) => Some(KeyAction::Right),
            (KeyCode::Home, false, _) => Some(KeyAction::Home),
            (KeyCode::End, false, _) => Some(KeyAction::End),
            (KeyCode::Tab, false, true) => Some(KeyAction::BackTab),
            (KeyCode::Tab, false, false) => Some(KeyAction::Tab),
            _ => None,
        };
    }
    match action? {
        Action::ProfilesUp => Some(KeyAction::Up),
        Action::ProfilesDown => Some(KeyAction::Down),
        Action::ProfilesPageUp => Some(KeyAction::PageUp),
        Action::ProfilesPageDown => Some(KeyAction::PageDown),
        Action::ProfilesFirst => Some(KeyAction::First),
        Action::ProfilesLast => Some(KeyAction::Last),
        Action::ProfilesOpen => Some(KeyAction::Open),
        Action::ProfilesFilter => Some(KeyAction::Filter),
        Action::ProfilesRefresh => Some(KeyAction::Refresh),
        Action::ProfilesRescan => Some(KeyAction::Rescan),
        Action::ProfilesNew => Some(KeyAction::New),
        Action::ProfilesDescription => Some(KeyAction::EditDescription),
        Action::ProfilesSoul => Some(KeyAction::EditSoul),
        Action::ProfilesBack => Some(KeyAction::Back),
        _ => None,
    }
}
