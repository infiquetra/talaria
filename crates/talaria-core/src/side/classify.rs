//! Routing a raw draft to shell, empty shell, or prompt.

/// What the raw draft asks for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Submission {
    /// Run this command, trimmed, as one script even when it spans lines.
    Shell(String),
    /// A `!` with nothing after it.
    ShellEmpty,
    /// Anything else goes on to the slash dispatch and the prompt path.
    Prompt,
}

/// Route on the first character only. Runs before mentions expand.
///
/// A leading space or a later `!` makes a prompt, which is also how to send the agent a prompt
/// that starts with `!`.
pub fn classify(raw_draft: &str) -> Submission {
    match raw_draft.strip_prefix('!') {
        None => Submission::Prompt,
        Some(rest) => {
            let command = rest.trim();
            if command.is_empty() {
                Submission::ShellEmpty
            } else {
                Submission::Shell(command.to_string())
            }
        }
    }
}
