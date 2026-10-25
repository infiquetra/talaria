//! Slash-line grammar for issue #223.

/// A slash command split into a lower-case name and an outer-trimmed argument.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SlashLine {
    /// Name without the leading slash, lower-cased. Empty for a bare `/`.
    pub name: String,
    /// Argument with interior whitespace kept.
    pub arg: String,
}

/// True when `text` starts with a slash command shape.
#[must_use]
pub fn looks_like_slash_command(text: &str) -> bool {
    let _ = text;
    todo!()
}

/// Split one line. `None` when it is ordinary prose.
#[must_use]
pub fn parse_slash_line(text: &str) -> Option<SlashLine> {
    let _ = text;
    todo!()
}
