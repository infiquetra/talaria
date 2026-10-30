//! Renderer uses the shared character table.

#[test]
fn renderer_and_core_share_the_same_sanitizer() {
    let input = "a\u{1b}b\u{7}c\u{202e}d";
    assert_eq!(
        talaria_tui::history::sanitize(input),
        talaria_core::text::sanitize(input)
    );
    let core = include_str!("../../talaria-core/src/text.rs");
    let terminal = include_str!("../src/history.rs");
    assert!(core.contains("fn stand_in"));
    assert!(!terminal.contains("fn stand_in"));
}
