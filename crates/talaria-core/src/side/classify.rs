//! Routing a raw draft to shell, empty shell, or prompt.

/// What the raw draft asks for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Submission {
    Shell(String),
    ShellEmpty,
    Prompt,
}

/// Route on the first character only. Runs before mentions expand.
pub fn classify(raw_draft: &str) -> Submission {
    let _ = raw_draft;
    todo!("#225")
}
