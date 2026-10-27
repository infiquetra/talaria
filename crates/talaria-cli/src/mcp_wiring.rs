use std::collections::BTreeMap;

use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use talaria_core::mcp::{McpAction, McpEffect, McpInput, McpKey, McpView, SecretSlot};
use talaria_core::providers::ProvidersKey;
use talaria_tui::keys::{Action, Key, KeyContext, KeyMap};
use talaria_tui::providers::{SecretField, SecretOutcome};
use zeroize::Zeroizing;

#[derive(Default)]
pub struct McpWiring {
    fields: BTreeMap<SecretSlot, SecretField>,
}

impl McpWiring {
    pub fn route_key(&mut self, view: &McpView, key: KeyEvent) -> Option<McpInput> {
        self.route_key_with_map(view, key, &KeyMap::defaults())
    }

    pub fn route_key_with_map(
        &mut self,
        view: &McpView,
        key: KeyEvent,
        map: &KeyMap,
    ) -> Option<McpInput> {
        if matches!(key.kind, KeyEventKind::Release) {
            return None;
        }
        let control = key.modifiers.contains(KeyModifiers::CONTROL);
        if control && key.code == KeyCode::Char('c') {
            return Some(McpInput::Close);
        }
        if let Some(slot) = view.secret_focus {
            let secret_key = match (key.code, control) {
                (KeyCode::Char('u'), true) => Some(ProvidersKey::CtrlU),
                (KeyCode::Char(ch), false)
                    if !key
                        .modifiers
                        .intersects(KeyModifiers::ALT | KeyModifiers::SUPER) =>
                {
                    Some(ProvidersKey::Char(ch))
                }
                (KeyCode::Backspace, false) => Some(ProvidersKey::Backspace),
                (KeyCode::Enter, false) => Some(ProvidersKey::Enter),
                _ => None,
            };
            if let Some(secret_key) = secret_key {
                let field = self.fields.entry(slot).or_default();
                return match field.handle_key(&secret_key) {
                    SecretOutcome::Edited { empty } => Some(McpInput::SecretEdited { slot, empty }),
                    SecretOutcome::Submit => Some(McpInput::SecretSubmitted { slot }),
                    SecretOutcome::Clear => Some(McpInput::SecretEdited { slot, empty: true }),
                    SecretOutcome::NotHandled if key.code == KeyCode::Enter => {
                        Some(McpInput::SecretSubmitted { slot })
                    }
                    SecretOutcome::NotHandled => None,
                };
            }
        }
        if let Some(action) = map.resolve(&Key::from(key), KeyContext::Mcp) {
            let action = match action {
                Action::McpNextTab => McpAction::NextTab,
                Action::McpPrevTab => McpAction::PrevTab,
                Action::McpReload => McpAction::Reload,
                Action::McpAdd => McpAction::Add,
                Action::McpTest => McpAction::Test,
                Action::McpToggle => McpAction::Toggle,
                Action::McpSetKey => McpAction::SetKey,
                Action::McpSignIn => McpAction::SignIn,
                Action::McpRemove => McpAction::Remove,
                Action::McpApply => McpAction::Apply,
                Action::McpSave => McpAction::Save,
                Action::McpOpenLink => McpAction::OpenLink,
                Action::McpCopy => McpAction::Copy,
                Action::McpPasteRedirect => McpAction::PasteRedirect,
                _ => return None,
            };
            return Some(McpInput::Key(McpKey::Action(action)));
        }
        let mapped = match (key.code, control) {
            (KeyCode::Char(ch), false)
                if !key
                    .modifiers
                    .intersects(KeyModifiers::ALT | KeyModifiers::SUPER) =>
            {
                McpKey::Char(ch)
            }
            (KeyCode::Tab, false) if key.modifiers.contains(KeyModifiers::SHIFT) => McpKey::BackTab,
            (KeyCode::Tab, false) => McpKey::Tab,
            (KeyCode::BackTab, false) => McpKey::BackTab,
            (KeyCode::Enter, false) => McpKey::Enter,
            (KeyCode::Esc, false) => McpKey::Esc,
            (KeyCode::Up, false) => McpKey::Up,
            (KeyCode::Down, false) => McpKey::Down,
            (KeyCode::PageUp, false) => McpKey::PageUp,
            (KeyCode::PageDown, false) => McpKey::PageDown,
            (KeyCode::Home, false) => McpKey::Home,
            (KeyCode::End, false) => McpKey::End,
            (KeyCode::Left, false) => McpKey::Left,
            (KeyCode::Right, false) => McpKey::Right,
            (KeyCode::Backspace, false) => McpKey::Backspace,
            _ => return None,
        };
        Some(McpInput::Key(mapped))
    }

    pub fn route_paste(&mut self, view: &McpView, text: &str) -> Option<McpInput> {
        if let Some(slot) = view.secret_focus {
            return match self.fields.entry(slot).or_default().handle_paste(text) {
                SecretOutcome::Edited { empty } => Some(McpInput::SecretEdited { slot, empty }),
                _ => None,
            };
        }
        Some(McpInput::Key(McpKey::Paste(text.to_owned())))
    }

    #[must_use]
    pub fn secret_for(&self, effect: &McpEffect) -> Option<Zeroizing<String>> {
        let field = self.fields.get(&effect.secret_slot()?)?;
        Some(Zeroizing::new(field.value().to_string()))
    }

    pub fn forget(&mut self) {
        for field in self.fields.values_mut() {
            field.clear();
        }
        self.fields.clear();
    }

    pub fn forget_slot(&mut self, slot: SecretSlot) {
        self.fields.remove(&slot);
    }

    #[must_use]
    pub fn secret_len(&self, slot: SecretSlot) -> usize {
        self.fields.get(&slot).map_or(0, SecretField::len)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn view(focus: Option<SecretSlot>) -> McpView {
        let mut view = talaria_core::mcp::view(
            &talaria_core::mcp::McpState::default(),
            std::time::Instant::now(),
        );
        view.secret_focus = focus;
        view
    }

    #[test]
    fn configured_binding_replaces_the_default_inside_the_overlay() {
        let (map, notices) = KeyMap::from_entries(&BTreeMap::from([(
            "mcp_apply".to_owned(),
            vec!["ctrl+f".to_owned()],
        )]));
        assert!(notices.is_empty());
        let mut wiring = McpWiring::default();
        assert!(matches!(
            wiring.route_key_with_map(
                &view(None),
                KeyEvent::new(KeyCode::Char('f'), KeyModifiers::CONTROL),
                &map
            ),
            Some(McpInput::Key(McpKey::Action(McpAction::Apply)))
        ));
        assert!(
            wiring
                .route_key_with_map(
                    &view(None),
                    KeyEvent::new(KeyCode::Char('y'), KeyModifiers::CONTROL),
                    &map
                )
                .is_none()
        );
    }

    #[test]
    fn secret_characters_take_priority_over_letter_actions() {
        let mut wiring = McpWiring::default();
        assert!(matches!(
            wiring.route_key(
                &view(Some(SecretSlot::Token)),
                KeyEvent::new(KeyCode::Char('c'), KeyModifiers::NONE)
            ),
            Some(McpInput::SecretEdited {
                slot: SecretSlot::Token,
                empty: false
            })
        ));
        assert_eq!(wiring.secret_len(SecretSlot::Token), 1);
    }

    #[test]
    fn empty_secret_submit_reaches_field_validation() {
        assert!(matches!(
            McpWiring::default().route_key(
                &view(Some(SecretSlot::KeyValue)),
                KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)
            ),
            Some(McpInput::SecretSubmitted {
                slot: SecretSlot::KeyValue
            })
        ));
    }
}
