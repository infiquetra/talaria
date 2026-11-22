use std::collections::BTreeMap;
use talaria_core::settings::{Screen, SettingsAction, SettingsKey, SettingsView};
use talaria_tui::comfort_route::{Focus, Route, route};
use talaria_tui::keys::{Action, Key, KeyChord, KeyContext, KeyMap};
use talaria_tui::settings::route_settings;

fn key(chord: &str) -> Key {
    KeyChord::parse(chord, KeyContext::Settings)
        .unwrap()
        .to_key()
}

fn view(screen: Screen, text_focus: bool) -> SettingsView {
    SettingsView {
        screen,
        text_focus,
        ..Default::default()
    }
}

#[test]
fn settings_context_resolves_first_and_rebinds() {
    let map = KeyMap::defaults();
    let browse = view(Screen::Browsing, false);
    for (chord, action) in [
        ("up", SettingsAction::Up),
        ("down", SettingsAction::Down),
        ("pageup", SettingsAction::PageUp),
        ("pagedown", SettingsAction::PageDown),
        ("left", SettingsAction::PrevCategory),
        ("right", SettingsAction::NextCategory),
        ("enter", SettingsAction::Edit),
        ("space", SettingsAction::Toggle),
        ("/", SettingsAction::Filter),
        ("d", SettingsAction::Default),
        ("u", SettingsAction::UndoField),
        ("r", SettingsAction::Reveal),
        ("a", SettingsAction::Review),
        ("ctrl+r", SettingsAction::Reload),
        ("?", SettingsAction::Help),
        ("esc", SettingsAction::Close),
    ] {
        assert_eq!(
            route_settings(&browse, &map, &key(chord)),
            Some(SettingsKey::Action(action)),
            "{chord}"
        );
    }
    assert_eq!(route_settings(&browse, &map, &key("ctrl+g")), None);
    assert_eq!(
        route(Some(Action::HistorySearch), Focus::Overlay),
        Route::Overlay
    );
    assert_eq!(route(Some(Action::Suspend), Focus::Overlay), Route::Suspend);
    assert_eq!(
        route_settings(&browse, &map, &key("ctrl+r")),
        Some(SettingsKey::Action(SettingsAction::Reload))
    );

    let mut entries = BTreeMap::new();
    entries.insert("settings_down".to_owned(), vec!["j".to_owned()]);
    let (rebound, notices) = KeyMap::from_entries(&entries);
    assert!(notices.is_empty(), "{notices:?}");
    assert_eq!(
        route_settings(&browse, &rebound, &key("j")),
        Some(SettingsKey::Action(SettingsAction::Down))
    );
    assert_eq!(route_settings(&browse, &rebound, &key("down")), None);
    let editing = view(Screen::Editing, true);
    assert_eq!(
        route_settings(&editing, &rebound, &key("j")),
        Some(SettingsKey::Char('j'))
    );
    let filter = view(Screen::Browsing, true);
    assert_eq!(
        route_settings(&filter, &rebound, &key("j")),
        Some(SettingsKey::Char('j'))
    );
}

#[test]
fn the_chooser_names_the_reload_key_the_user_bound() {
    assert_eq!(
        KeyMap::defaults().settings_label(SettingsAction::Reload),
        Some("Ctrl+R".to_owned())
    );
    let mut entries = BTreeMap::new();
    entries.insert("settings_reload".to_owned(), vec!["f5".to_owned()]);
    let (rebound, notices) = KeyMap::from_entries(&entries);
    assert!(notices.is_empty(), "{notices:?}");
    assert_eq!(
        rebound.settings_label(SettingsAction::Reload),
        Some("F5".to_owned())
    );
    entries.insert("settings_reload".to_owned(), Vec::new());
    let (unbound, _) = KeyMap::from_entries(&entries);
    assert_eq!(unbound.settings_label(SettingsAction::Reload), None);
}

#[test]
fn ctrl_r_reaches_the_chooser_and_typing_still_filters() {
    let map = KeyMap::defaults();
    let choosing = view(Screen::Choosing, true);
    assert_eq!(
        route_settings(&choosing, &map, &key("ctrl+r")),
        Some(SettingsKey::Action(SettingsAction::Reload))
    );
    assert_eq!(
        route_settings(&choosing, &map, &key("r")),
        Some(SettingsKey::Char('r'))
    );
    // A text editor keeps Ctrl+R to itself.
    assert_eq!(
        route_settings(&view(Screen::Editing, true), &map, &key("ctrl+r")),
        None
    );
}
