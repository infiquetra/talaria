//! Properties of spoken text.

use proptest::prelude::*;
use proptest::test_runner::FileFailurePersistence;
use talaria_core::voice::speakable::speakable_text;

fn few_cases() -> ProptestConfig {
    let mut config = ProptestConfig::with_cases(8);
    config.failure_persistence = Some(Box::new(FileFailurePersistence::Off));
    config
}

proptest! {
    #![proptest_config(few_cases())]

    #[test]
    fn bounded_and_control_free(
        text in "\\PC{0,80}",
        max_chars in 100usize..10_001,
    ) {
        let spoken = speakable_text("hello", 1500).expect("plain prose is spoken");
        prop_assert_eq!(spoken, "hello");
        if let Some(out) = speakable_text(&text, max_chars) {
            prop_assert!(out.chars().count() <= max_chars);
            prop_assert!(!out.chars().any(|ch| ch.is_control()));
        }
    }

    #[test]
    fn generated_markdown_loses_bodies_and_destinations(
        prose in "[A-Za-z]{4,12}",
        secret in "[a-z]{6,10}",
        host in "[a-z]{4,8}",
    ) {
        let markdown = format!(
            "{prose}\n\n```\n{secret}\n```\n\nSee [docs](https://{host}.example/{secret}) and https://{host}.example/{secret}/raw."
        );
        let spoken = speakable_text(&markdown, 1500).unwrap_or_default();
        prop_assert!(spoken.contains(&prose), "prose kept: {spoken:?}");
        prop_assert!(!spoken.contains(&secret), "a fenced body or destination remained: {spoken:?}");
        prop_assert!(!spoken.contains(&host), "a link destination remained: {spoken:?}");
    }
}
