//! Built-in files use the same strict parser as user themes.
use super::{ThemeFile, parse};

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
