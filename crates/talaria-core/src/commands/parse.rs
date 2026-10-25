//! Slash-line grammar for issue #223.
//!
//! A slash command is `/` at position 0, then a name with no whitespace and no second `/`,
//! then whitespace or the end of the text. This is the shape both Hermes clients use, so
//! `/usr/local` and `run /clean` stay prose.

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
    let Some(rest) = text.strip_prefix('/') else {
        return false;
    };
    for ch in rest.chars() {
        if ch.is_whitespace() {
            return true;
        }
        if ch == '/' {
            return false;
        }
    }
    true
}

/// Split one line. `None` when it is ordinary prose.
///
/// A bare `/` (or `/` followed only by whitespace) is a line with an empty name, which runs
/// nothing. `/ words` is prose, because the name is empty and an argument follows.
#[must_use]
pub fn parse_slash_line(text: &str) -> Option<SlashLine> {
    if !looks_like_slash_command(text) {
        return None;
    }
    let rest = &text[1..];
    let (name, arg) = match rest.find(char::is_whitespace) {
        Some(split) => (&rest[..split], &rest[split..]),
        None => (rest, ""),
    };
    let name = name.to_lowercase();
    let arg = arg.trim();
    if name.is_empty() && !arg.is_empty() {
        return None;
    }
    Some(SlashLine {
        name,
        arg: arg.to_owned(),
    })
}
