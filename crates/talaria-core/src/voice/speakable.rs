//! Turn Markdown into text the gateway can speak.

/// Spoken form of `markdown`, or nothing when no letter or digit remains.
///
/// The body is a stub: it returns nothing, so the speech tests fail by assertion.
#[must_use]
pub fn speakable_text(markdown: &str, max_chars: usize) -> Option<String> {
    let _ = (markdown, max_chars);
    None
}

#[cfg(test)]
mod tests {
    use super::speakable_text;

    #[test]
    fn markdown_cases() {
        let word = "Word. ".repeat(300);
        let thirteen: String = std::iter::repeat("Word.")
            .take(13)
            .collect::<Vec<_>>()
            .join(" ");
        let truncated = format!("{thirteen} (reply truncated)");
        assert_eq!(
            truncated.chars().count(),
            95,
            "the pinned cut is 95 characters"
        );
        let letters = "a".repeat(100);
        let cut_letters = format!("{} (reply truncated)", "a".repeat(82));
        assert_eq!(cut_letters.chars().count(), 100);
        let prose_100 = format!("A fine day. {}", "x".repeat(88));
        assert_eq!(prose_100.chars().count(), 100);

        let cases: &[(&str, usize, Option<&str>)] = &[
            (
                "# Done\n\nAll **tests** pass.",
                1500,
                Some("Done All tests pass."),
            ),
            (
                "Run:\n\n```bash\ncargo test\n```\nThen commit.",
                1500,
                Some("Run: (code omitted) Then commit."),
            ),
            (
                "See [the docs](https://x.example/a).",
                1500,
                Some("See the docs."),
            ),
            (
                "Visit https://x.example now.",
                1500,
                Some("Visit (link) now."),
            ),
            ("Use `cargo fmt`.", 1500, Some("Use cargo fmt.")),
            (
                "| a | b |\n|---|---|\n| 1 | 2 |",
                1500,
                Some("(table omitted)"),
            ),
            (
                "Name | Value\n--- | ---\na | 1",
                1500,
                Some("(table omitted)"),
            ),
            ("Pick a | b carefully.", 1500, Some("Pick a | b carefully.")),
            ("Use the http crate.", 1500, Some("Use the http crate.")),
            ("a ` b", 1500, Some("a ` b")),
            ("x](y", 1500, Some("x](y")),
            ("![chart](c.png)", 1500, Some("chart")),
            ("- one\n- two", 1500, Some("one two")),
            ("> quoted", 1500, Some("quoted")),
            ("***", 1500, None),
            ("```\nfn main() {}\n```", 1500, Some("(code omitted)")),
            ("Grüße, café. Fin.", 1500, Some("Grüße, café. Fin.")),
            ("Visit https://x.example.", 1500, Some("Visit (link).")),
            (
                "See [docs][1].\n\n[1]: https://x.example",
                1500,
                Some("See docs."),
            ),
            ("![](c.png) Done.", 1500, Some("Done.")),
            ("Up\n\n---\n\nDown", 1500, Some("Up Down")),
            ("    let x = 1;", 1500, Some("let x = 1;")),
        ];
        for (input, max_chars, expected) in cases {
            assert_eq!(
                speakable_text(input, *max_chars).as_deref(),
                *expected,
                "{input:?}"
            );
        }
        assert_eq!(
            speakable_text(&word, 100).as_deref(),
            Some(truncated.as_str()),
            "a long run of sentences cuts at the limit"
        );
        assert_eq!(
            speakable_text(&letters, 100).as_deref(),
            Some(cut_letters.as_str()),
            "a run with no break still fits the limit"
        );
        assert_eq!(
            speakable_text(&prose_100, 100).as_deref(),
            Some(prose_100.as_str()),
            "prose that already fits is unchanged"
        );
    }
}
