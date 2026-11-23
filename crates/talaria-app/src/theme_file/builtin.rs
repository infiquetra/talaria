//! Built-in files use the same strict parser as user themes.
use super::{ThemeFile, parse};

/// The built-in theme identifiers, in the order the `/theme` selector lists them.
pub const BUILTIN_THEMES: [&str; 3] = ["terminal", "ember", "paper"];

/// One line describing a built-in theme for the `/theme` selector, from `docs/themes.md`.
#[must_use]
pub fn builtin_description(id: &str) -> Option<&'static str> {
    Some(match id {
        "terminal" => "Terminal defaults and named colours; follows the Hermes skin",
        "ember" => "A warm dark theme; ignores Hermes skins",
        "paper" => "A light theme with blue and brown accents; ignores Hermes skins",
        _ => return None,
    })
}

#[must_use]
pub fn builtin(id: &str) -> Option<ThemeFile> {
    let source = match id {
        "terminal" => include_str!("../../themes/terminal.toml"),
        "ember" => include_str!("../../themes/ember.toml"),
        "paper" => include_str!("../../themes/paper.toml"),
        _ => return None,
    };
    parse(source).ok()
}
