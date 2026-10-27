//! Key routing and the secret lens for the connectors overlay.
//!
//! Typed characters stay in this process. The core sees a key name and whether the field is empty.

use std::collections::BTreeMap;

use talaria_core::connectors::{ConnectorsEffect, ConnectorsInput, ConnectorsKey};
use zeroize::Zeroizing;

/// Routes keys and holds typed secrets for one overlay.
#[derive(Default)]
pub struct ConnectorsWiring;

impl ConnectorsWiring {
    /// Turn a connectors key into an input.
    #[must_use]
    pub fn route_key(&self, key: ConnectorsKey) -> ConnectorsInput {
        ConnectorsInput::Key(key)
    }

    /// Paste into the filter, a name prompt, or the list editor.
    #[must_use]
    pub fn route_paste(&self, text: String) -> ConnectorsInput {
        ConnectorsInput::Paste(text)
    }

    /// Secrets for a platform save. Nothing is stored until a later change fills this.
    #[must_use]
    pub fn secrets_for(
        &self,
        effect: &ConnectorsEffect,
    ) -> Option<BTreeMap<String, Zeroizing<String>>> {
        let _ = effect;
        None
    }

    /// Drop anything typed.
    pub fn forget(&self) {}

    /// Lengths of typed secrets, for the masked editor. Empty until a field is typed.
    #[must_use]
    pub fn secret_lens(&self) -> BTreeMap<String, usize> {
        BTreeMap::new()
    }
}
