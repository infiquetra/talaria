//! The checklist toggle key: bound by default, unbindable in config.

use std::collections::BTreeMap;

use talaria_tui::keys::{Action, Key, KeyCode, KeyContext, KeyKind, KeyMap, Mods};

fn ctrl_t() -> Key {
    Key {
        code: KeyCode::Char('t'),
        mods: Mods {
            ctrl: true,
            alt: false,
            shift: false,
            super_key: false,
            other: false,
        },
        kind: KeyKind::Press,
    }
}

#[test]
fn toggle_tasks_defaults_to_ctrl_t_and_can_be_unbound() {
    let map = KeyMap::defaults();
    assert_eq!(
        map.resolve(&ctrl_t(), KeyContext::Composer),
        Some(Action::ToggleTasks)
    );
    assert_eq!(map.label(Action::ToggleTasks), Some("Ctrl+T".to_string()));
    let mut entries = BTreeMap::new();
    entries.insert("toggle_tasks".to_string(), Vec::new());
    let (map, notices) = KeyMap::from_entries(&entries);
    assert!(notices.is_empty(), "{notices:?}");
    assert_eq!(map.resolve(&ctrl_t(), KeyContext::Composer), None);
    assert_eq!(map.label(Action::ToggleTasks), None);
}
