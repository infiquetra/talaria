//! Text comparisons used by assistant segments. Whitespace policy lives here so chunking
//! cannot change a cell.

/// Drop leading `\r` and `\n` only. Spaces and tabs stay, so a chunk of spaces opens a cell.
pub fn trim_leading_breaks(chunk: &str) -> &str {
    let _ = chunk;
    todo!("P1-05")
}

/// Remove CSI sequences (`ESC [` … final byte) and any other `ESC` plus the following byte.
/// The caller treats a blank remainder as absent.
pub fn strip_terminal_escapes(text: &str) -> String {
    let _ = text;
    todo!("P1-05")
}

/// Delete every Unicode whitespace character. Used to compare a final reply with what was shown.
pub fn remove_whitespace(text: &str) -> String {
    let _ = text;
    todo!("P1-05")
}

/// The tail of `original` after `kept` non-whitespace characters, with leading whitespace removed.
pub fn text_after_non_whitespace(original: &str, kept: usize) -> String {
    let _ = (original, kept);
    todo!("P1-05")
}

#[cfg(test)]
mod tests {
    use super::{
        remove_whitespace, strip_terminal_escapes, text_after_non_whitespace, trim_leading_breaks,
    };

    #[test]
    fn leading_breaks_drop_and_spaces_stay() {
        assert_eq!(trim_leading_breaks("\n\nHi"), "Hi");
        assert_eq!(trim_leading_breaks("\r\n"), "");
        assert_eq!(trim_leading_breaks("  "), "  ");
        assert_eq!(trim_leading_breaks("  x"), "  x");
    }

    #[test]
    fn escapes_are_removed_and_a_blank_remainder_is_empty() {
        let stripped = strip_terminal_escapes("\u{1b}[31mred\u{1b}[0m");
        assert_eq!(stripped, "red");
        assert!(strip_terminal_escapes("\u{1b}[0m").trim().is_empty());
        assert!(strip_terminal_escapes("review diff\na/old -> b/new").contains("review diff"));
    }

    #[test]
    fn whitespace_removal_and_remainder_follow_the_non_whitespace_count() {
        assert_eq!(remove_whitespace("Checking.\n\nDone."), "Checking.Done.");
        assert_eq!(
            text_after_non_whitespace("Checking.\n\nDone.", "Checking.".chars().count()),
            "Done."
        );
        assert_eq!(text_after_non_whitespace("xyz", 0), "xyz");
    }
}
