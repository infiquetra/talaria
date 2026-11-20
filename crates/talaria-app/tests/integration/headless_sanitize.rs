//! Headless and launch text must not let separators, zero-width marks, or tags through.

use talaria_app::headless::output::{neutralise_line, terminal_safe};
use talaria_app::launch::plain;

const HOSTILE: &[char] = &[
    '\u{2028}',
    '\u{2029}',
    '\u{200B}',
    '\u{200C}',
    '\u{200D}',
    '\u{2060}',
    '\u{FEFF}',
    '\u{E0041}',
    '\u{1b}',
    '\u{85}',
];

#[test]
fn terminal_safe_hostile_text_is_neutralised() {
    for ch in HOSTILE {
        let out = terminal_safe(&format!("a{ch}b"));
        assert_eq!(out, "a?b", "U+{:04X} leaked as {out:?}", u32::from(*ch));
        assert!(!out.contains(*ch));
    }
    assert_eq!(terminal_safe("a\nb\tc"), "a\nb\tc");
    assert_eq!(terminal_safe("\n\t"), "\n\t");
    assert_eq!(terminal_safe("x\u{2028}\n\t\u{E0041}\u{1b}"), "x?\n\t??");
}

#[test]
fn plain_hostile_text_is_neutralised() {
    for ch in HOSTILE {
        let out = plain(&format!("a{ch}b"));
        assert_eq!(out, "ab", "U+{:04X} leaked as {out:?}", u32::from(*ch));
        assert!(!out.contains(*ch));
    }
    assert_eq!(plain("a\u{1b}[2Jb\u{2028}c"), "abc");
    assert_eq!(plain("keep\u{1b}[31m me"), "keep me");
}

#[test]
fn neutralise_line_hostile_text_is_neutralised() {
    for ch in HOSTILE {
        let out = neutralise_line(&format!("a{ch}b"));
        assert_eq!(out, "a?b", "U+{:04X} leaked as {out:?}", u32::from(*ch));
        assert!(!out.contains(*ch));
    }
    assert_eq!(neutralise_line("a\nb\rc\td"), "a?b?c?d");
}
