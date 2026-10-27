//! Configuration assistant presentation.

mod draw;

pub use draw::draw_configure;

use crate::keys::{Key, KeyMap};
use talaria_core::configure::ConfigureKey;

/// Translate a key using the configuration assistant's bindings.
pub fn route_configure(_map: &KeyMap, _key: &Key) -> Option<ConfigureKey> {
    todo!("configure key routing")
}
