//! Shared completion keys. Issue #223, scenario "completion actions parse and coexist".

use std::collections::BTreeMap;

use talaria_tui::keys::{Action, KeyChord, KeyMap};

fn labels(map: &KeyMap, action: Action) -> Vec<String> {
    map.chords(action).iter().map(KeyChord::label).collect()
}

#[test]
fn completion_actions_parse_and_coexist() {
    let (map, notices) = KeyMap::from_entries(&BTreeMap::new());
    assert!(notices.is_empty(), "{notices:?}");
    assert_eq!(Action::ALL.len(), 156);
    for name in [
        "completion_next",
        "completion_prev",
        "completion_accept",
        "completion_close",
    ] {
        assert!(Action::from_config_name(name).is_some(), "{name}");
    }
    assert_eq!(labels(&map, Action::CompletionNext), ["Down", "Ctrl+N"]);
    assert_eq!(labels(&map, Action::CompletionPrev), ["Up", "Ctrl+P"]);
    assert_eq!(labels(&map, Action::CompletionAccept), ["Tab"]);
    assert_eq!(labels(&map, Action::CompletionClose), ["Esc"]);

    let mut entries = BTreeMap::new();
    entries.insert("completion_close".into(), vec!["ctrl+g".into()]);
    let (map, notices) = KeyMap::from_entries(&entries);
    assert!(notices.is_empty(), "{notices:?}");
    assert_eq!(labels(&map, Action::CompletionClose), ["Ctrl+G"]);

    let mut entries = BTreeMap::new();
    entries.insert("completion_next".into(), vec!["ctrl+g".into()]);
    entries.insert("completion_prev".into(), vec!["ctrl+g".into()]);
    let (map, notices) = KeyMap::from_entries(&entries);
    let lost = notices.iter().any(|notice| {
        notice.contains("completion_prev")
            && notice.contains("Ctrl+G")
            && notice.contains("completion_next")
    });
    assert!(lost, "{notices:?}");
    assert_eq!(labels(&map, Action::CompletionNext), ["Ctrl+G"]);
    assert!(
        !labels(&map, Action::CompletionPrev)
            .iter()
            .any(|label| label == "Ctrl+G")
    );

    let mut entries = BTreeMap::new();
    entries.insert("completion_close".into(), vec!["esc".into()]);
    let (_map, notices) = KeyMap::from_entries(&entries);
    assert!(
        notices.is_empty(),
        "a chord shared with the composer is not a conflict: {notices:?}"
    );
}
