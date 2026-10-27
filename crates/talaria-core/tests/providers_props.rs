//! Hostile provider text is drawn without control characters.

#[path = "support/providers.rs"]
mod support;

use proptest::prelude::*;
use support::{env, joined, keys_tab, open, step};
use talaria_core::providers::ProvidersInput;

proptest! {
    #[test]
    fn hostile_text_is_sanitised(extra in "[A-Za-z]{0,12}") {
        let escape = '\u{1b}';
        let mark = '\u{202e}';
        let stand_in = '\u{241b}';
        let raw = format!("A{escape}{mark}{extra}");
        let (mut state, _) = open(keys_tab(), true);
        let mut row = env("EXAMPLE_API_KEY", "example", &raw, true);
        row.tail = Some(format!("«redacted:abcd...wxyz{escape}»"));
        let _ = step(&mut state, ProvidersInput::EnvLoaded(Ok(vec![row])));
        let text = joined(&state);
        prop_assert!(!text
            .chars()
            .any(|ch| ch != '\n' && (ch.is_control() || ch == mark)));
        prop_assert!(
            text.contains(stand_in),
            "escape is drawn as a stand-in: {}",
            text
        );
        prop_assert!(
            text.contains("‹U+202E›"),
            "a direction mark is drawn as a stand-in: {}",
            text
        );
        prop_assert!(text.contains(&extra) || extra.is_empty());
        prop_assert!(!text.contains("abcd..."));
    }
}
