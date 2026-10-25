//! The comfort key actions and their routing. Behavior contract: issue 228.

use std::collections::BTreeMap;

use talaria_tui::comfort_route::{Focus, Route, route};
use talaria_tui::keys::{Action, Key, KeyCode, KeyKind, KeyMap, Mods};

fn press(code: KeyCode, ctrl: bool, alt: bool) -> Key {
    Key {
        code,
        mods: Mods {
            ctrl,
            alt,
            ..Mods::default()
        },
        kind: KeyKind::Press,
    }
}

#[test]
fn new_actions_safe_and_unique() {
    let map = KeyMap::defaults();
    let ctrl_g = || press(KeyCode::Char('g'), true, false);
    let alt_g = || press(KeyCode::Char('g'), false, true);
    let ctrl_r = || press(KeyCode::Char('r'), true, false);
    let ctrl_z = || press(KeyCode::Char('z'), true, false);

    assert_eq!(map.resolve(&ctrl_g()), Some(Action::OpenEditor));
    assert_eq!(map.resolve(&alt_g()), Some(Action::OpenEditor));
    assert_eq!(map.resolve(&ctrl_r()), Some(Action::HistorySearch));
    assert_eq!(map.resolve(&ctrl_z()), Some(Action::Suspend));

    for action in Action::ALL {
        for chord in map.chords(action) {
            let holders = Action::ALL
                .iter()
                .filter(|other| other.context() == action.context())
                .filter(|other| map.chords(**other).contains(chord))
                .count();
            assert_eq!(holders, 1, "one action holds {}", chord.label());
        }
    }

    for action in Action::ALL {
        if action == Action::Suspend {
            continue;
        }
        assert!(
            !map.chords(action)
                .iter()
                .any(|chord| chord.matches(&ctrl_z())),
            "{} must not bind ctrl+z",
            action.config_name()
        );
    }

    let mut entries = BTreeMap::new();
    entries.insert("suspend".to_owned(), vec!["ctrl+x".to_owned()]);
    let (map, notices) = KeyMap::from_entries(&entries);
    assert!(notices.is_empty(), "{notices:?}");
    assert_eq!(
        map.resolve(&press(KeyCode::Char('x'), true, false)),
        Some(Action::Suspend)
    );
    assert_eq!(map.resolve(&ctrl_z()), None, "rebinding moves the chord");
}

#[test]
fn route_puts_suspend_first() {
    for focus in [
        Focus::Composer,
        Focus::Search,
        Focus::Card,
        Focus::QueueSelection,
        Focus::Overlay,
    ] {
        assert_eq!(route(Some(Action::Suspend), focus), Route::Suspend);
    }

    assert_eq!(
        route(Some(Action::OpenEditor), Focus::Search),
        Route::Search
    );
    assert_eq!(route(Some(Action::OpenEditor), Focus::Card), Route::Card);
    assert_eq!(
        route(Some(Action::OpenEditor), Focus::QueueSelection),
        Route::QueueSelection
    );
    assert_eq!(
        route(Some(Action::OpenEditor), Focus::Overlay),
        Route::Overlay
    );
    assert_eq!(
        route(Some(Action::HistorySearch), Focus::Overlay),
        Route::Overlay
    );
    assert_eq!(route(None, Focus::Search), Route::Search);
    assert_eq!(route(None, Focus::Card), Route::Card);
    assert_eq!(route(None, Focus::QueueSelection), Route::QueueSelection);
    assert_eq!(route(None, Focus::Overlay), Route::Overlay);

    assert_eq!(
        route(Some(Action::OpenEditor), Focus::Composer),
        Route::OpenEditor
    );
    assert_eq!(
        route(Some(Action::HistorySearch), Focus::Composer),
        Route::HistorySearch
    );
    assert_eq!(
        route(Some(Action::Submit), Focus::Composer),
        Route::Composer
    );
    assert_eq!(route(None, Focus::Composer), Route::Composer);
}
