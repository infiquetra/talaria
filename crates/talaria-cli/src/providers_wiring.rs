//! Routes provider keys into the secret field and lends that field to the service.

use talaria_app::providers::ProviderService;
use talaria_core::providers::{ProvidersEffect, ProvidersInput, ProvidersKey, ProvidersView};
use talaria_tui::providers::SecretField;
use zeroize::Zeroizing;

/// The overlay's one secret field.
pub struct ProvidersWiring {
    field: SecretField,
}

impl ProvidersWiring {
    /// An empty field with its buffer reserved.
    #[must_use]
    pub fn new() -> Self {
        Self {
            field: SecretField::new(),
        }
    }

    /// Give a key to the field while it is focused. The stub always hands the key to core.
    pub fn route_key(&mut self, view: &ProvidersView, key: ProvidersKey) -> ProvidersInput {
        let _ = (view, &self.field);
        ProvidersInput::Key(key)
    }

    /// Give pasted text to the focused field. The stub accepts none.
    pub fn route_paste(&mut self, view: &ProvidersView, text: &str) -> Option<ProvidersInput> {
        let _ = (view, text, &self.field);
        None
    }

    /// A copy of the field for the one call that needs it. The stub lends nothing.
    pub fn secret_for(&self, effect: &ProvidersEffect) -> Option<Zeroizing<String>> {
        let _ = (effect, &self.field);
        None
    }

    /// Zero the field.
    pub fn forget(&mut self) {
        self.field.clear();
    }

    /// How many characters the field holds.
    #[must_use]
    pub fn secret_len(&self) -> usize {
        self.field.len()
    }
}

impl Default for ProvidersWiring {
    fn default() -> Self {
        Self::new()
    }
}

/// Run one effect. The overlay loop calls this; the tests name it so the seam stays visible.
#[allow(dead_code)]
pub async fn run_effect(
    service: &ProviderService,
    effect: ProvidersEffect,
    secret: Option<Zeroizing<String>>,
) -> Vec<ProvidersInput> {
    service.execute(effect, secret).await
}

#[cfg(test)]
mod tests {
    use std::time::Instant;

    use proptest::prelude::*;
    use ratatui::buffer::Buffer;
    use ratatui::layout::Rect;
    use talaria_core::providers::{
        ApiKeyChange, CardKind, CardView, EndpointDraft, ProvidersEffect, ProvidersInput,
        ProvidersKey, ProvidersState, ProvidersTab, ProvidersView, update, view,
    };
    use talaria_tui::providers::draw_providers;

    use super::{ProvidersWiring, run_effect};

    const SENTINEL: &str = "sk-test-SENTINEL-0123456789abcdef";

    fn focus(on: bool) -> ProvidersView {
        ProvidersView {
            title: "Providers · profile fixture".into(),
            tab: ProvidersTab::Keys,
            rows: vec!["OpenRouter".into()],
            filter: String::new(),
            selected: Some(0),
            card: Some(CardView {
                kind: CardKind::Key,
                lines: vec!["API key for OpenRouter".into()],
                footer: "enter save · ctrl+u clear · esc cancel".into(),
            }),
            notice: None,
            banner: None,
            secret_focus: on,
            footer: String::new(),
        }
    }

    fn typed_draft() -> EndpointDraft {
        EndpointDraft {
            id: String::new(),
            name: "example-endpoint".into(),
            base_url: "https://example.invalid/v1".into(),
            model: "example-model".into(),
            api_key: ApiKeyChange::Typed,
            api_mode: None,
            context_length: None,
            discover_models: true,
            models: None,
            model_details: None,
        }
    }

    fn kept_draft(change: ApiKeyChange) -> EndpointDraft {
        let mut draft = typed_draft();
        draft.api_key = change;
        draft
    }

    fn buffer_text(buf: &Buffer) -> String {
        (0..buf.area.height)
            .map(|y| {
                (0..buf.area.width)
                    .map(|x| buf[(x, y)].symbol())
                    .collect::<String>()
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    proptest! {
        #[test]
        fn key_never_leaves_the_secret_field(secret in "sk-test-[a-z0-9]{12,24}") {
            let _ = run_effect;
            let mut wiring = ProvidersWiring::new();
            let shown = focus(true);
            let mut state = ProvidersState::new();
            let mut dumped = String::new();
            let now = Instant::now();
            for ch in secret.chars() {
                let input = wiring.route_key(&shown, ProvidersKey::Char(ch));
                let effects = update(&mut state, input.clone(), now);
                dumped.push_str(&format!(
                    "{input:?} {effects:?} {state:?} {:?}\n",
                    view(&state, now)
                ));
            }
            let pasted = wiring.route_paste(&shown, &secret);
            dumped.push_str(&format!("{pasted:?}\n"));
            let cleared = wiring.route_key(&shown, ProvidersKey::CtrlU);
            dumped.push_str(&format!("{cleared:?}\n"));
            for ch in secret.chars() {
                let _ = wiring.route_key(&shown, ProvidersKey::Char(ch));
            }
            let check = ProvidersEffect::CheckKey {
                env_var: "OPENROUTER_API_KEY".into(),
            };
            let save = ProvidersEffect::SaveKey {
                slug: "openrouter".into(),
            };
            let copied = wiring.secret_for(&check);
            prop_assert_eq!(
                copied.as_ref().map(|value| value.as_str()),
                Some(secret.as_str())
            );
            let _ = wiring.secret_for(&save);
            wiring.forget();
            prop_assert_eq!(wiring.secret_len(), 0);
            let area = Rect::new(0, 0, 80, 24);
            let mut buf = Buffer::empty(area);
            draw_providers(&shown, wiring.secret_len(), area, &mut buf);
            dumped.push_str(&buffer_text(&buf));
            dumped.push_str(&format!("{:?} {:?}", view(&state, now), state));
            prop_assert!(!dumped.contains(secret.as_str()));
        }
    }

    #[test]
    fn only_four_effects_get_the_key() {
        let _ = run_effect;
        let mut wiring = ProvidersWiring::new();
        let shown = focus(true);
        let first = wiring.route_key(&shown, ProvidersKey::Char('s'));
        assert!(
            matches!(first, ProvidersInput::SecretEdited { empty: false }),
            "a focused key reached the overlay: {first:?}"
        );
        for ch in SENTINEL.chars().skip(1) {
            let _ = wiring.route_key(&shown, ProvidersKey::Char(ch));
        }
        let pasted = wiring.route_paste(&shown, SENTINEL);
        assert!(
            matches!(pasted, Some(ProvidersInput::SecretEdited { empty: false })),
            "focused paste did not reach the field: {pasted:?}"
        );

        let idle = focus(false);
        let passed = wiring.route_key(&idle, ProvidersKey::Char('z'));
        assert!(
            matches!(passed, ProvidersInput::Key(ProvidersKey::Char('z'))),
            "an unfocused key was swallowed: {passed:?}"
        );
        assert!(wiring.route_paste(&idle, SENTINEL).is_none());

        let typed = typed_draft();
        let kept = kept_draft(ApiKeyChange::Keep);
        let cleared = kept_draft(ApiKeyChange::Clear);
        let effects = [
            ProvidersEffect::CheckKey {
                env_var: "OPENROUTER_API_KEY".into(),
            },
            ProvidersEffect::SaveKey {
                slug: "openrouter".into(),
            },
            ProvidersEffect::CheckEndpoint(typed.clone()),
            ProvidersEffect::SaveEndpoint(typed),
            ProvidersEffect::CheckEndpoint(kept.clone()),
            ProvidersEffect::SaveEndpoint(kept),
            ProvidersEffect::CheckEndpoint(cleared.clone()),
            ProvidersEffect::SaveEndpoint(cleared),
            ProvidersEffect::LoadEnv,
            ProvidersEffect::LoadOptions,
            ProvidersEffect::LoadOauth,
            ProvidersEffect::LoadEndpoints,
            ProvidersEffect::ForgetSecret,
            ProvidersEffect::Disconnect {
                slug: "openrouter".into(),
            },
            ProvidersEffect::StartSignIn {
                id: "nous".into(),
                attempt: 1,
            },
            ProvidersEffect::Poll {
                id: "nous".into(),
                session_id: "sess-1".into(),
                after: std::time::Duration::from_secs(2),
            },
            ProvidersEffect::CancelSignIn {
                session_id: "sess-1".into(),
            },
            ProvidersEffect::SignOut { id: "nous".into() },
            ProvidersEffect::DeleteEndpoint { id: "demo".into() },
            ProvidersEffect::OpenUrl("https://example.invalid/device".into()),
            ProvidersEffect::CopyText("ABCD-1234".into()),
            ProvidersEffect::Close,
        ];
        for effect in &effects {
            let got = wiring.secret_for(effect);
            let wants = match effect {
                ProvidersEffect::CheckKey { .. } | ProvidersEffect::SaveKey { .. } => true,
                ProvidersEffect::CheckEndpoint(draft) | ProvidersEffect::SaveEndpoint(draft) => {
                    draft.api_key == ApiKeyChange::Typed
                }
                _ => false,
            };
            if wants {
                assert_eq!(
                    got.as_ref().map(|value| value.as_str()),
                    Some(SENTINEL),
                    "the key was not lent for {effect:?}"
                );
            } else {
                assert!(got.is_none(), "the key was lent for {effect:?}");
            }
        }
        wiring.forget();
        assert_eq!(wiring.secret_len(), 0);
    }
}
