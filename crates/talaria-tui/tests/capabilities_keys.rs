use std::collections::BTreeMap;

use talaria_core::capabilities::{CapInput, CapTab, CapView, KeyAction};
use talaria_core::commands::local::LocalAction;
use talaria_core::settings::SETTINGS_LINKS;
use talaria_tui::capabilities::route_capabilities;
use talaria_tui::keys::{Key, KeyChord, KeyContext, KeyMap};

fn key(chord: &str) -> Key {
    KeyChord::parse(chord, KeyContext::Capabilities)
        .unwrap()
        .to_key()
}

#[test]
fn defaults_and_rebinding() {
    typed_input_uses_fixed_text_keys_before_rebindings();

    let map = KeyMap::defaults();
    let view = CapView::default();
    for (chord, action) in [
        ("tab", KeyAction::NextTab),
        ("shift+tab", KeyAction::PrevTab),
        ("1", KeyAction::SelectTab(CapTab::Skills)),
        ("2", KeyAction::SelectTab(CapTab::Hub)),
        ("3", KeyAction::SelectTab(CapTab::Toolsets)),
        ("4", KeyAction::SelectTab(CapTab::Plugins)),
        ("up", KeyAction::Up),
        ("down", KeyAction::Down),
        ("pageup", KeyAction::PageUp),
        ("pagedown", KeyAction::PageDown),
        ("home", KeyAction::Home),
        ("end", KeyAction::End),
        ("enter", KeyAction::Enter),
        ("space", KeyAction::Toggle),
        ("/", KeyAction::Filter),
        ("n", KeyAction::NextPage),
        ("p", KeyAction::PrevPage),
        ("i", KeyAction::Install),
        ("c", KeyAction::Catalog),
        ("x", KeyAction::Remove),
        ("a", KeyAction::ShowAll),
        ("r", KeyAction::Refresh),
        ("y", KeyAction::Confirm),
        ("esc", KeyAction::Cancel),
    ] {
        assert_eq!(
            map.resolve_capabilities(KeyContext::Capabilities, &key(chord)),
            Some(action.clone()),
            "default {chord}"
        );
        assert_eq!(
            route_capabilities(&view, &map, &key(chord)),
            Some(CapInput::Key(action))
        );
    }
    for chord in ["ctrl+b", "ctrl+s", "ctrl+r", "ctrl+c", "f1", "f2"] {
        assert_eq!(
            map.resolve_capabilities(KeyContext::Capabilities, &key(chord)),
            None
        );
    }
    let entries = BTreeMap::from([("capabilities_down".into(), vec!["j".into()])]);
    let (rebound, notices) = KeyMap::from_entries(&entries);
    assert!(notices.is_empty(), "{notices:?}");
    assert_eq!(
        route_capabilities(&view, &rebound, &key("j")),
        Some(CapInput::Key(KeyAction::Down))
    );
    assert_eq!(route_capabilities(&view, &rebound, &key("down")), None);
    assert!(
        SETTINGS_LINKS
            .iter()
            .any(|link| link.label == "Skills, toolsets and plugins"
                && link.action == LocalAction::Capabilities)
    );
}

fn typed_input_uses_fixed_text_keys_before_rebindings() {
    let (map, _) = KeyMap::from_entries(&BTreeMap::from([
        ("capabilities_down".into(), vec!["j".into()]),
        ("capabilities_refresh".into(), vec!["enter".into()]),
    ]));
    for prefix in ["Filter: ", "Search: ", "Install: "] {
        let view = CapView {
            text_focus: true,
            input: Some(prefix.into()),
            ..Default::default()
        };
        for character in ['j', 'n', 'i', 'y', '1', ' ', '/', 'é'] {
            let chord = if character == ' ' {
                "space".into()
            } else {
                character.to_string()
            };
            assert_eq!(
                route_capabilities(&view, &map, &key(&chord)),
                Some(CapInput::Char(character))
            );
        }
        assert_eq!(
            route_capabilities(&view, &map, &key("backspace")),
            Some(CapInput::Backspace)
        );
        assert_eq!(
            route_capabilities(&view, &map, &key("enter")),
            Some(CapInput::Key(KeyAction::Enter))
        );
        assert_eq!(
            route_capabilities(&view, &map, &key("esc")),
            Some(CapInput::Key(KeyAction::Cancel))
        );
        for ignored in [
            "up",
            "down",
            "tab",
            "shift+tab",
            "home",
            "end",
            "pageup",
            "pagedown",
            "ctrl+b",
            "ctrl+s",
        ] {
            assert_eq!(
                route_capabilities(&view, &map, &key(ignored)),
                None,
                "{prefix}: {ignored}"
            );
        }
    }
}
