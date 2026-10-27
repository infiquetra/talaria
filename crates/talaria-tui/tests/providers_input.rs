//! Secret field and the providers key map. The field stores nothing and the map binds nothing.

use talaria_core::providers::ProvidersKey;
use talaria_tui::keys::{KeyChord, KeyContext, KeyMap, ProvidersAction};
use talaria_tui::providers::{SecretField, SecretOutcome};

#[test]
fn secret_field_editing() {
    let mut field = SecretField::new();
    assert_eq!(
        field.value().capacity(),
        65_536,
        "the buffer is reserved up front"
    );

    assert_eq!(
        field.handle_key(&ProvidersKey::Char('a')),
        SecretOutcome::Edited { empty: false },
        "a typed character was not stored"
    );
    assert_eq!(field.len(), 1);
    assert_eq!(
        field.handle_key(&ProvidersKey::Backspace),
        SecretOutcome::Edited { empty: true }
    );
    assert_eq!(field.len(), 0);

    assert_eq!(
        field.handle_paste("  sk-a b\r\n"),
        SecretOutcome::Edited { empty: false },
        "paste was not stored"
    );
    assert_eq!(field.value().as_str(), "sk-a b");
    assert_eq!(field.len(), 6);

    assert_eq!(field.handle_key(&ProvidersKey::CtrlU), SecretOutcome::Clear);
    assert_eq!(field.len(), 0);
    assert_eq!(
        field.handle_key(&ProvidersKey::Enter),
        SecretOutcome::NotHandled
    );
    assert_ne!(
        field.handle_key(&ProvidersKey::Enter),
        SecretOutcome::Submit
    );

    for ch in "abcdefghij".chars() {
        let _ = field.handle_key(&ProvidersKey::Char(ch));
    }
    assert_eq!(
        field.value().capacity(),
        65_536,
        "typing grew the reserved buffer"
    );
}

#[test]
fn providers_keymap_defaults() {
    let map = KeyMap::defaults();
    let expected = [
        (ProvidersAction::NextTab, "providers_next_tab", "tab"),
        (ProvidersAction::PrevTab, "providers_prev_tab", "shift+tab"),
        (ProvidersAction::Reload, "providers_reload", "ctrl+r"),
        (ProvidersAction::Remove, "providers_remove", "ctrl+d"),
        (ProvidersAction::Test, "providers_test", "ctrl+t"),
        (ProvidersAction::Save, "providers_save", "ctrl+s"),
        (ProvidersAction::ClearKey, "providers_clear_key", "ctrl+x"),
        (ProvidersAction::OpenLink, "providers_open_link", "o"),
        (ProvidersAction::Copy, "providers_copy", "c"),
    ];
    assert_eq!(ProvidersAction::ALL.len(), expected.len());
    for (action, name, chord_text) in expected {
        assert_eq!(action.config_name(), name);
        assert_eq!(action.default_chord(), chord_text);
        let chord = KeyChord::parse(chord_text, KeyContext::Providers).expect("the chord parses");
        let key = chord.to_key();
        assert_eq!(
            map.resolve_providers(&key),
            Some(action),
            "{name} is not bound"
        );
        assert!(
            !map.providers_chords(action).is_empty(),
            "{name} has no chord"
        );
    }
}
