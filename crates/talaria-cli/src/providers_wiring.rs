//! Routes provider input while keeping key values outside application state.

use talaria_core::providers::{
    ApiKeyChange, CardKind, ProvidersEffect, ProvidersInput, ProvidersKey, ProvidersView,
};
use talaria_tui::keys::{Action, Key, KeyCode, KeyContext, KeyKind, KeyMap};
use talaria_tui::providers::{SecretField, SecretOutcome};
use zeroize::{Zeroize, Zeroizing};

#[must_use]
pub fn key_for(view: &ProvidersView, map: &KeyMap, key: Key) -> Option<ProvidersKey> {
    if key.kind == KeyKind::Release {
        return None;
    }
    let plain = !key.mods.ctrl && !key.mods.alt && !key.mods.super_key && !key.mods.other;
    if key.mods.ctrl && !key.mods.alt && !key.mods.super_key && key.code == KeyCode::Char('u') {
        return Some(ProvidersKey::CtrlU);
    }
    if plain && let KeyCode::Char(ch) = key.code {
        let locked_key_field = !view.secret_focus
            && view.card.as_ref().is_some_and(|card| {
                card.kind == CardKind::EndpointForm
                    && card
                        .lines
                        .get(2)
                        .is_some_and(|line| line.trim_start().starts_with('›'))
            });
        if locked_key_field {
            return None;
        }
        if view.secret_focus
            || view.card.is_none()
            || view
                .card
                .as_ref()
                .is_some_and(|card| card.kind == CardKind::EndpointForm)
        {
            return Some(ProvidersKey::Char(ch));
        }
        if matches!(ch, 'y' | 'n')
            && view.card.as_ref().is_some_and(|card| {
                matches!(
                    card.kind,
                    CardKind::RemoveConfirm | CardKind::SignOutConfirm | CardKind::DeleteConfirm
                )
            })
        {
            return Some(ProvidersKey::Char(ch));
        }
    }
    if view.card.is_some() && key.code == KeyCode::Tab {
        return Some(if key.mods.shift {
            ProvidersKey::BackTab
        } else {
            ProvidersKey::Tab
        });
    }
    let action = map.resolve(&key, KeyContext::Providers).and_then(|action| {
        Some(match action {
            Action::ProvidersNextTab => talaria_core::providers::ProvidersAction::NextTab,
            Action::ProvidersPrevTab => talaria_core::providers::ProvidersAction::PrevTab,
            Action::ProvidersReload => talaria_core::providers::ProvidersAction::Reload,
            Action::ProvidersRemove => talaria_core::providers::ProvidersAction::Remove,
            Action::ProvidersTest => talaria_core::providers::ProvidersAction::Test,
            Action::ProvidersSave => talaria_core::providers::ProvidersAction::Save,
            Action::ProvidersClearKey => talaria_core::providers::ProvidersAction::ClearKey,
            Action::ProvidersOpenLink => talaria_core::providers::ProvidersAction::OpenLink,
            Action::ProvidersCopy => talaria_core::providers::ProvidersAction::Copy,
            _ => return None,
        })
    });
    if let Some(action) = action {
        return Some(ProvidersKey::Action(action));
    }
    if !plain {
        return None;
    }
    Some(match key.code {
        KeyCode::Enter => ProvidersKey::Enter,
        KeyCode::Esc => ProvidersKey::Esc,
        KeyCode::Backspace => ProvidersKey::Backspace,
        KeyCode::Up => ProvidersKey::Up,
        KeyCode::Down => ProvidersKey::Down,
        KeyCode::PageUp => ProvidersKey::PageUp,
        KeyCode::PageDown => ProvidersKey::PageDown,
        KeyCode::Home => ProvidersKey::Home,
        KeyCode::End => ProvidersKey::End,
        KeyCode::Left => ProvidersKey::Left,
        KeyCode::Right => ProvidersKey::Right,
        _ => return None,
    })
}

#[derive(Default)]
pub struct ProvidersWiring {
    field: SecretField,
}

impl ProvidersWiring {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn route_key(&mut self, view: &ProvidersView, key: ProvidersKey) -> ProvidersInput {
        if view.secret_focus {
            let outcome = if let ProvidersKey::Paste(mut text) = key {
                let outcome = self.field.handle_paste(&text);
                text.zeroize();
                return field_input(outcome).unwrap_or(ProvidersInput::SecretEdited {
                    empty: self.field.is_empty(),
                });
            } else {
                self.field.handle_key(&key)
            };
            if let Some(input) = field_input(outcome) {
                return input;
            }
            if matches!(key, ProvidersKey::Char(_)) {
                return ProvidersInput::SecretEdited {
                    empty: self.field.is_empty(),
                };
            }
        }
        ProvidersInput::Key(key)
    }

    pub fn route_paste(&mut self, view: &ProvidersView, text: &str) -> Option<ProvidersInput> {
        if view.secret_focus {
            return field_input(self.field.handle_paste(text));
        }
        let card = view.card.as_ref()?;
        if card.kind == CardKind::EndpointForm
            && card
                .lines
                .iter()
                .take(6)
                .enumerate()
                .any(|(index, line)| index != 2 && line.trim_start().starts_with('›'))
        {
            Some(ProvidersInput::Key(ProvidersKey::Paste(text.to_owned())))
        } else {
            None
        }
    }

    #[must_use]
    pub fn secret_for(&self, effect: &ProvidersEffect) -> Option<Zeroizing<String>> {
        let needs_key = match effect {
            ProvidersEffect::CheckKey { .. } | ProvidersEffect::SaveKey { .. } => true,
            ProvidersEffect::CheckEndpoint(draft) | ProvidersEffect::SaveEndpoint(draft) => {
                draft.api_key == ApiKeyChange::Typed
            }
            _ => false,
        };
        if !needs_key {
            return None;
        }
        let mut owned = Zeroizing::new(String::with_capacity(self.field.value().len()));
        owned.push_str(self.field.value());
        Some(owned)
    }

    pub fn forget(&mut self) {
        self.field.clear();
    }

    #[must_use]
    pub fn secret_len(&self) -> usize {
        self.field.len()
    }

    pub fn draw(
        &self,
        view: &ProvidersView,
        area: ratatui::layout::Rect,
        buffer: &mut ratatui::buffer::Buffer,
    ) {
        talaria_tui::providers::draw_providers(view, self.secret_len(), area, buffer);
    }

    pub fn copy<W: std::io::Write>(
        &mut self,
        terminal: &mut talaria_tui::terminal::InlineTerminal<W>,
        text: &str,
    ) -> std::io::Result<()> {
        terminal.copy_to_clipboard(text)
    }
}

fn field_input(outcome: SecretOutcome) -> Option<ProvidersInput> {
    match outcome {
        SecretOutcome::Edited { empty } => Some(ProvidersInput::SecretEdited { empty }),
        SecretOutcome::Submit => Some(ProvidersInput::SecretSubmitted),
        SecretOutcome::Clear => Some(ProvidersInput::SecretCleared),
        SecretOutcome::NotHandled => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;
    use ratatui::{buffer::Buffer, layout::Rect};
    use std::time::{Duration, Instant};
    use talaria_core::providers::{self, EndpointDraft, OpenTarget, ProvidersState, ProvidersTab};

    fn check_domain(
        wiring: &mut ProvidersWiring,
        state: &mut ProvidersState,
        input: ProvidersInput,
        now: Instant,
        secret: &str,
    ) -> Vec<ProvidersEffect> {
        assert!(!format!("{input:?}").contains(secret));
        let effects = providers::update(state, input, now);
        for effect in &effects {
            assert!(!format!("{effect:?}").contains(secret));
            let _copy = wiring.secret_for(effect);
            if matches!(effect, ProvidersEffect::ForgetSecret) {
                wiring.forget();
                assert_eq!(wiring.secret_len(), 0);
            }
        }
        let view = providers::view(state, now);
        assert!(!format!("{state:?} {view:?}").contains(secret));
        let area = Rect::new(0, 0, 100, 24);
        let mut buffer = Buffer::empty(area);
        talaria_tui::providers::draw_providers(&view, wiring.secret_len(), area, &mut buffer);
        let drawn: String = buffer.content.iter().map(|cell| cell.symbol()).collect();
        assert!(!drawn.contains(secret));
        effects
    }

    fn key_card(wiring: &mut ProvidersWiring, now: Instant, secret: &str) -> ProvidersState {
        let mut state = ProvidersState::default();
        check_domain(
            wiring,
            &mut state,
            ProvidersInput::Open {
                target: OpenTarget::Provider {
                    slug: "example".into(),
                },
                profile_name: "fixture".into(),
                connected: true,
            },
            now,
            secret,
        );
        check_domain(
            wiring,
            &mut state,
            ProvidersInput::EnvLoaded(Ok(vec![providers::EnvRow {
                env_var: "EXAMPLE_API_KEY".into(),
                slug: "example".into(),
                label: "Example".into(),
                primary: true,
                is_password: true,
                ..Default::default()
            }])),
            now,
            secret,
        );
        let options = serde_json::from_value(serde_json::json!({"providers": [{
            "slug":"example", "name":"Example", "models":[], "authenticated":false,
            "auth_type":"api_key", "key_env":"EXAMPLE_API_KEY"
        }]}))
        .expect("valid catalogue fixture");
        check_domain(
            wiring,
            &mut state,
            ProvidersInput::OptionsLoaded(Ok(options)),
            now,
            secret,
        );
        check_domain(
            wiring,
            &mut state,
            ProvidersInput::OauthLoaded(Ok(Vec::new())),
            now,
            secret,
        );
        assert!(providers::view(&state, now).secret_focus);
        state
    }

    proptest! {
        #[test]
        fn keys_stay_outside_domain_and_rendered_output(suffix in "[a-zA-Z0-9]{16,96}") {
            let secret = format!("credential_{suffix}");
            let now = Instant::now();
            let mut wiring = ProvidersWiring::new();
            let mut state = key_card(&mut wiring, now, &secret);
            for ch in secret.chars() {
                let view = providers::view(&state, now);
                let input = wiring.route_key(&view, ProvidersKey::Char(ch));
                assert!(matches!(input, ProvidersInput::SecretEdited { .. }));
                check_domain(&mut wiring, &mut state, input, now, &secret);
            }
            let view = providers::view(&state, now);
            let clear = wiring.route_key(&view, ProvidersKey::CtrlU);
            check_domain(&mut wiring, &mut state, clear, now, &secret);
            assert_eq!(wiring.secret_len(), 0);
            let input = wiring.route_paste(&providers::view(&state, now), &secret).expect("key field takes paste");
            check_domain(&mut wiring, &mut state, input, now, &secret);
            let input = wiring.route_key(&providers::view(&state, now), ProvidersKey::Enter);
            let effects = check_domain(&mut wiring, &mut state, input, now, &secret);
            assert!(effects.iter().any(|effect| matches!(effect, ProvidersEffect::CheckKey { .. })));
            let effects = check_domain(&mut wiring, &mut state, ProvidersInput::KeyChecked(Ok(providers::KeyCheck {
                ok: true, reachable: true, message: String::new(),
            })), now, &secret);
            assert!(effects.iter().any(|effect| matches!(effect, ProvidersEffect::SaveKey { .. })));
            let saved = serde_json::from_value(serde_json::json!({"slug":"example", "name":"Example", "models":[], "authenticated":true})).expect("saved provider fixture");
            check_domain(&mut wiring, &mut state, ProvidersInput::KeySaved(Ok(saved)), now, &secret);
            assert_eq!(wiring.secret_len(), 0);
            check_domain(&mut wiring, &mut state, ProvidersInput::Close, now, &secret);
            state = key_card(&mut wiring, now, &secret);
            let input = wiring.route_paste(&providers::view(&state, now), &secret).expect("key field takes paste");
            check_domain(&mut wiring, &mut state, input, now, &secret);
            let input = wiring.route_key(&providers::view(&state, now), ProvidersKey::Esc);
            check_domain(&mut wiring, &mut state, input, now, &secret);
            assert_eq!(wiring.secret_len(), 0);
            check_domain(&mut wiring, &mut state, ProvidersInput::Close, now, &secret);
        }
    }

    #[test]
    fn rejected_secret_text_never_reaches_domain_input() {
        let now = Instant::now();
        let mut wiring = ProvidersWiring::new();
        let state = key_card(&mut wiring, now, "credential_probe_0123456789");
        let view = providers::view(&state, now);
        assert!(wiring.route_paste(&view, &"x".repeat(65_536)).is_some());
        for key in [
            ProvidersKey::Char('z'),
            ProvidersKey::Char('\u{001b}'),
            ProvidersKey::Paste("credential_overflow_0123456789".into()),
        ] {
            assert!(matches!(
                wiring.route_key(&view, key),
                ProvidersInput::SecretEdited { empty: false }
            ));
            assert_eq!(wiring.secret_len(), 65_536);
        }
    }

    #[test]
    fn lends_secrets_only_to_the_four_secret_effects() {
        let mut wiring = ProvidersWiring::new();
        let now = Instant::now();
        let secret = "credential_example_0123456789";
        let mut state = key_card(&mut wiring, now, secret);
        let input = wiring
            .route_paste(&providers::view(&state, now), secret)
            .expect("focused field");
        check_domain(&mut wiring, &mut state, input, now, secret);
        let mut draft = EndpointDraft::default();
        let plain = draft.clone();
        draft.api_key = ApiKeyChange::Typed;
        for effect in [
            ProvidersEffect::CheckKey {
                env_var: "KEY".into(),
            },
            ProvidersEffect::SaveKey {
                slug: "example".into(),
            },
            ProvidersEffect::CheckEndpoint(draft.clone()),
            ProvidersEffect::SaveEndpoint(draft),
        ] {
            assert_eq!(
                wiring.secret_for(&effect).expect("secret effect").as_str(),
                secret
            );
        }
        for effect in [
            ProvidersEffect::LoadEnv,
            ProvidersEffect::LoadOptions,
            ProvidersEffect::LoadOauth,
            ProvidersEffect::LoadEndpoints,
            ProvidersEffect::ForgetSecret,
            ProvidersEffect::Disconnect {
                slug: "example".into(),
            },
            ProvidersEffect::StartSignIn {
                id: "example".into(),
                attempt: 1,
            },
            ProvidersEffect::Poll {
                id: "example".into(),
                session_id: "session".into(),
                after: Duration::from_secs(2),
            },
            ProvidersEffect::CancelSignIn {
                session_id: "session".into(),
            },
            ProvidersEffect::SignOut {
                id: "example".into(),
            },
            ProvidersEffect::CheckEndpoint(plain.clone()),
            ProvidersEffect::SaveEndpoint(plain),
            ProvidersEffect::DeleteEndpoint {
                id: "example".into(),
            },
            ProvidersEffect::OpenUrl("https://example.invalid".into()),
            ProvidersEffect::CopyText("device-code".into()),
            ProvidersEffect::Close,
        ] {
            assert!(wiring.secret_for(&effect).is_none());
        }
        let cleared = EndpointDraft {
            api_key: ApiKeyChange::Clear,
            ..Default::default()
        };
        for effect in [
            ProvidersEffect::CheckEndpoint(cleared.clone()),
            ProvidersEffect::SaveEndpoint(cleared),
        ] {
            assert!(wiring.secret_for(&effect).is_none());
        }
        let closed = providers::view(&ProvidersState::default(), now);
        assert!(matches!(
            wiring.route_key(&closed, ProvidersKey::Char('x')),
            ProvidersInput::Key(ProvidersKey::Char('x'))
        ));
        assert!(wiring.route_paste(&closed, secret).is_none());
        wiring.forget();
        assert_eq!(wiring.secret_len(), 0);
        assert_eq!(closed.tab, ProvidersTab::Keys);
    }
}
