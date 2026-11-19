//! The CLI helpers must not let separators, zero-width marks, or tags through.

use talaria_cli::args::neutralise;
use talaria_cli::remote::visible;
use talaria_cli::workflow::safe;

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
fn visible_hostile_text_is_neutralised() {
    for ch in HOSTILE {
        let out = visible(&format!("a{ch}b"));
        assert_eq!(out, "ab", "U+{:04X} leaked as {out:?}", u32::from(*ch));
        assert!(!out.contains(*ch));
    }
    assert_eq!(visible(&"a".repeat(300)).chars().count(), 256);
}

#[test]
fn safe_hostile_text_is_neutralised() {
    for ch in HOSTILE {
        let out = safe(&format!("a{ch}b"));
        assert_eq!(out, "a?b", "U+{:04X} leaked as {out:?}", u32::from(*ch));
        assert!(!out.contains(*ch));
    }
    assert_eq!(safe("a\nb\rc\td"), "a\\nb\\rc?d");
}

#[test]
fn neutralise_hostile_text_is_neutralised() {
    for ch in HOSTILE {
        let out = neutralise(&format!("a{ch}b"));
        assert_eq!(out, "a?b\n", "U+{:04X} leaked as {out:?}", u32::from(*ch));
        assert!(!out.contains(*ch));
    }
    assert_eq!(neutralise("a\nb\tc"), "a\nb\tc\n");
    assert_eq!(neutralise("\n"), "\n");
    assert_eq!(neutralise("\t"), "\t\n");
    assert_eq!(neutralise("x\u{2028}\n\t\u{E0041}"), "x?\n\t?\n");
    assert!(
        !neutralise("a\u{1b}[2J\nb").contains('\u{1b}'),
        "escape must not survive"
    );
}

#[test]
fn neutralise_keeps_layout_past_the_quote_limit() {
    let sixty_four = "a".repeat(64);
    assert_eq!(
        neutralise(&format!("`{sixty_four}\nhidden`")),
        format!("`{sixty_four}\n`\n")
    );
    assert_eq!(
        neutralise(&format!("'{sixty_four}\thidden'")),
        format!("'{sixty_four}\t'\n")
    );
    let long = "b".repeat(80);
    assert_eq!(
        neutralise(&format!("`{long}\n\tsecret`")),
        format!("`{}\n\t`\n", "b".repeat(64))
    );
    let plain = neutralise(&format!("`{}`", "c".repeat(80)));
    assert_eq!(plain, format!("`{}`\n", "c".repeat(64)));
    assert!(!plain.contains(&"c".repeat(65)));
}
