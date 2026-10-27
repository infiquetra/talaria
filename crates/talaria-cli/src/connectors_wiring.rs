//! Key routing and secret custody for the connectors overlay.
use std::{collections::BTreeMap, sync::Mutex};
use talaria_core::{
    connectors::{ConnectorsEffect, ConnectorsInput, ConnectorsKey},
    providers::ProvidersKey,
};
use talaria_tui::{
    keys::{Key, KeyCode, KeyKind},
    providers::{SecretField, SecretOutcome},
};
use zeroize::Zeroizing;

#[derive(Default)]
struct Secrets {
    fields: BTreeMap<String, SecretField>,
    pending: BTreeMap<String, Zeroizing<String>>,
    active: Option<String>,
}

/// Routes overlay keys and holds typed secrets locally.
#[derive(Default)]
pub struct ConnectorsWiring {
    secrets: Mutex<Secrets>,
}
impl ConnectorsWiring {
    #[must_use]
    pub fn route_key(&self, key: ConnectorsKey) -> ConnectorsInput {
        ConnectorsInput::Key(key)
    }
    #[must_use]
    pub fn route_paste(&self, text: String) -> ConnectorsInput {
        ConnectorsInput::Paste(text)
    }
    /// Select the local secret field before sending raw editor keys to it.
    pub fn edit_secret(&self, key: String) {
        let mut secrets = self
            .secrets
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if let Some(active) = secrets.active.take() {
            secrets.fields.remove(&active);
        }
        secrets.fields.insert(key.clone(), SecretField::new());
        secrets.active = Some(key);
    }
    /// Commit a nonempty editor buffer to the pending draft.
    #[must_use]
    pub fn accept_secret_edit(&self) -> bool {
        let mut secrets = self
            .secrets
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let Some(key) = secrets.active.clone() else {
            return false;
        };
        let Some(field) = secrets.fields.get(&key) else {
            return false;
        };
        if field.is_empty() {
            return false;
        }
        let value = field.value().clone();
        secrets.pending.insert(key.clone(), value);
        secrets.fields.remove(&key);
        secrets.active = None;
        true
    }
    /// Discard the editor buffer while retaining the previously accepted draft.
    pub fn cancel_secret_edit(&self) {
        let mut secrets = self
            .secrets
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if let Some(key) = secrets.active.take() {
            secrets.fields.remove(&key);
        }
    }
    /// Edit the selected field without sending its characters into domain state.
    #[must_use]
    pub fn route_secret_key(&self, key: &Key) -> Option<ConnectorsInput> {
        if key.kind == KeyKind::Release {
            return None;
        }
        let plain = !key.mods.ctrl && !key.mods.alt && !key.mods.super_key && !key.mods.other;
        let provider_key = match key.code {
            KeyCode::Char(ch) if plain => ProvidersKey::Char(ch),
            KeyCode::Backspace if plain => ProvidersKey::Backspace,
            KeyCode::Char('u')
                if key.mods.ctrl && !key.mods.alt && !key.mods.super_key && !key.mods.other =>
            {
                ProvidersKey::CtrlU
            }
            _ => return None,
        };
        self.edit(|field| field.handle_key(&provider_key))
    }
    /// Paste into the local secret field.
    #[must_use]
    pub fn route_secret_paste(&self, text: &str) -> Option<ConnectorsInput> {
        self.edit(|field| field.handle_paste(text))
    }
    fn edit(
        &self,
        apply: impl FnOnce(&mut SecretField) -> SecretOutcome,
    ) -> Option<ConnectorsInput> {
        let mut secrets = self
            .secrets
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let key = secrets.active.clone()?;
        let field = secrets.fields.get_mut(&key)?;
        match apply(field) {
            SecretOutcome::Edited { empty } => Some(ConnectorsInput::SecretEdited { key, empty }),
            SecretOutcome::Clear => Some(ConnectorsInput::SecretEdited { key, empty: true }),
            SecretOutcome::Submit | SecretOutcome::NotHandled => None,
        }
    }
    #[must_use]
    pub fn secrets_for(
        &self,
        effect: &ConnectorsEffect,
    ) -> Option<BTreeMap<String, Zeroizing<String>>> {
        let ConnectorsEffect::UpdatePlatform { typed_keys, .. } = effect else {
            return None;
        };
        let secrets = self
            .secrets
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let mut result = BTreeMap::new();
        for key in typed_keys {
            if let Some(value) = secrets.pending.get(key) {
                result.insert(key.clone(), value.clone());
            }
        }
        Some(result)
    }
    pub fn forget(&self) {
        let mut secrets = self
            .secrets
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        secrets.fields.clear();
        secrets.pending.clear();
        secrets.active = None;
    }
    #[must_use]
    pub fn secret_lens(&self) -> BTreeMap<String, usize> {
        let secrets = self
            .secrets
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let mut lengths: BTreeMap<String, usize> = secrets
            .pending
            .iter()
            .map(|(key, value)| (key.clone(), value.chars().count()))
            .collect();
        for (key, field) in &secrets.fields {
            if !field.is_empty() || secrets.active.as_ref() == Some(key) {
                lengths.insert(key.clone(), field.len());
            }
        }
        lengths
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn save() -> ConnectorsEffect {
        ConnectorsEffect::UpdatePlatform {
            id: "signal".into(),
            enabled: None,
            typed_keys: vec!["TOKEN".into()],
            lists: BTreeMap::new(),
            clear_env: Vec::new(),
        }
    }

    #[test]
    fn only_accepted_values_are_saved_and_canceled_replacements_are_discarded() {
        let wiring = ConnectorsWiring::default();
        wiring.edit_secret("TOKEN".into());
        assert!(!wiring.accept_secret_edit());
        assert!(wiring.route_secret_paste("first").is_some());
        assert!(wiring.accept_secret_edit());
        wiring.edit_secret("TOKEN".into());
        assert_eq!(wiring.secret_lens()["TOKEN"], 0);
        assert!(wiring.route_secret_paste("canceled").is_some());
        assert_eq!(
            wiring.secrets_for(&save()).unwrap()["TOKEN"].as_str(),
            "first"
        );
        wiring.cancel_secret_edit();
        assert_eq!(
            wiring.secrets_for(&save()).unwrap()["TOKEN"].as_str(),
            "first"
        );
        wiring.edit_secret("TOKEN".into());
        assert!(wiring.route_secret_paste("newer").is_some());
        assert_eq!(
            wiring.secrets_for(&save()).unwrap()["TOKEN"].as_str(),
            "first"
        );
        assert!(wiring.accept_secret_edit());
        assert_eq!(
            wiring.secrets_for(&save()).unwrap()["TOKEN"].as_str(),
            "newer"
        );
        assert_eq!(
            wiring.secrets_for(&save()).unwrap()["TOKEN"].as_str(),
            "newer"
        );
        wiring.forget();
        assert!(wiring.secret_lens().is_empty());
        assert!(wiring.secrets_for(&save()).unwrap().is_empty());
    }
}
