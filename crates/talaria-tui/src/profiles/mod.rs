//! Profiles overlay rendering and key translation.

mod form;
mod render;

pub use form::cursor_position;
pub use render::{ProfilesRenderer, draw, draw_with_theme, list_page_size};

use talaria_core::profiles::{KeyAction, KeyLabels};

use crate::keys::{Action, Key, KeyCode, KeyContext, KeyKind, KeyMap};

/// The labels of the keys bound to the profiles actions the hints name.
///
/// A plain letter reads in lower case and Shift with a letter as the capital, so the
/// default hints read `m models` and `R rescan`; any other chord keeps the keymap's
/// label, such as `Ctrl+E` or `F5`. An action with no key has no label.
#[must_use]
pub fn key_labels(map: &KeyMap) -> KeyLabels {
    let label = |action| {
        map.label(action).map(|label| {
            let letter = |text: &str| {
                let mut chars = text.chars();
                match (chars.next(), chars.next()) {
                    (Some(c), None) if c.is_alphabetic() => Some(c),
                    _ => None,
                }
            };
            if let Some(c) = letter(&label) {
                c.to_lowercase().collect()
            } else if let Some(c) = label.strip_prefix("Shift+").and_then(letter) {
                c.to_uppercase().collect()
            } else {
                label
            }
        })
    };
    KeyLabels {
        open: label(Action::ProfilesOpen),
        models: label(Action::ProfilesModels),
        connect: label(Action::ProfilesConnect),
        refresh: label(Action::ProfilesRefresh),
        rescan: label(Action::ProfilesRescan),
        new: label(Action::ProfilesNew),
        delete: label(Action::ProfilesDelete),
        rename: label(Action::ProfilesRename),
        description: label(Action::ProfilesDescription),
        soul: label(Action::ProfilesSoul),
    }
}

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
        Action::ProfilesDelete => Some(KeyAction::Delete),
        Action::ProfilesRename => Some(KeyAction::Rename),
        Action::ProfilesDescription => Some(KeyAction::EditDescription),
        Action::ProfilesSoul => Some(KeyAction::EditSoul),
        Action::ProfilesModels => Some(KeyAction::Models),
        Action::ProfilesConnect => Some(KeyAction::Connect),
        Action::ProfilesBack => Some(KeyAction::Back),
        _ => None,
    }
}
