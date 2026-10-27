//! Profiles keys: defaults, rebinding and the Esc order. Issue #253.

use std::collections::BTreeMap;

use talaria_core::commands::local::{LocalAction, LocalCommandTable};
use talaria_core::profiles::{
    Description, GatewayView, HostTopology, KeyAction, Panel, ProfilesEffect, ProfilesInput,
    ProfilesState, RosterEntry,
};
use talaria_tui::keys::{Action, Key, KeyCode, KeyContext, KeyKind, KeyMap, Mods};

fn press(code: KeyCode, ctrl: bool, shift: bool) -> Key {
    Key {
        code,
        mods: Mods {
            ctrl,
            shift,
            ..Mods::default()
        },
        kind: KeyKind::Press,
    }
}

fn key_of(text: &str) -> Key {
    match text {
        "up" => press(KeyCode::Up, false, false),
        "down" => press(KeyCode::Down, false, false),
        "pageup" => press(KeyCode::PageUp, false, false),
        "pagedown" => press(KeyCode::PageDown, false, false),
        "home" => press(KeyCode::Home, false, false),
        "end" => press(KeyCode::End, false, false),
        "enter" => press(KeyCode::Enter, false, false),
        "esc" => press(KeyCode::Esc, false, false),
        "F1" => press(KeyCode::F(1), false, false),
        "F2" => press(KeyCode::F(2), false, false),
        "ctrl+b" => press(KeyCode::Char('b'), true, false),
        "ctrl+s" => press(KeyCode::Char('s'), true, false),
        "ctrl+g" => press(KeyCode::Char('g'), true, false),
        "R" => press(KeyCode::Char('R'), false, true),
        single => {
            let mut chars = single.chars();
            match (chars.next(), chars.next()) {
                (Some(code), None) => press(KeyCode::Char(code), false, false),
                _ => panic!("a test key: {single}"),
            }
        }
    }
}

fn resolve(map: &KeyMap, text: &str) -> Option<Action> {
    map.resolve(&key_of(text), KeyContext::Profiles)
}

#[test]
fn keys_and_rebinding() {
    // Every default resolves in the profiles context.
    let map = KeyMap::defaults();
    for (text, action) in [
        ("up", Action::ProfilesUp),
        ("down", Action::ProfilesDown),
        ("pageup", Action::ProfilesPageUp),
        ("pagedown", Action::ProfilesPageDown),
        ("home", Action::ProfilesFirst),
        ("end", Action::ProfilesLast),
        ("enter", Action::ProfilesOpen),
        ("/", Action::ProfilesFilter),
        ("r", Action::ProfilesRefresh),
        ("R", Action::ProfilesRescan),
        ("n", Action::ProfilesNew),
        ("e", Action::ProfilesDescription),
        ("s", Action::ProfilesSoul),
        ("esc", Action::ProfilesBack),
    ] {
        assert_eq!(resolve(&map, text), Some(action), "{text}");
        assert_eq!(action.context(), KeyContext::Profiles);
    }

    // The risky keys stay free in this context.
    for text in ["F1", "F2", "ctrl+b", "ctrl+s"] {
        assert_eq!(resolve(&map, text), None, "{text} is free");
    }

    // Rebinding one action leaves the rest alone.
    let mut entries = BTreeMap::new();
    entries.insert("profiles_refresh".to_owned(), vec!["ctrl+g".to_owned()]);
    let (map, notices) = KeyMap::from_entries(&entries);
    assert!(notices.is_empty(), "{notices:?}");
    assert_eq!(resolve(&map, "ctrl+g"), Some(Action::ProfilesRefresh));
    assert_eq!(resolve(&map, "r"), None, "the old chord is gone");
    assert_eq!(resolve(&map, "R"), Some(Action::ProfilesRescan));

    // `/profiles` opens the overlay.
    let table = LocalCommandTable::builtin();
    let command = table.find("profiles").expect("/profiles is registered");
    assert_eq!(command.action, LocalAction::Profiles);
    assert!(!command.takes_argument);

    // Esc undoes the innermost open thing, in order.
    let mut machine = ProfilesState::new("default".to_owned());
    let _ = machine.update(ProfilesInput::Open, 0);
    let _ = machine.update(
        ProfilesInput::Discovered(
            0,
            Ok(GatewayView {
                served: vec!["default".to_owned()],
                parked: vec![],
            }),
        ),
        0,
    );
    let _ = machine.update(
        ProfilesInput::Listed(
            0,
            Ok(vec![RosterEntry {
                name: "default".to_owned(),
                display_name: "default".to_owned(),
                is_default: true,
                model: String::new(),
                provider: String::new(),
                description: String::new(),
                skill_count: 0,
            }]),
        ),
        0,
    );
    let _ = machine.update(
        ProfilesInput::Topology(
            0,
            Ok(HostTopology {
                stopped: vec![],
                own_gateway: Some(vec![]),
                gateway_mode: Some("multiplex".to_owned()),
                single_reason: None,
            }),
        ),
        0,
    );

    // A confirmation beats the form beneath it.
    let _ = machine.update(ProfilesInput::Key(KeyAction::New), 10);
    let _ = machine.update(ProfilesInput::Key(KeyAction::Rescan), 10);
    assert!(machine.view().confirm.is_some());
    let effects = machine.update(ProfilesInput::Key(KeyAction::Back), 10);
    assert!(!effects.contains(&ProfilesEffect::Close));
    assert_eq!(machine.view().confirm, None);
    assert!(matches!(machine.view().panel, Panel::Form(_)));
    let _ = machine.update(ProfilesInput::Key(KeyAction::Back), 10);
    assert!(matches!(machine.view().panel, Panel::None));

    // A review beats the input beneath it.
    let _ = machine.update(ProfilesInput::Key(KeyAction::EditDescription), 20);
    let full = Description {
        name: "default".to_owned(),
        description: "base".to_owned(),
        soul: "soul\n".to_owned(),
        model_provider: "ollama-cloud".to_owned(),
        model_default: "minimax-m3.1-flash".to_owned(),
        skills: vec![],
        toolsets: vec![],
        toolsets_pinned: false,
        mcp_servers: vec![],
    };
    let _ = machine.update(
        ProfilesInput::Described(0, "default".to_owned(), Ok(full)),
        20,
    );
    let _ = machine.update(ProfilesInput::Key(KeyAction::Char('!')), 20);
    let _ = machine.update(ProfilesInput::Key(KeyAction::Enter), 20);
    assert!(matches!(machine.view().panel, Panel::Review(_)));
    let _ = machine.update(ProfilesInput::Key(KeyAction::Back), 20);
    assert!(matches!(
        machine.view().panel,
        Panel::DescriptionInput { .. }
    ));
    let _ = machine.update(ProfilesInput::Key(KeyAction::Back), 20);
    assert!(matches!(machine.view().panel, Panel::None));

    // The narrow detail returns to the list.
    let _ = machine.update(ProfilesInput::Key(KeyAction::OpenDetail), 30);
    assert!(matches!(machine.view().panel, Panel::NarrowDetail(_)));
    let _ = machine.update(ProfilesInput::Key(KeyAction::Back), 30);
    assert!(matches!(machine.view().panel, Panel::Detail(_)));

    // A filter clears before the overlay closes.
    let _ = machine.update(ProfilesInput::Key(KeyAction::Filter), 40);
    let _ = machine.update(ProfilesInput::Key(KeyAction::Char('d')), 40);
    assert_eq!(machine.view().filter.as_deref(), Some("d"));
    let effects = machine.update(ProfilesInput::Key(KeyAction::Back), 40);
    assert!(!effects.contains(&ProfilesEffect::Close));
    assert_eq!(machine.view().filter, None);
    assert_eq!(
        machine.update(ProfilesInput::Key(KeyAction::Back), 40),
        vec![ProfilesEffect::Close]
    );
}
