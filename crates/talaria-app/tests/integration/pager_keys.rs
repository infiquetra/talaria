//! Transcript key defaults, rebinding and conflicts. Issue 222.
//!
//! The sixteen transcript chords are the built-in defaults. A plain letter is a pager key.
//! Escape and Control-C are reserved. A pager search bound to q drops the close key.
//! Two pager actions on one letter keep the earlier one. The same arrow may be used
//! in the composer and in the transcript.

use std::collections::BTreeMap;

use talaria_tui::keys::{Action, KeyChord, KeyContext, KeyMap};

fn labels(map: &KeyMap, action: Action) -> Vec<String> {
    map.chords(action).iter().map(KeyChord::label).collect()
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
fn defaults_rebinding_and_duplicates() {
    let map = KeyMap::defaults();
    let wanted = [
        (Action::TranscriptOpen, vec!["Ctrl+O"]),
        (Action::PagerLineUp, vec!["Up", "K"]),
        (Action::PagerLineDown, vec!["Down", "J"]),
        (Action::PagerPageUp, vec!["PageUp", "B"]),
        (Action::PagerPageDown, vec!["PageDown", "Space"]),
        (Action::PagerHalfUp, vec!["Ctrl+U"]),
        (Action::PagerHalfDown, vec!["Ctrl+D"]),
        (Action::PagerTop, vec!["G", "Home"]),
        (Action::PagerBottom, vec!["Shift+G", "End"]),
        (Action::PagerPrevPrompt, vec!["{"]),
        (Action::PagerNextPrompt, vec!["}"]),
        (Action::PagerSearch, vec!["/"]),
        (Action::PagerNextMatch, vec!["N"]),
        (Action::PagerPrevMatch, vec!["Shift+N"]),
        (Action::PagerDetails, vec!["T"]),
        (Action::PagerClose, vec!["Q"]),
    ];
    for (action, expected) in &wanted {
        assert_eq!(
            labels(&map, *action),
            expected.as_slice(),
            "{} is one of the sixteen transcript chords",
            action.config_name()
        );
    }

    let (rebound, notices) = KeyMap::from_entries(&entries(&[("pager_line_up", &["ctrl+k"])]));
    assert!(notices.is_empty(), "{notices:?}");
    assert_eq!(labels(&rebound, Action::PagerLineUp), vec!["Ctrl+K"]);

    assert!(
        KeyChord::parse("k", KeyContext::Pager).is_ok(),
        "a plain letter is a transcript key"
    );
    let reserved = KeyChord::parse("esc", KeyContext::Pager).expect_err("esc is reserved");
    assert!(
        reserved.contains("reserved in the transcript pager"),
        "{reserved}"
    );
    let interrupt = KeyChord::parse("ctrl+c", KeyContext::Pager).expect_err("ctrl+c is reserved");
    assert!(
        interrupt.contains("reserved in the transcript pager"),
        "{interrupt}"
    );

    let (_lost, lost_notices) = KeyMap::from_entries(&entries(&[("pager_search", &["q"])]));
    let lost = lost_notices
        .iter()
        .find(|notice| notice.contains("pager_close"))
        .expect("pager_close lost its default");
    assert_eq!(
        lost,
        "[keys] pager_close in ~/.talaria/config.toml lost its default key Q because \
         pager_search now uses it; pager_close has no key until you bind one."
    );

    let (_clash, clash_notices) = KeyMap::from_entries(&entries(&[
        ("pager_line_up", &["j"]),
        ("pager_line_down", &["j"]),
    ]));
    let clash = clash_notices
        .iter()
        .find(|notice| notice.contains("pager_line_down"))
        .expect("the later binding loses");
    assert_eq!(
        clash,
        "[keys] pager_line_down in ~/.talaria/config.toml: J is already bound to pager_line_up; \
         keeping it on pager_line_up only."
    );

    let (shared, shared_notices) = KeyMap::from_entries(&entries(&[("pager_line_up", &["up"])]));
    assert!(
        shared_notices
            .iter()
            .all(|notice| !notice.contains("already bound")),
        "{shared_notices:?}"
    );
    assert_eq!(labels(&shared, Action::MoveUp), vec!["Up"]);
    assert_eq!(labels(&shared, Action::PagerLineUp), vec!["Up"]);

    for (action, _) in &wanted {
        for chord in map.chords(*action) {
            let label = chord.label();
            assert!(
                !matches!(label.as_str(), "F1" | "F2" | "Ctrl+B" | "Ctrl+S"),
                "{label}"
            );
        }
    }
}
