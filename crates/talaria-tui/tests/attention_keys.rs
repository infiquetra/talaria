//! The jump key and who receives it.

use std::collections::BTreeMap;

use talaria_tui::comfort_route::{Focus, Route, route};
use talaria_tui::keys::{Action, KeyMap};

fn labels(map: &KeyMap, action: Action) -> Vec<String> {
    map.chords(action)
        .iter()
        .map(|chord| chord.label())
        .collect()
}

fn entries(pairs: &[(&str, &[&str])]) -> BTreeMap<String, Vec<String>> {
    pairs
        .iter()
        .map(|(name, chords)| {
            (
                (*name).to_string(),
                chords.iter().map(|chord| (*chord).to_string()).collect(),
            )
        })
        .collect()
}

#[test]
fn attention_next_default_alt_a_unique_and_rebindable() {
    let map = KeyMap::defaults();
    assert_eq!(
        labels(&map, Action::AttentionNext),
        ["Alt+A"],
        "missing default chord"
    );
    for action in Action::ALL {
        if action == Action::AttentionNext {
            continue;
        }
        let holds = map
            .chords(action)
            .iter()
            .any(|chord| chord.label() == "Alt+A");
        assert!(!holds, "{action:?} also holds Alt+A");
    }

    let (rebound, notices) = KeyMap::from_entries(&entries(&[("attention_next", &["alt+b"])]));
    assert!(notices.is_empty(), "{notices:?}");
    assert_eq!(labels(&rebound, Action::AttentionNext), ["Alt+B"]);

    let (unbound, notices) = KeyMap::from_entries(&entries(&[("attention_next", &[])]));
    assert!(notices.is_empty(), "{notices:?}");
    assert!(unbound.chords(Action::AttentionNext).is_empty());
}

#[test]
fn route_attention_next_only_from_composer_focus() {
    assert_eq!(
        route(Some(Action::AttentionNext), Focus::Composer),
        Route::AttentionNext,
        "missing composer route"
    );
    assert_eq!(
        route(Some(Action::AttentionNext), Focus::Search),
        Route::Search
    );
    assert_eq!(route(Some(Action::AttentionNext), Focus::Card), Route::Card);
    assert_eq!(
        route(Some(Action::AttentionNext), Focus::QueueSelection),
        Route::QueueSelection
    );
    assert_eq!(
        route(Some(Action::AttentionNext), Focus::Overlay),
        Route::Overlay
    );
}
