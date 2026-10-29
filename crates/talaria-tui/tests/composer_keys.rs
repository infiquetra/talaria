//! Key behaviour of the composer, driven by scripts.

#[path = "composer_support/mod.rs"]
mod composer_support;

use std::collections::BTreeMap;
use std::time::Duration;

use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use proptest::prelude::*;
use talaria_tui::composer::{Composer, ComposerContext, ComposerOutcome};
use talaria_tui::keys::{Action, Key, KeyMap};
use talaria_tui::live::LiveSection;
use unicode_segmentation::UnicodeSegmentation;

use composer_support::Rig;

#[test]
fn wrapped_vertical_motion_uses_the_visual_column_after_edits_and_resize() {
    let mut rig = Rig::new();
    rig.composer.load("abcdefghijklmnopqrst".into());
    let _ = rig.composer.section(20, 8);
    rig.press("up");
    assert_eq!(rig.composer.caret(), 8);
    rig.press("down");
    assert_eq!(rig.composer.caret(), 20);
    rig.press("left up");
    assert_eq!(rig.composer.caret(), 7);
    rig.press("down right");
    let _ = rig.composer.section(24, 8);
    rig.press("up");
    assert_eq!(rig.composer.caret(), 4);

    rig.composer.load("abcdefghij你abcdefgh".into());
    let _ = rig.composer.section(20, 8);
    rig.press("up");
    assert_eq!(rig.composer.caret(), 8);
    rig.press("down");
    assert_eq!(rig.composer.caret(), rig.composer.text().len());

    rig.composer
        .load("abcdefghijklmnopqrstuvwx\nab\n1234567890".into());
    let _ = rig.composer.section(20, 8);
    for (key, expected) in [
        ("up", 27),
        ("up", 22),
        ("up", 10),
        ("down", 22),
        ("down", 27),
        ("down", 38),
    ] {
        rig.press(key);
        assert_eq!(rig.composer.caret(), expected, "{key}");
    }
}

#[test]
fn sanitized_clusters_and_following_text_remain_visible() {
    let flag = "\u{1f3f4}\u{e0067}\u{e0062}\u{e0065}\u{e006e}\u{e0067}\u{e007f}";
    for text in [format!("{flag}X"), "abcdefghij\u{2028}X".to_owned()] {
        let mut rig = Rig::new();
        rig.composer.load(text.clone());
        let section = rig.composer.section(20, 8);
        let rendered = composer_support::rows(&section, 20, 8).join("\n");
        assert!(rendered.contains('X'), "{rendered:?}");
        assert!(rendered.contains('�'), "{rendered:?}");
        assert_eq!(rig.composer.text(), text);
    }
}

fn labels(map: &KeyMap, action: Action) -> Vec<String> {
    map.chords(action)
        .iter()
        .map(|chord| chord.label())
        .collect()
}

#[test]
fn submit_and_newline_keys() {
    let mut rig = Rig::new();
    let outcomes = rig.press("'  hi there ' shift+enter enter");
    assert_eq!(
        outcomes.last(),
        Some(&ComposerOutcome::Submit("  hi there".to_string()))
    );
    assert_eq!(rig.composer.text(), "");
    assert!(rig.composer.is_empty());
    assert_eq!(rig.composer.history(), ["  hi there".to_string()]);

    let mut rig = Rig::new();
    assert_eq!(rig.press("enter"), vec![ComposerOutcome::Unchanged]);
    rig.press("'   '");
    assert_eq!(rig.press("enter"), vec![ComposerOutcome::Unchanged]);
    assert!(rig.composer.history().is_empty());

    let mut rig = Rig::new();
    assert_eq!(
        rig.press("shift+enter alt+enter ctrl+j"),
        vec![
            ComposerOutcome::Changed,
            ComposerOutcome::Changed,
            ComposerOutcome::Changed,
        ]
    );
    assert_eq!(rig.composer.text(), "\n\n\n");
}

#[test]
fn history_walks_and_restores_the_draft() {
    let mut rig = Rig::new();
    assert_eq!(
        rig.press("alpha enter"),
        vec![
            ComposerOutcome::Changed,
            ComposerOutcome::Changed,
            ComposerOutcome::Changed,
            ComposerOutcome::Changed,
            ComposerOutcome::Changed,
            ComposerOutcome::Submit("alpha".to_string()),
        ]
    );
    rig.press("beta enter");
    rig.press("gam");
    assert_eq!(rig.composer.text(), "gam");
    assert_eq!(rig.composer.caret(), 3);

    assert_eq!(rig.press("up"), vec![ComposerOutcome::Changed]);
    assert_eq!(rig.composer.text(), "beta");
    assert_eq!(rig.composer.caret(), 4);
    assert_eq!(rig.press("up"), vec![ComposerOutcome::Changed]);
    assert_eq!(rig.composer.text(), "alpha");
    assert_eq!(rig.composer.caret(), 5);
    assert_eq!(rig.press("down"), vec![ComposerOutcome::Changed]);
    assert_eq!(rig.composer.text(), "beta");
    assert_eq!(rig.composer.caret(), 4);
    assert_eq!(rig.press("down"), vec![ComposerOutcome::Changed]);
    assert_eq!(rig.composer.text(), "gam");
    assert_eq!(rig.composer.caret(), 3);

    rig.press("up");
    rig.press("!");
    assert_eq!(rig.composer.text(), "beta!");
    rig.press("up");
    assert_eq!(rig.composer.text(), "alpha");
    rig.press("down");
    assert_eq!(rig.composer.text(), "beta!");
    assert_eq!(
        rig.press("enter"),
        vec![ComposerOutcome::Submit("beta!".to_string())]
    );
    assert_eq!(
        rig.composer.history(),
        ["alpha".to_string(), "beta".to_string(), "beta!".to_string()]
    );
}

#[test]
fn history_rules_and_inner_rows() {
    let mut rig = Rig::new();
    rig.press("a enter b enter a enter");
    assert_eq!(
        rig.composer.history(),
        ["a".to_string(), "b".to_string(), "a".to_string()]
    );

    let before = rig.composer.history().to_vec();
    rig.press("enter");
    rig.press("'   ' enter");
    assert_eq!(rig.composer.history(), before.as_slice());

    rig.press("a enter a enter");
    assert_eq!(rig.composer.history().last().map(String::as_str), Some("a"));
    let duplicates = rig
        .composer
        .history()
        .windows(2)
        .filter(|pair| pair[0] == pair[1])
        .count();
    assert_eq!(duplicates, 0);

    let mut rig = Rig::new();
    for index in 0..1001 {
        let word = format!("p{index}");
        rig.press(&format!("{word} enter"));
    }
    assert_eq!(rig.composer.history().len(), 1000);
    assert_eq!(
        rig.composer.history().first().map(String::as_str),
        Some("p1")
    );
    assert_eq!(
        rig.composer.history().last().map(String::as_str),
        Some("p1000")
    );

    let seeded = vec![
        String::new(),
        "   ".to_string(),
        "a".to_string(),
        "a".to_string(),
        "b".to_string(),
    ];
    let rig = Rig::with_history(seeded);
    assert_eq!(rig.composer.history(), ["a".to_string(), "b".to_string()]);
    let repeated = Rig::with_history(vec!["a".into(), "b".into(), "a".into()]);
    assert_eq!(
        repeated.composer.history(),
        ["a".to_string(), "b".to_string(), "a".to_string()]
    );
    let many: Vec<String> = (0..1005).map(|index| format!("s{index}")).collect();
    let rig = Rig::with_history(many);
    assert_eq!(rig.composer.history().len(), 1000);
    assert_eq!(
        rig.composer.history().first().map(String::as_str),
        Some("s5")
    );
    assert_eq!(
        rig.composer.history().last().map(String::as_str),
        Some("s1004")
    );

    let mut rig = Rig::with_history(vec!["alpha".into(), "beta".into()]);
    rig.press("up up");
    assert_eq!(rig.composer.text(), "alpha");
    let caret = rig.composer.caret();
    assert_eq!(rig.press("up"), vec![ComposerOutcome::Unchanged]);
    assert_eq!(rig.composer.text(), "alpha");
    assert_eq!(rig.composer.caret(), caret);

    let mut rig = Rig::new();
    rig.press("ab shift+enter cd shift+enter ef");
    assert_eq!(rig.composer.text(), "ab\ncd\nef");
    assert!(!rig.composer.cursor_on_first_row());
    rig.press("up");
    assert_eq!(rig.composer.text(), "ab\ncd\nef");
    assert_eq!(rig.composer.caret(), "ab\ncd".len());
    let history_before = rig.composer.history().to_vec();
    rig.press("up");
    assert_eq!(rig.composer.caret(), "ab".len());
    rig.press("down");
    assert_eq!(rig.composer.caret(), "ab\ncd".len());
    assert_eq!(rig.composer.history(), history_before.as_slice());
}

#[test]
fn esc_interrupts_only_a_running_turn() {
    let mut rig = Rig::new();
    rig.set_running(true);
    rig.press("keep");
    let draft = rig.composer.text().to_string();
    assert_eq!(rig.press("esc"), vec![ComposerOutcome::Interrupt]);
    assert_eq!(rig.composer.text(), draft);
    assert_eq!(rig.press("esc"), vec![ComposerOutcome::Interrupt]);

    let mut rig = Rig::new();
    rig.press("keep");
    assert_eq!(rig.press("esc"), vec![ComposerOutcome::Unchanged]);
    assert_eq!(rig.composer.text(), "keep");

    let mut rig = Rig::new();
    assert_eq!(rig.press("ctrl+c"), vec![ComposerOutcome::Changed]);
    assert_eq!(rig.press("esc"), vec![ComposerOutcome::Changed]);
    assert_eq!(rig.press("ctrl+c"), vec![ComposerOutcome::Changed]);
}

#[test]
fn ctrl_c_follows_the_decision_table() {
    let mut rig = Rig::new();
    rig.press("draft");
    assert_eq!(rig.press("ctrl+c"), vec![ComposerOutcome::Changed]);
    assert_eq!(rig.composer.text(), "");
    assert_eq!(rig.composer.history(), ["draft".to_string()]);

    let mut rig = Rig::new();
    rig.set_running(true);
    assert_eq!(rig.press("ctrl+c"), vec![ComposerOutcome::Interrupt]);
    assert!(rig.composer.is_empty());

    let mut rig = Rig::new();
    assert_eq!(rig.press("ctrl+c"), vec![ComposerOutcome::Changed]);
    rig.advance(1_899);
    assert_eq!(rig.press("ctrl+c"), vec![ComposerOutcome::ExitRequested]);

    let mut rig = Rig::new();
    assert_eq!(rig.press("ctrl+c"), vec![ComposerOutcome::Changed]);
    rig.advance(2_000);
    assert_eq!(rig.press("ctrl+c"), vec![ComposerOutcome::Changed]);
    rig.advance(2_000);
    assert_eq!(rig.press("ctrl+c"), vec![ComposerOutcome::Changed]);

    let mut rig = Rig::new();
    rig.set_running(true);
    rig.press("stay");
    assert_eq!(rig.press("esc"), vec![ComposerOutcome::Interrupt]);
    assert_eq!(rig.composer.text(), "stay");

    let mut rig = Rig::new();
    rig.press("ctrl+c");
    assert_eq!(rig.press("esc"), vec![ComposerOutcome::Changed]);
    assert_eq!(rig.press("ctrl+c"), vec![ComposerOutcome::Changed]);

    let mut rig = Rig::new();
    assert_eq!(rig.press("esc"), vec![ComposerOutcome::Unchanged]);

    let mut rig = Rig::new();
    rig.press("ctrl+c");
    rig.advance(2_000);
    assert_eq!(rig.tick(), ComposerOutcome::Changed);
    assert_eq!(rig.press("ctrl+c"), vec![ComposerOutcome::Changed]);

    let mut rig = Rig::new();
    rig.press("ctrl+c");
    rig.press("a");
    assert_eq!(rig.press("ctrl+c"), vec![ComposerOutcome::Changed]);
    assert_eq!(rig.composer.text(), "");

    let mut rig = Rig::new();
    rig.press("'   '");
    assert!(!rig.composer.is_empty());
    assert_eq!(rig.press("ctrl+c"), vec![ComposerOutcome::Changed]);
    assert_eq!(rig.composer.text(), "");
    assert!(rig.composer.history().is_empty());
}

#[test]
fn release_unbound_and_custom_keys() {
    let mut rig = Rig::new();
    let release = Key::from(KeyEvent::new_with_kind(
        KeyCode::Char('a'),
        KeyModifiers::NONE,
        KeyEventKind::Release,
    ));
    rig.now += Duration::from_millis(100);
    assert_eq!(
        rig.composer.handle_key(
            release,
            ComposerContext {
                turn_running: false
            },
            rig.now,
        ),
        ComposerOutcome::Unchanged
    );
    assert_eq!(
        rig.press("f1 f2 ctrl+b ctrl+s"),
        vec![
            ComposerOutcome::Unchanged,
            ComposerOutcome::Unchanged,
            ComposerOutcome::Unchanged,
            ComposerOutcome::Unchanged,
        ]
    );

    let mut entries = BTreeMap::new();
    entries.insert("submit".to_string(), vec!["alt+enter".to_string()]);
    entries.insert("newline".to_string(), vec!["enter".to_string()]);
    entries.insert("cancel".to_string(), Vec::new());
    let (map, notices) = KeyMap::from_entries(&entries);
    assert!(notices.is_empty(), "{notices:?}");
    assert_eq!(labels(&map, Action::Submit), vec!["Alt+Enter".to_string()]);
    assert_eq!(labels(&map, Action::Newline), vec!["Enter".to_string()]);
    assert!(labels(&map, Action::Cancel).is_empty());
    let mut rig = Rig::from_composer(Composer::new(map));
    assert_eq!(rig.press("enter"), vec![ComposerOutcome::Changed]);
    assert_eq!(rig.composer.text(), "\n");
    rig.press("hi");
    assert_eq!(
        rig.press("alt+enter"),
        vec![ComposerOutcome::Submit("\nhi".to_string())]
    );
    assert_eq!(rig.press("ctrl+c"), vec![ComposerOutcome::Unchanged]);

    let mut rig = Rig::new();
    rig.composer.load("loaded".to_string());
    assert_eq!(rig.composer.text(), "loaded");
    assert_eq!(rig.composer.caret(), "loaded".len());
    assert!(rig.composer.history().is_empty());
    rig.composer.clear();
    assert_eq!(rig.composer.text(), "");
    assert!(rig.composer.history().is_empty());

    rig.composer.set_title(Some("Topic".to_string()));
    let section = rig.composer.section(60, 10);
    let height = section.height(60);
    let drawn = composer_support::rows(&section, 60, height);
    assert!(drawn[0].contains("Topic"), "{}", drawn[0]);

    let mut rig = Rig::new();
    assert_eq!(rig.composer.next_tick(), None);
    rig.burst("abc");
    let due = rig.composer.next_tick().expect("burst deadline");
    assert_eq!(due, rig.now + Duration::from_millis(8));
    assert_eq!(rig.press("ctrl+c"), vec![ComposerOutcome::Changed]);
    assert_eq!(rig.composer.text(), "");
    assert_eq!(rig.composer.history(), ["abc".to_string()]);
    assert_eq!(rig.composer.next_tick(), None);
}

fn any_event() -> impl Strategy<Value = Event> {
    const CHORDS: &[&str] = &[
        "enter",
        "shift+enter",
        "left",
        "right",
        "up",
        "down",
        "backspace",
        "ctrl+w",
        "ctrl+y",
        "ctrl+a",
        "ctrl+e",
    ];
    prop_oneof![
        proptest::sample::select(&['a', ' ', '\n', '你', '\u{0301}', '\u{200D}', 'é'][..])
            .prop_map(Event::Char),
        proptest::sample::select(CHORDS).prop_map(Event::Chord),
        Just(Event::Paste("hi\n".to_string())),
    ]
}

proptest! {
    #![proptest_config(ProptestConfig {
        cases: 16,
        failure_persistence: None,
        ..ProptestConfig::default()
    })]

    #[test]
    fn random_keys_keep_the_draft_valid(
        events in proptest::collection::vec(any_event(), 0..200usize),
    ) {
        let mut rig = Rig::new();
        for event in events {
            match event {
                Event::Char(ch) => {
                    rig.now += Duration::from_millis(100);
                    let key = Key::from(KeyEvent::new(KeyCode::Char(ch), KeyModifiers::NONE));
                    let _ = rig.composer.handle_key(
                        key,
                        ComposerContext { turn_running: false },
                        rig.now,
                    );
                }
                Event::Chord(name) => {
                    let _ = rig.press(name);
                }
                Event::Paste(text) => {
                    let _ = rig.composer.handle_paste(&text);
                }
            }
            let text = rig.composer.text();
            let caret = rig.composer.caret();
            let bounds: Vec<usize> = text
                .grapheme_indices(true)
                .map(|(index, _)| index)
                .chain(std::iter::once(text.len()))
                .collect();
            assert!(bounds.contains(&caret), "caret {caret} in {text:?}");
            assert!(text.chars().all(|ch| !ch.is_control() || ch == '\n' || ch == '\t'));
        }
    }
}

#[derive(Clone, Debug)]
enum Event {
    Char(char),
    Chord(&'static str),
    Paste(String),
}
