mod render;

pub use render::draw_capabilities;

use crate::keys::{Key, KeyCode, KeyContext, KeyKind, KeyMap};
pub use talaria_core::capabilities::{CapInput, CapTab, CapView, KeyAction};

/// Translate a key into an overlay input, giving text entry its fixed keys.
pub fn route_capabilities(view: &CapView, map: &KeyMap, key: &Key) -> Option<CapInput> {
    if key.kind == KeyKind::Release {
        return None;
    }
    if view.text_focus {
        if key.mods.ctrl || key.mods.alt || key.mods.super_key || key.mods.other {
            return None;
        }
        return match key.code {
            KeyCode::Char(c) if !c.is_control() => Some(CapInput::Char(c)),
            KeyCode::Backspace if !key.mods.shift => Some(CapInput::Backspace),
            KeyCode::Enter if !key.mods.shift => Some(CapInput::Key(KeyAction::Enter)),
            KeyCode::Esc if !key.mods.shift => Some(CapInput::Key(KeyAction::Cancel)),
            _ => None,
        };
    }
    if view.hints == "y confirm  any other key cancels" {
        return Some(CapInput::Key(
            if map.resolve_capabilities(KeyContext::Capabilities, key) == Some(KeyAction::Confirm) {
                KeyAction::Confirm
            } else {
                KeyAction::Cancel
            },
        ));
    }
    map.resolve_capabilities(KeyContext::Capabilities, key)
        .map(CapInput::Key)
}
