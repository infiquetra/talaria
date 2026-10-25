//! External editor: resolution, hand-off, and result handling.
//!
//! Stubs. Behavior contract: issue 228.

use std::path::Path;

/// Why an editor command could not be read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EditorError {
    /// Shell-style split failed; holds the reason.
    Parse(String),
}

/// What an editor run produced. `Failed` holds the notice text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EditResult {
    /// Replace the draft with this cleaned text, unsent.
    Loaded(String),
    /// The edited text was empty; the draft was cleared.
    Cleared,
    /// The draft is unchanged; `hint` asks for the wait-flag notice.
    Unchanged {
        /// Whether to show the wait-flag notice.
        hint: bool,
    },
    /// The draft is unchanged; holds the notice text.
    Failed(String),
}

/// Pick the editor argument list: config, environment, well-known names.
pub fn resolve_editor(
    config: Option<&str>,
    env: &dyn Fn(&str) -> Option<String>,
    is_executable_on_path: &dyn Fn(&str) -> bool,
) -> Result<Vec<String>, EditorError> {
    let _ = (config, env, is_executable_on_path);
    todo!("#228")
}

/// Run the editor on the draft and classify the result. Blocking.
pub fn edit_draft(argv: &[String], draft: &str, cwd: &Path, tmp_dir: &Path) -> EditResult {
    let _ = (argv, draft, cwd, tmp_dir);
    todo!("#228")
}

/// The notice for an unreadable editor command. The caller shows it on `Parse`.
pub fn parse_notice(reason: &str) -> String {
    let _ = reason;
    todo!("#228")
}

/// The notice for an editor that returned at once without changes.
pub fn unchanged_hint_notice() -> String {
    todo!("#228")
}

/// The notice for an edit that cleared the draft.
pub fn cleared_notice() -> String {
    todo!("#228")
}
