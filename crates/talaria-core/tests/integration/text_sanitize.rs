//! Doctor scrubbing keeps redaction and uses the shared unsafe-character table.

use talaria_core::doctor::scrub_text;
use talaria_core::text::{Replacement, SanitizeOptions, sanitize, sanitize_with_options};

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
fn scrub_text_hostile_text_is_neutralised() {
    for ch in HOSTILE {
        let out = scrub_text(&format!("a{ch}b"), None);
        assert_eq!(out, "a?b", "U+{:04X} leaked as {out:?}", u32::from(*ch));
        assert!(!out.contains(*ch));
    }
    assert_eq!(scrub_text("see /tmp/secret", None), "see <path>");
    assert_eq!(scrub_text(&"x".repeat(40), None), "<redacted>");
    let mixed = format!("a\u{200B}{}", "x".repeat(40));
    assert_eq!(scrub_text(&mixed, None), "a?<redacted>");
    assert!(!scrub_text(&mixed, None).contains('\u{200B}'));
}

#[test]
fn sanitize_with_options_keeps_display_joiners_and_layout_newlines() {
    assert_eq!(sanitize("a\u{200B}b").as_ref(), "a\u{200B}b");
    assert_eq!(sanitize("\n").as_ref(), "\u{240a}");
    assert_eq!(
        sanitize_with_options(
            "a\u{200B}\n\tb\u{2028}\u{E0041}",
            SanitizeOptions::LAYOUT,
            Replacement::Char('?'),
        )
        .as_ref(),
        "a?\n\tb??"
    );
    assert_eq!(
        sanitize_with_options(
            "a\u{200C}\u{200D}\u{2060}\u{FEFF}b",
            SanitizeOptions::CLI,
            Replacement::Drop
        )
        .as_ref(),
        "ab"
    );
}
