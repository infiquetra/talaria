//! The prompt text: one argument, piped input, or both joined.

/// Largest accepted size of piped input, and of the joined text.
pub const MAX_STDIN_BYTES: usize = 1_048_576;

/// Why no text could be sent.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PromptError {
    Empty,
    StdinTooLarge,
    PromptTooLarge,
    NotUtf8,
}

/// Join the argument and the piped bytes into the text to send.
pub fn assemble_prompt(arg: Option<&str>, stdin: Option<&[u8]>) -> Result<String, PromptError> {
    let _ = (arg, stdin);
    todo!("#235")
}

#[cfg(test)]
mod tests {
    use super::{MAX_STDIN_BYTES, PromptError, assemble_prompt};

    #[test]
    fn prompt_from_argument_and_stdin() {
        // The join rows: each side alone gives itself, both join with two
        // newlines, and bytes pass through unchanged.
        assert_eq!(assemble_prompt(Some("hello"), None).as_deref(), Ok("hello"));
        assert_eq!(
            assemble_prompt(Some("facet"), Some(b"")).as_deref(),
            Ok("facet")
        );
        assert_eq!(
            assemble_prompt(Some("ask"), Some(b"piped")).as_deref(),
            Ok("ask\n\npiped")
        );
        assert_eq!(assemble_prompt(None, Some(b"solo")).as_deref(), Ok("solo"));
        assert!(matches!(
            assemble_prompt(None, None),
            Err(PromptError::Empty)
        ));
        assert_eq!(
            assemble_prompt(Some("a\0b"), Some(b"c\0d")).as_deref(),
            Ok("a\0b\n\nc\0d")
        );
    }

    #[test]
    fn stdin_limits_and_utf8() {
        let cap = vec![b'x'; MAX_STDIN_BYTES];
        assert!(assemble_prompt(None, Some(&cap)).is_ok());
        let over = vec![b'x'; MAX_STDIN_BYTES + 1];
        assert_eq!(
            assemble_prompt(None, Some(&over)),
            Err(PromptError::StdinTooLarge)
        );
        // The joined text over the cap is refused even when each part fits.
        assert_eq!(
            assemble_prompt(Some("a"), Some(&cap)),
            Err(PromptError::PromptTooLarge)
        );
        let big_arg = "y".repeat(MAX_STDIN_BYTES + 1);
        assert_eq!(
            assemble_prompt(Some(&big_arg), None),
            Err(PromptError::PromptTooLarge)
        );
        assert_eq!(
            assemble_prompt(None, Some(&[0xff, 0xfe])),
            Err(PromptError::NotUtf8)
        );
        assert_eq!(
            assemble_prompt(Some("   "), Some(b" \n ")),
            Err(PromptError::Empty)
        );
        assert_eq!(assemble_prompt(Some(""), None), Err(PromptError::Empty));
    }
}
