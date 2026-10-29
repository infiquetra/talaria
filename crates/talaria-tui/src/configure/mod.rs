//! Configuration assistant presentation.

mod draw;

pub use draw::{detail_scroll_target, draw_configure};
pub use talaria_core::configure::ConfigureKey;

use crate::keys::{Key, KeyCode, KeyKind, KeyMap};

/// Translate a key using the configuration assistant's bindings.
pub fn route_configure(map: &KeyMap, key: &Key) -> Option<ConfigureKey> {
    if key.kind == KeyKind::Release {
        return None;
    }
    map.resolve_configure(key).or_else(|| {
        if key.mods.ctrl || key.mods.alt || key.mods.super_key || key.mods.other {
            return None;
        }
        match key.code {
            KeyCode::Enter => Some(ConfigureKey::Enter),
            KeyCode::Backspace => Some(ConfigureKey::Backspace),
            _ => None,
        }
    })
}
