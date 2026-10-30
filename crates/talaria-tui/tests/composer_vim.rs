//! Vim command scripts. Each case starts from a fresh composer with vim turned on, then loads.

#[path = "composer_support/mod.rs"]
mod composer_support;

use std::collections::BTreeMap;
use std::time::Duration;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use talaria_tui::composer::{Composer, ComposerContext, ComposerOutcome, VimMode, byte_of_cluster};
use talaria_tui::keys::{Action, Key, KeyChord, KeyContext, KeyMap};
use talaria_tui::live::LiveSection;

use composer_support::{Rig, rows};

const SEED: u64 = 42;

#[test]
fn toggle_action_turns_vim_on_and_off_and_keeps_every_default() {
    let defaults = KeyMap::defaults();
    assert_eq!(Action::ALL.len(), 194);
    assert_eq!(Action::ALL[Action::ALL.len() - 1], Action::ToggleVim);
    assert!(defaults.chords(Action::ToggleVim).is_empty());
    for action in Action::ALL {
        for chord in defaults.chords(action) {
            let resolved = defaults.resolve(&chord.to_key(), action.context());
            assert_eq!(
                resolved,
                Some(action),
                "{} on {}",
                chord.label(),
                action.config_name()
            );
        }
    }
    for text in ["ctrl+v", "alt+v"] {
        let chord = KeyChord::parse(text, KeyContext::Composer).expect(text);
        assert_eq!(
            defaults.resolve(&chord.to_key(), KeyContext::Composer),
            Some(Action::AttachClipboardImage),
            "{text}"
        );
    }
    let mut bogus = BTreeMap::new();
    bogus.insert("bogus".to_string(), vec!["x".to_string()]);
    let (_map, notices) = KeyMap::from_entries(&bogus);
    assert!(
        notices[0].ends_with("toggle_vim."),
        "known actions end with toggle_vim: {}",
        notices[0]
    );

    let mut unbound = BTreeMap::new();
    unbound.insert("toggle_vim".to_string(), Vec::new());
    let (unbound, unbound_notices) = KeyMap::from_entries(&unbound);
    assert!(
        unbound_notices
            .iter()
            .all(|notice| !notice.contains("not a known action")),
        "{unbound_notices:?}"
    );
    assert!(unbound.chords(Action::ToggleVim).is_empty());

    let mut entries = BTreeMap::new();
    entries.insert("toggle_vim".to_string(), vec!["ctrl+alt+v".to_string()]);
    let (map, bind_notices) = KeyMap::from_entries(&entries);
    assert!(
        bind_notices
            .iter()
            .all(|notice| !notice.contains("not a known action")),
        "{bind_notices:?}"
    );
    let chord = KeyChord::parse("ctrl+alt+v", KeyContext::Composer).expect("ctrl+alt+v");
    assert_eq!(
        map.resolve(&chord.to_key(), KeyContext::Composer),
        Some(Action::ToggleVim)
    );
    let mut rig = Rig::from_composer(Composer::new(map));
    assert!(!rig.composer.vim_enabled());
    assert!(
        drawn(&mut rig)
            .iter()
            .all(|row| !row.contains("-- NORMAL --"))
    );
    rig.press("ctrl+alt+v");
    assert_eq!(
        rig.composer.vim_mode(),
        Some(VimMode::Normal),
        "ctrl+alt+v enters NORMAL"
    );
    assert_eq!(rig.composer.text(), "");
    rig.press("x");
    assert_eq!(rig.composer.text(), "");
    assert_eq!(rig.composer.vim_mode(), Some(VimMode::Normal));
    rig.press("ctrl+alt+v");
    assert!(!rig.composer.vim_enabled());
    assert_eq!(rig.composer.vim_mode(), None);
    assert!(
        drawn(&mut rig)
            .iter()
            .all(|row| !row.contains("-- NORMAL --"))
    );

    let started = Composer::from_startup(KeyMap::defaults(), Vec::new(), true);
    assert!(started.vim_enabled());
    assert_eq!(
        started.vim_mode(),
        Some(VimMode::Normal),
        "vim = true configuration still starts in NORMAL"
    );
}

#[test]
fn insert_esc_enters_normal_and_typing_there_is_not_text() {
    let mut plain = Rig::new();
    plain.press("hello");
    assert_eq!(plain.composer.text(), "hello");

    let mut rig = load_byte("", 0);
    assert!(rig.composer.vim_enabled());
    assert_eq!(rig.composer.text(), "");
    rig.press("ihello esc");
    assert_eq!(rig.composer.vim_mode(), Some(VimMode::Normal));
    assert_eq!(rig.composer.text(), "hello");
    assert_eq!(rig.composer.caret(), byte_of_cluster("hello", 4));
    rig.press("h");
    assert_eq!(rig.composer.caret(), 3);
    assert_eq!(rig.composer.text(), "hello");
    rig.press("e");
    assert_eq!(rig.composer.caret(), 4);
    rig.press("l");
    assert_eq!(rig.composer.caret(), 4);
    rig.press("l");
    assert_eq!(rig.composer.caret(), 4);
    rig.press("o");
    assert_eq!(rig.composer.text(), "hello\n");
    assert!(!rig.composer.text().contains("hellohello"));
    assert_eq!(rig.composer.vim_mode(), Some(VimMode::Insert));
    rig.press("esc");
    assert_eq!(rig.composer.vim_mode(), Some(VimMode::Normal));
    assert_eq!(rig.composer.text(), "hello\n");

    let mut running = load_byte("", 0);
    running.set_running(true);
    running.press("ihello");
    let first = running.press("esc");
    assert_eq!(first, vec![ComposerOutcome::Changed]);
    assert_eq!(running.composer.vim_mode(), Some(VimMode::Normal));
    assert_eq!(running.composer.text(), "hello");
    let second = running.press("esc");
    assert_eq!(second, vec![ComposerOutcome::Interrupt]);
    assert_eq!(running.composer.text(), "hello");
}

#[test]
fn motions_move_the_caret() {
    let emoji = "a🙂b x";
    assert_eq!(byte_of_cluster(emoji, 0), 0);
    assert_eq!(byte_of_cluster(emoji, 1), 1);
    assert_eq!(byte_of_cluster(emoji, 2), 5);
    assert_eq!(byte_of_cluster(emoji, 3), 6);
    assert_eq!(byte_of_cluster(emoji, 4), 7);

    let line = "one two  three";
    assert_eq!(motion(line, 0, "h"), 0);
    assert_eq!(motion(line, 0, "l"), 1);
    assert_eq!(motion(line, 6, "0"), 0);
    assert_eq!(motion(line, 6, "^"), 0);
    assert_eq!(motion("  two", 2, "^"), 2);
    assert_eq!(motion("  two", 2, "0"), 0);
    assert_eq!(motion(line, 0, "$"), 13);
    assert_eq!(motion("ab\ncde", 0, "2$"), 5);
    assert_eq!(motion(line, 0, "w"), 4);
    assert_eq!(motion(line, 0, "3w"), 13);
    assert_eq!(motion(line, 9, "b"), 4);
    assert_eq!(motion(line, 0, "e"), 2);
    assert_eq!(motion(line, 0, "W"), 4);
    assert_eq!(motion(line, 9, "B"), 4);
    assert_eq!(motion(line, 0, "E"), 2);
    assert_eq!(motion("ab\ncde", 0, "j"), 3);
    assert_eq!(motion("ab\ncde", 3, "k"), 0);
    assert_eq!(motion("ab\ncde", 0, "k"), 0);
    assert_eq!(motion("ab\ncde", 3, "j"), 3);
    assert_eq!(motion("ab\ncde", 1, "j"), 4);
    assert_eq!(motion("ab\ncde", 4, "k"), 1);
    assert_eq!(motion("ab\ncde", 0, "2j"), 3);
    assert_eq!(motion("ab\ncde", 5, "gg"), 0);
    assert_eq!(motion("ab\ncde", 0, "G"), 3);
    assert_eq!(motion("ab\ncde", 0, "2gg"), 3);
    assert_eq!(motion("ab\ncde", 5, "1G"), 0);
    assert_eq!(motion(line, 1, "left"), 0);
    assert_eq!(motion(line, 0, "right"), 1);
    assert_eq!(motion("ab\ncde", 0, "down"), 3);
    assert_eq!(motion("ab\ncde", 3, "up"), 0);
    assert_eq!(motion("ab", 1, "l"), 1);
    assert_eq!(motion(emoji, 0, "l"), 1);
    assert_eq!(motion(emoji, 1, "l"), 5);
    assert_eq!(motion(emoji, 0, "w"), 1);
    assert_eq!(motion(emoji, 1, "w"), 5);
    assert_eq!(motion(emoji, 5, "w"), 7);

    let mut found = motion_rig("one two one", 0);
    found.press("ft");
    assert_eq!(found.composer.caret(), 4);
    found.press(";");
    assert_eq!(found.composer.caret(), 4);
    found.press(",");
    assert_eq!(found.composer.caret(), 4);
    assert_eq!(motion("one two one", 8, "Fo"), 6);
    assert_eq!(motion("one two one", 0, "fz"), 0);
    assert_eq!(motion("one two one", 0, "tw"), 4);
    assert_eq!(motion("one two one", 8, "Tw"), 6);
}

#[test]
fn operators_with_motions_and_counts() {
    let samples = [
        (
            "one two three",
            0,
            "dw",
            "two three",
            0,
            Some("one "),
            Some(false),
        ),
        (
            "one two three",
            4,
            "d$",
            "one ",
            3,
            Some("two three"),
            Some(false),
        ),
        (
            "one two three",
            0,
            "d2w",
            "three",
            0,
            Some("one two "),
            Some(false),
        ),
        (
            "one two three",
            0,
            "y2w",
            "one two three",
            0,
            Some("one two "),
            Some(false),
        ),
        (
            "one two three",
            0,
            "yy",
            "one two three",
            0,
            Some("one two three\n"),
            Some(true),
        ),
        (
            "one two three",
            4,
            "D",
            "one ",
            3,
            Some("two three"),
            Some(false),
        ),
        ("a\nb\nc", 0, "2dd", "c", 0, Some("a\nb\n"), Some(true)),
        ("only", 0, "2dd", "", 0, Some("only\n"), Some(true)),
        ("a\nb", 2, "dj", "a", 0, Some("b\n"), Some(true)),
        ("a\nb\nc", 2, "d2j", "a", 0, Some("b\nc\n"), Some(true)),
        ("ab", 1, "d$", "a", 0, Some("b"), Some(false)),
        ("a b", 0, "de", " b", 0, Some("a"), Some(false)),
        ("one\ntwo", 0, "dw", "two", 0, Some("one\n"), Some(false)),
        ("one\ntwo", 4, "db", "two", 0, Some("one\n"), Some(false)),
    ];
    for (text, at, script, expect, caret, kill, linewise) in samples {
        let rig = edited(text, at, script);
        assert_eq!(rig.composer.text(), expect, "{script} on {text:?}");
        assert_eq!(rig.composer.caret(), caret, "{script}");
        assert_eq!(rig.composer.vim_mode(), Some(VimMode::Normal), "{script}");
        if let Some(kill) = kill {
            assert_eq!(rig.composer.kill_buffer(), kill, "{script}");
        }
        if let Some(linewise) = linewise {
            assert_eq!(rig.composer.kill_linewise(), linewise, "{script}");
        }
    }

    let changed = edited("one two three", 0, "cwTWO esc");
    assert_eq!(changed.composer.text(), "TWO two three");
    assert_eq!(changed.composer.vim_mode(), Some(VimMode::Normal));

    let line = edited("a\nb", 0, "ccx esc");
    assert_eq!(line.composer.text(), "x\nb");
    assert_eq!(line.composer.vim_mode(), Some(VimMode::Normal));
    assert!(line.composer.kill_linewise());

    let word = edited("a b", 0, "cwz esc");
    assert_eq!(word.composer.text(), "z b");
    assert_eq!(word.composer.kill_buffer(), "a");
    assert_eq!(word.composer.vim_mode(), Some(VimMode::Normal));

    let change_line = edited("one two three", 4, "C");
    assert_eq!(change_line.composer.text(), "one ");
    assert_eq!(change_line.composer.vim_mode(), Some(VimMode::Insert));
    assert_eq!(change_line.composer.kill_buffer(), "two three");

    let mut at_end = load_byte("ab", 2);
    assert_eq!(at_end.composer.caret(), 2);
    assert_eq!(at_end.composer.kill_buffer(), "");
    at_end.press("dw");
    assert_eq!(at_end.composer.text(), "ab");
    assert_eq!(at_end.composer.kill_buffer(), "");

    let mut at_start = load_byte("ab", 0);
    at_start.press("db");
    assert_eq!(at_start.composer.text(), "ab");
    assert_eq!(at_start.composer.kill_buffer(), "");
}

#[test]
fn insert_entries_and_single_edits() {
    let mut into = load_byte("two", 0);
    assert_eq!(into.composer.text(), "two");
    into.press("i");
    assert_eq!(into.composer.vim_mode(), Some(VimMode::Insert));
    assert_eq!(into.composer.caret(), 0);
    assert_eq!(into.composer.text(), "two");

    let mut after = load_byte("two", 0);
    after.press("a");
    assert_eq!(after.composer.vim_mode(), Some(VimMode::Insert));
    assert_eq!(after.composer.caret(), 1);

    let mut first = load_byte("  two", 4);
    first.press("I");
    assert_eq!(first.composer.caret(), 2);
    assert_eq!(first.composer.vim_mode(), Some(VimMode::Insert));
    assert_eq!(first.composer.text(), "  two");

    let mut end = load_byte("  two", 0);
    end.press("A");
    assert_eq!(end.composer.caret(), 5);
    assert_eq!(end.composer.vim_mode(), Some(VimMode::Insert));

    let mut below = load_byte("  two", 0);
    below.press("o");
    assert_eq!(below.composer.text(), "  two\n");
    assert_eq!(below.composer.caret(), 6);
    assert_eq!(below.composer.vim_mode(), Some(VimMode::Insert));

    let mut above = load_byte("  two", 2);
    above.press("O");
    assert_eq!(above.composer.text(), "\n  two");
    assert_eq!(above.composer.caret(), 0);
    assert_eq!(above.composer.vim_mode(), Some(VimMode::Insert));

    let mut counted = load_byte("", 0);
    counted.press("3iab esc");
    assert_eq!(counted.composer.text(), "ab");
    assert_eq!(counted.composer.vim_mode(), Some(VimMode::Normal));

    let deleted = edited("two", 0, "x");
    assert_eq!(deleted.composer.text(), "wo");
    assert_eq!(deleted.composer.caret(), 0);
    assert_eq!(deleted.composer.kill_buffer(), "t");
    assert_eq!(deleted.composer.vim_mode(), Some(VimMode::Normal));

    let tail = edited("two", 2, "x");
    assert_eq!(tail.composer.text(), "tw");
    assert_eq!(tail.composer.caret(), 1);

    let before = edited("two", 0, "X");
    assert_eq!(before.composer.text(), "two");
    assert_eq!(before.composer.caret(), 0);

    let inner = edited("two", 2, "X");
    assert_eq!(inner.composer.text(), "to");
    assert_eq!(inner.composer.caret(), 1);

    let split = edited("ab\ncd", 1, "x");
    assert_eq!(split.composer.text(), "a\ncd");
    let held = edited("ab\ncd", 3, "X");
    assert_eq!(held.composer.text(), "ab\ncd");
    assert_eq!(held.composer.caret(), 3);

    let replaced = edited("two", 0, "rX");
    assert_eq!(replaced.composer.text(), "Xwo");
    assert_eq!(replaced.composer.vim_mode(), Some(VimMode::Normal));
    assert_eq!(replaced.composer.kill_buffer(), "");

    let cancelled = edited("two", 0, "r esc");
    assert_eq!(cancelled.composer.text(), "two");
    assert_eq!(cancelled.composer.vim_mode(), Some(VimMode::Normal));

    let substitute = edited("two", 0, "s");
    assert_eq!(substitute.composer.text(), "wo");
    assert_eq!(substitute.composer.caret(), 0);
    assert_eq!(substitute.composer.vim_mode(), Some(VimMode::Insert));
    assert_eq!(substitute.composer.kill_buffer(), "t");

    let line = edited("ab\ncd", 0, "S");
    assert_eq!(line.composer.text(), "\ncd");
    assert_eq!(line.composer.caret(), 0);
    assert_eq!(line.composer.vim_mode(), Some(VimMode::Insert));
    assert!(line.composer.kill_linewise());
}

#[test]
fn visual_delete_yank_change_and_cancel() {
    let deleted = edited("one two three", 0, "ved");
    assert_eq!(deleted.composer.text(), " two three");
    assert_eq!(deleted.composer.caret(), 0);
    assert_eq!(deleted.composer.kill_buffer(), "one");
    assert!(!deleted.composer.kill_linewise());
    assert_eq!(deleted.composer.vim_mode(), Some(VimMode::Normal));

    let yanked = edited("one two three", 0, "vey");
    assert_eq!(yanked.composer.text(), "one two three");
    assert_eq!(yanked.composer.caret(), 0);
    assert_eq!(yanked.composer.kill_buffer(), "one");
    assert_eq!(yanked.composer.vim_mode(), Some(VimMode::Normal));

    let lines = edited("one\ntwo", 0, "Vjd");
    assert_eq!(lines.composer.text(), "");
    assert_eq!(lines.composer.vim_mode(), Some(VimMode::Normal));
    assert!(lines.composer.kill_linewise());

    let widened = edited("one two three", 0, "vVd");
    assert_eq!(widened.composer.text(), "");
    assert_eq!(widened.composer.vim_mode(), Some(VimMode::Normal));
    assert!(widened.composer.kill_linewise());

    let cancelled = edited("one two three", 0, "ve esc");
    assert_eq!(cancelled.composer.text(), "one two three");
    assert_eq!(cancelled.composer.kill_buffer(), "");
    assert_eq!(cancelled.composer.vim_mode(), Some(VimMode::Normal));
}

#[test]
fn undo_groups_and_redo() {
    let mut put = load_byte("one\ntwo", 0);
    assert_eq!(put.composer.text(), "one\ntwo");
    put.press("dd");
    assert_eq!(put.composer.text(), "two");
    put.press("p");
    assert_eq!(put.composer.text(), "two\none");
    put.press("u");
    assert_eq!(put.composer.text(), "two");

    let mut typed = load_byte("", 0);
    typed.press("ihello esc");
    assert_eq!(typed.composer.text(), "hello");
    typed.press("u");
    assert_eq!(typed.composer.text(), "");
    typed.press("ctrl+r");
    assert_eq!(typed.composer.text(), "hello");
    assert_eq!(typed.composer.vim_mode(), Some(VimMode::Normal));

    let mut session = load_byte("", 0);
    session.press("iab");
    session.composer.handle_paste("XYZ");
    session.press("cd esc");
    assert_eq!(session.composer.text(), "abXYZcd");
    session.press("u");
    assert_eq!(session.composer.text(), "");

    let mut outside = load_byte("", 0);
    outside.composer.handle_paste("XYZ");
    assert_eq!(outside.composer.text(), "XYZ");
    assert_eq!(outside.composer.vim_mode(), Some(VimMode::Normal));
    outside.press("u");
    assert_eq!(outside.composer.text(), "");
    outside.press("u");
    assert_eq!(outside.composer.text(), "");

    let word = edited("one two", 0, "dwu");
    assert_eq!(word.composer.text(), "one two");
    assert_eq!(word.composer.caret(), 0);

    let mid_word = edited("alpha beta gamma", 6, "dwu");
    assert_eq!(mid_word.composer.text(), "alpha beta gamma");
    assert_eq!(mid_word.composer.caret(), 6);

    let changed = edited("aa bb", 0, "cw1 esc u");
    assert_eq!(changed.composer.text(), "aa bb");
    assert_eq!(changed.composer.caret(), 0);

    let mut cleared = load_byte("ab", 0);
    cleared.press("x");
    cleared.press("u");
    cleared.press("rX");
    assert_eq!(cleared.composer.text(), "Xb");
    cleared.press("ctrl+r");
    assert_eq!(cleared.composer.text(), "Xb");

    let mut visual = load_byte("ab", 0);
    visual.press("x");
    visual.press("u");
    assert_eq!(visual.composer.text(), "ab");
    visual.press("v");
    assert_eq!(visual.composer.vim_mode(), Some(VimMode::Visual));
    visual.press("ctrl+r");
    assert_eq!(visual.composer.vim_mode(), Some(VimMode::Normal));
    assert_eq!(visual.composer.text(), "b");

    let mut visual_line = load_byte("ab", 0);
    visual_line.press("x");
    visual_line.press("u");
    visual_line.press("V");
    assert_eq!(visual_line.composer.vim_mode(), Some(VimMode::VisualLine));
    visual_line.press("ctrl+r");
    assert_eq!(visual_line.composer.vim_mode(), Some(VimMode::Normal));
    assert_eq!(visual_line.composer.text(), "b");

    let mut empty_redo = load_byte("ab", 0);
    empty_redo.press("v");
    assert_eq!(empty_redo.composer.vim_mode(), Some(VimMode::Visual));
    empty_redo.press("ctrl+r");
    assert_eq!(empty_redo.composer.vim_mode(), Some(VimMode::Visual));
    assert_eq!(empty_redo.composer.text(), "ab");
    empty_redo.press("esc");
    empty_redo.press("V");
    empty_redo.press("ctrl+r");
    assert_eq!(empty_redo.composer.vim_mode(), Some(VimMode::VisualLine));

    let mut depth = load_byte(&"a".repeat(101), 0);
    for _ in 0..101 {
        depth.press("x");
    }
    assert_eq!(depth.composer.text(), "");
    for _ in 0..100 {
        depth.press("u");
    }
    assert_eq!(depth.composer.text().chars().count(), 100);
    depth.press("u");
    assert_ne!(depth.composer.text().chars().count(), 101);

    let mut sent = load_byte("", 0);
    sent.press("ihello esc");
    let submitted = sent.press("enter");
    assert_eq!(
        submitted,
        vec![ComposerOutcome::Submit("hello".to_string())]
    );
    assert_eq!(sent.composer.text(), "");
    assert_eq!(sent.composer.vim_mode(), Some(VimMode::Insert));
    sent.press("esc");
    assert_eq!(sent.composer.vim_mode(), Some(VimMode::Normal));
    sent.press("u");
    assert_eq!(sent.composer.text(), "");

    let mut toggled = load_byte("ab", 0);
    toggled.press("x");
    assert_eq!(toggled.composer.text(), "b");
    toggled.composer.set_vim_enabled(false);
    toggled.composer.set_vim_enabled(true);
    toggled.press("u");
    assert_eq!(toggled.composer.text(), "b");
}

#[test]
fn put_uses_the_shared_kill_buffer() {
    let mut line = load_byte("one\ntwo", 0);
    assert_eq!(line.composer.kill_buffer(), "");
    line.press("dd");
    assert_eq!(line.composer.text(), "two");
    assert_eq!(line.composer.caret(), 0);
    assert_eq!(line.composer.kill_buffer(), "one\n");
    assert!(line.composer.kill_linewise());
    line.press("p");
    assert_eq!(line.composer.text(), "two\none");
    assert_eq!(line.composer.caret(), 4);

    let mut above = load_byte("one\ntwo", 0);
    above.press("ddP");
    assert_eq!(above.composer.text(), "one\ntwo");
    assert_eq!(above.composer.caret(), 0);

    let mut counted = load_byte("one\ntwo", 0);
    counted.press("dd2p");
    assert_eq!(counted.composer.text(), "two\none\none");
    assert_eq!(counted.composer.caret(), 8);

    let mut after = load_byte("ab cd", 0);
    after.press("ye");
    assert_eq!(after.composer.text(), "ab cd");
    assert_eq!(after.composer.caret(), 0);
    assert_eq!(after.composer.kill_buffer(), "ab");
    assert!(!after.composer.kill_linewise());
    after.press("p");
    assert_eq!(after.composer.text(), "aabb cd");
    assert_eq!(after.composer.caret(), 2);

    let mut before = load_byte("ab cd", 0);
    before.press("yeP");
    assert_eq!(before.composer.text(), "abab cd");
    assert_eq!(before.composer.caret(), 1);

    let mut copies = load_byte("ab cd", 0);
    copies.press("ye3p");
    assert_eq!(copies.composer.text(), "aabababb cd");
    assert_eq!(copies.composer.caret(), 6);

    let mut yanked = load_byte("one\ntwo", 0);
    yanked.press("ddi ctrl+y");
    assert_eq!(yanked.composer.text(), "one\ntwo");

    let mut empty = load_byte("ab", 0);
    empty.press("p");
    assert_eq!(empty.composer.text(), "ab");
    assert_eq!(empty.composer.caret(), 0);
    empty.press("P");
    assert_eq!(empty.composer.text(), "ab");
}

#[test]
fn find_character_and_dot_repeat() {
    let mut found = load_byte("one two one", 0);
    assert_eq!(found.composer.text(), "one two one");
    found.press("ft");
    assert_eq!(found.composer.caret(), 4);
    found.press(";");
    assert_eq!(found.composer.caret(), 4);
    found.press(",");
    assert_eq!(found.composer.caret(), 4);
    found.press("Fo");
    assert_eq!(found.composer.caret(), 0);
    let missed = found.press("fz");
    assert_eq!(found.composer.caret(), 0);
    assert_eq!(found.composer.text(), "one two one");
    assert_eq!(missed.last(), Some(&ComposerOutcome::Unchanged));

    let mut dotted = load_byte("aa bb", 0);
    dotted.press("cw1 esc");
    assert_eq!(dotted.composer.text(), "1 bb");
    dotted.press("w");
    dotted.press(".");
    assert_eq!(dotted.composer.text(), "1 1");
    assert_eq!(dotted.composer.vim_mode(), Some(VimMode::Normal));
}

#[test]
fn pending_unknown_and_saturating_counts() {
    let mut rig = load_byte("keep me", 0);
    assert_eq!(rig.composer.text(), "keep me");
    let started = rig.press("d");
    assert_eq!(started, vec![ComposerOutcome::Changed]);
    assert_eq!(rig.composer.text(), "keep me");
    assert!(
        bottom(&mut rig).contains("-- NORMAL --  d"),
        "{}",
        bottom(&mut rig)
    );
    let cancelled = rig.press("z");
    assert_eq!(cancelled, vec![ComposerOutcome::Changed]);
    assert_eq!(rig.composer.text(), "keep me");
    assert!(
        !bottom(&mut rig).contains("-- NORMAL --  d"),
        "{}",
        bottom(&mut rig)
    );
    assert!(
        rig.composer
            .vim_state()
            .and_then(|state| state.pending_label())
            .is_none()
    );

    let mut counted = load_byte("keep me", 0);
    let digit = counted.press("2");
    assert_eq!(digit, vec![ComposerOutcome::Changed]);
    assert!(
        bottom(&mut counted).contains("-- NORMAL --  2"),
        "{}",
        bottom(&mut counted)
    );
    let cleared = counted.press("esc");
    assert_eq!(cleared, vec![ComposerOutcome::Changed]);
    assert_eq!(counted.composer.text(), "keep me");
    assert!(
        counted
            .composer
            .vim_state()
            .and_then(|state| state.pending_label())
            .is_none()
    );

    let mut bare = load_byte("keep me", 0);
    let unknown = bare.press("z");
    assert_eq!(unknown, vec![ComposerOutcome::Unchanged]);
    assert_eq!(bare.composer.text(), "keep me");

    let mut long = load_byte("abcd\nzz", 0);
    long.press("99999999999999999999");
    assert!(
        bottom(&mut long).contains("99999999999999999999"),
        "{}",
        bottom(&mut long)
    );
    long.press("x");
    assert_eq!(long.composer.text(), "\nzz");
    assert_eq!(long.composer.vim_mode(), Some(VimMode::Normal));

    let mut insert = load_byte("", 0);
    insert.press("i123 esc");
    assert_eq!(insert.composer.text(), "123");
    assert_eq!(insert.composer.vim_mode(), Some(VimMode::Normal));
}

#[test]
fn submit_ctrlc_and_history_per_mode() {
    let mut off = Rig::new();
    off.composer.load(" hi ".to_string());
    let submitted = off.press("enter");
    assert_eq!(submitted, vec![ComposerOutcome::Submit(" hi".to_string())]);
    assert_eq!(off.composer.history(), [" hi".to_string()].as_slice());

    let mut cleared = Rig::new();
    cleared.composer.load(" hi ".to_string());
    cleared.press("ctrl+c");
    assert_eq!(cleared.composer.text(), "");
    assert_eq!(cleared.composer.history(), [" hi ".to_string()].as_slice());

    let mut recall = Rig::with_history(vec!["past".to_string()]);
    recall.press("up");
    assert_eq!(recall.composer.text(), "past");
    assert!(recall.composer.last_history_recall());

    let mut normal = load_byte(" hi ", 0);
    let from_normal = normal.press("enter");
    assert_eq!(
        from_normal,
        vec![ComposerOutcome::Submit(" hi".to_string())]
    );
    assert_eq!(normal.composer.history(), [" hi".to_string()].as_slice());
    assert_eq!(normal.composer.text(), "");
    assert_eq!(normal.composer.vim_mode(), Some(VimMode::Insert));

    let mut insert = load_byte(" hi ", 0);
    insert.press("i");
    let from_insert = insert.press("enter");
    assert_eq!(
        from_insert,
        vec![ComposerOutcome::Submit(" hi".to_string())]
    );
    assert_eq!(insert.composer.history(), [" hi".to_string()].as_slice());

    let mut visual = load_byte(" hi ", 0);
    visual.press("v");
    let from_visual = visual.press("enter");
    assert_eq!(from_visual, vec![ComposerOutcome::Changed]);
    assert_eq!(visual.composer.text(), " hi ");
    assert_eq!(visual.composer.vim_mode(), Some(VimMode::Normal));
    assert!(visual.composer.history().is_empty());

    let mut normal_text = load_byte("keep", 0);
    normal_text.press("ctrl+c");
    assert_eq!(normal_text.composer.text(), "keep");
    assert!(normal_text.composer.history().is_empty());
    assert_eq!(normal_text.composer.vim_mode(), Some(VimMode::Normal));

    let mut normal_empty = load_byte("", 0);
    normal_empty.set_running(true);
    let interrupted = normal_empty.press("ctrl+c");
    assert_eq!(interrupted, vec![ComposerOutcome::Interrupt]);

    let mut normal_idle = load_byte("", 0);
    let idle = normal_idle.press("ctrl+c");
    assert_eq!(idle, vec![ComposerOutcome::Unchanged]);
    assert_ne!(idle, vec![ComposerOutcome::ExitRequested]);

    let mut insert_text = load_byte("keep", 0);
    insert_text.press("i ctrl+c");
    assert_eq!(insert_text.composer.text(), "");
    assert_eq!(
        insert_text.composer.history(),
        ["keep".to_string()].as_slice()
    );

    let mut visual_cancel = load_byte("keep", 0);
    visual_cancel.press("v ctrl+c");
    assert_eq!(visual_cancel.composer.text(), "keep");
    assert_eq!(visual_cancel.composer.vim_mode(), Some(VimMode::Normal));

    let mut moved = Rig::with_history(vec!["past".to_string()]);
    moved.composer.set_vim_enabled(true);
    moved.composer.load("ab\ncd".to_string());
    moved.composer.set_caret(3);
    moved.press("up");
    assert_eq!(moved.composer.text(), "ab\ncd");
    assert!(!moved.composer.last_history_recall());
    assert_eq!(moved.composer.caret(), 0);
    assert_eq!(moved.composer.history(), ["past".to_string()].as_slice());
}

#[test]
fn paste_burst_and_load_in_vim_modes() {
    let mut fast = load_byte("one two", 0);
    assert!(
        fast.composer.next_tick().is_none(),
        "no burst deadline in normal"
    );
    assert!(!fast.composer.is_collecting_burst());
    fast_keys(&mut fast, "dwd");
    assert_eq!(fast.composer.text(), "two");
    assert!(!fast.composer.text().contains("dwd"));
    assert!(!fast.composer.is_collecting_burst());
    assert!(fast.composer.next_tick().is_none());
    fast.press("esc");
    assert_eq!(fast.composer.text(), "two");

    let mut cuts = load_byte("abcd", 0);
    fast_keys(&mut cuts, "xxx");
    assert_eq!(cuts.composer.text(), "d");
    assert!(!cuts.composer.is_collecting_burst());
    assert!(cuts.composer.next_tick().is_none());

    let mut normal = load_byte("keep", 4);
    normal.composer.handle_paste("XYZ");
    assert_eq!(normal.composer.text(), "keepXYZ");
    assert_eq!(normal.composer.vim_mode(), Some(VimMode::Normal));
    normal.press("u");
    assert_eq!(normal.composer.text(), "keep");

    let mut visual = load_byte("one two", 0);
    visual.press("ve");
    visual.composer.handle_paste("ZZ");
    assert_eq!(visual.composer.text(), "ZZ two");
    assert_eq!(visual.composer.vim_mode(), Some(VimMode::Normal));
    visual.press("u");
    assert_eq!(visual.composer.text(), "one two");

    let mut session = load_byte("", 0);
    session.press("iab");
    session.composer.handle_paste("XYZ");
    session.press("cd esc");
    assert_eq!(session.composer.text(), "abXYZcd");
    session.press("u");
    assert_eq!(session.composer.text(), "");

    let mut loaded = load_byte("old", 0);
    assert_eq!(loaded.composer.text(), "old");
    loaded.composer.load("new".to_string());
    assert_eq!(loaded.composer.text(), "new");
    loaded.press("u");
    assert_eq!(loaded.composer.text(), "old");
    assert_eq!(loaded.composer.vim_mode(), Some(VimMode::Normal));
    loaded.press("u");
    assert_eq!(loaded.composer.text(), "old");
}

#[test]
fn random_vim_keys_keep_invariants() {
    let drafts = [
        "a🙂b x",
        "e\u{0301} cafe",
        "  two  lines\n\nhere",
        "one two\nthree",
    ];
    let mut rng = Rng(SEED);
    for draft in drafts {
        let mut rig = load_byte(draft, 0);
        rig.set_running(true);
        assert!(on_boundary(rig.composer.text(), rig.composer.caret()));
        assert!(forbidden(rig.composer.text()).is_none());
        for _ in 0..24 {
            let sample = rng.key();
            let before = rig.composer.text().to_string();
            let caret = rig.composer.caret();
            let before_mode = rig.composer.vim_mode();
            let kill = rig.composer.kill_buffer().to_string();
            rig.now += Duration::from_millis(100);
            let outcome = rig.composer.handle_key(
                sample.key,
                ComposerContext { turn_running: true },
                rig.now,
            );
            match outcome {
                ComposerOutcome::Submit(_) => {
                    assert!(sample.kind == KeyKind::Enter, "submit from {sample:?}");
                }
                ComposerOutcome::Interrupt => {
                    assert!(
                        matches!(sample.kind, KeyKind::Esc | KeyKind::CtrlC),
                        "interrupt from {sample:?}"
                    );
                }
                ComposerOutcome::ExitRequested => panic!("exit requested from {sample:?}"),
                ComposerOutcome::Unchanged | ComposerOutcome::Changed => {}
            }
            let after = rig.composer.text();
            assert!(forbidden(after).is_none(), "{after:?}");
            assert!(
                on_boundary(after, rig.composer.caret()),
                "caret {}",
                rig.composer.caret()
            );
            assert!(rig.composer.vim_mode().is_some(), "mode after {sample:?}");
            if let Some(ch) = sample.letter {
                let typing = inserted_only(before.as_str(), after, caret, ch);
                let put = matches!(ch, 'p' | 'P') && kill == ch.to_string();
                let entered = matches!(rig.composer.vim_mode(), Some(VimMode::Insert))
                    || matches!(before_mode, Some(VimMode::Insert));
                if typing && !put && !entered {
                    panic!("letter {ch} was inserted from mode {before_mode:?}");
                }
            }
        }
    }
}

fn load_byte(text: &str, byte: usize) -> Rig {
    let mut rig = Rig::new();
    rig.composer.set_vim_enabled(true);
    rig.composer.load(text.to_string());
    rig.composer.set_caret(byte);
    rig
}

fn edited(text: &str, byte: usize, script: &str) -> Rig {
    let mut rig = load_byte(text, byte);
    rig.press(script);
    rig
}

fn motion(text: &str, byte: usize, script: &str) -> usize {
    let mut rig = motion_rig(text, byte);
    rig.press(script);
    assert_eq!(rig.composer.text(), text, "{script}");
    assert!(!rig.composer.last_history_recall(), "{script}");
    assert_eq!(rig.composer.history(), ["old".to_string()].as_slice());
    assert_eq!(rig.composer.vim_mode(), Some(VimMode::Normal), "{script}");
    rig.composer.caret()
}

fn motion_rig(text: &str, byte: usize) -> Rig {
    let mut rig = Rig::with_history(vec!["old".to_string()]);
    rig.composer.set_vim_enabled(true);
    rig.composer.load(text.to_string());
    rig.composer.set_caret(byte);
    assert_eq!(rig.composer.text(), text);
    rig
}

fn drawn(rig: &mut Rig) -> Vec<String> {
    let section = rig.composer.section(60, 10);
    let height = section.height(60);
    rows(&section, 60, height)
}

fn bottom(rig: &mut Rig) -> String {
    drawn(rig).last().cloned().unwrap_or_default()
}

fn fast_keys(rig: &mut Rig, text: &str) {
    for ch in text.chars() {
        rig.now += Duration::from_millis(1);
        let key = Key::from(KeyEvent::new(KeyCode::Char(ch), KeyModifiers::NONE));
        rig.composer.handle_key(
            key,
            ComposerContext {
                turn_running: rig_running(rig),
            },
            rig.now,
        );
    }
}

fn rig_running(rig: &Rig) -> bool {
    let _ = rig;
    false
}

fn on_boundary(text: &str, caret: usize) -> bool {
    if caret > text.len() || !text.is_char_boundary(caret) {
        return false;
    }
    let mut index = 0;
    loop {
        let at = byte_of_cluster(text, index);
        if at == caret {
            return true;
        }
        if at >= text.len() {
            return false;
        }
        index += 1;
    }
}

fn forbidden(text: &str) -> Option<char> {
    text.chars()
        .find(|ch| ch.is_control() && *ch != '\n' && *ch != '\t')
}

fn inserted_only(before: &str, after: &str, caret: usize, ch: char) -> bool {
    let at = caret.min(before.len());
    if !before.is_char_boundary(at) {
        return false;
    }
    let mut expected = String::new();
    expected.push_str(&before[..at]);
    expected.push(ch);
    expected.push_str(&before[at..]);
    after == expected
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum KeyKind {
    Enter,
    Esc,
    CtrlC,
    Other,
}

#[derive(Clone, Copy, Debug)]
struct Sample {
    key: Key,
    kind: KeyKind,
    letter: Option<char>,
}

struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 = self
            .0
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1);
        self.0
    }

    fn index_below(&mut self, count: usize) -> usize {
        (self.next() as usize) % count
    }

    fn key(&mut self) -> Sample {
        let letters = b"abcdefghijklmnopqrstuvwxyz";
        match self.index_below(10) {
            0 => Sample {
                key: Key::from(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE)),
                kind: KeyKind::Esc,
                letter: None,
            },
            1 => Sample {
                key: Key::from(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)),
                kind: KeyKind::Enter,
                letter: None,
            },
            2 => Sample {
                key: Key::from(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL)),
                kind: KeyKind::CtrlC,
                letter: None,
            },
            3 => Sample {
                key: Key::from(KeyEvent::new(KeyCode::Char('r'), KeyModifiers::CONTROL)),
                kind: KeyKind::Other,
                letter: None,
            },
            4 => Sample {
                key: Key::from(KeyEvent::new(KeyCode::Up, KeyModifiers::NONE)),
                kind: KeyKind::Other,
                letter: None,
            },
            5 => Sample {
                key: Key::from(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE)),
                kind: KeyKind::Other,
                letter: None,
            },
            6 => Sample {
                key: Key::from(KeyEvent::new(KeyCode::Left, KeyModifiers::NONE)),
                kind: KeyKind::Other,
                letter: None,
            },
            7 => Sample {
                key: Key::from(KeyEvent::new(KeyCode::Right, KeyModifiers::NONE)),
                kind: KeyKind::Other,
                letter: None,
            },
            _ => {
                let ch = letters[self.index_below(letters.len())] as char;
                let shift = self.index_below(5) == 0;
                let code = if shift {
                    KeyCode::Char(ch.to_ascii_uppercase())
                } else {
                    KeyCode::Char(ch)
                };
                let mods = if shift {
                    KeyModifiers::SHIFT
                } else {
                    KeyModifiers::NONE
                };
                Sample {
                    key: Key::from(KeyEvent::new(code, mods)),
                    kind: KeyKind::Other,
                    letter: Some(if shift { ch.to_ascii_uppercase() } else { ch }),
                }
            }
        }
    }
}
