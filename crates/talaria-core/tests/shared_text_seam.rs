//! Shared text safety and ownership.

#[test]
fn shared_sanitizer_preserves_visible_replacements_and_single_table() {
    let input = "a\u{1b}b\u{7}c\u{202e}d";
    assert_eq!(talaria_core::text::sanitize(input), "a␛b␇c‹U+202E›d");
    let core = include_str!("../src/text.rs");
    let terminal = include_str!("../../talaria-tui/src/history.rs");
    assert!(
        core.contains("fn stand_in"),
        "the character table belongs to core"
    );
    assert!(
        !terminal.contains("fn stand_in"),
        "the renderer must not keep a second table"
    );
}
