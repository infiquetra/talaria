//! The launch the operator asked for, and the worktree names derived from it.

use std::collections::BTreeSet;
use std::path::PathBuf;

/// One background launch.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LaunchRequest {
    /// Sent byte for byte. Never trimmed.
    pub prompt: String,
    /// The profile the session is created in.
    pub profile: String,
    /// Canonical existing folder the launch starts from.
    pub folder: PathBuf,
    /// Passed through when the operator set one.
    pub model: Option<String>,
    /// Passed through when the operator set one.
    pub provider: Option<String>,
    /// Already accepted. Sent as `reasoning`.
    pub effort: Option<String>,
    /// Set when the launch wants a fresh worktree.
    pub worktree: Option<WorktreeWanted>,
    /// Where the request came from.
    pub origin: LaunchOrigin,
}

/// A worktree the operator asked for. `name` is the explicit slug, when one was typed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WorktreeWanted {
    /// `--name`, or the dialog field. `None` means the prompt supplies the slug.
    pub name: Option<String>,
}

/// Where the launch was started.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LaunchOrigin {
    /// `talaria --bg`.
    CommandLine,
    /// The fleet list.
    FleetView,
}

/// Largest prompt this launch accepts, in bytes.
pub const MAX_PROMPT_BYTES: usize = 1_048_576;

/// A worktree directory name and the branch that points at it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WorktreeNames {
    /// `talaria-<slug>` or `talaria-<slug>-N`.
    pub dir_name: String,
    /// `talaria/<slug>` or `talaria/<slug>-N`.
    pub branch: String,
}

/// An explicit name cleaned to nothing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NameError {
    /// No ASCII letter or digit remained.
    Empty,
}

/// Lower-case, collapse runs outside `a-z0-9` to one `-`, trim, keep 40 characters.
/// An empty result is `work`.
#[must_use]
pub fn slugify(text: &str) -> String {
    let _ = text;
    todo!("slugify")
}

/// The same cleaning rule as [`slugify`], except an empty result is [`NameError::Empty`].
pub fn explicit_slug(name: &str) -> Result<String, NameError> {
    let _ = name;
    todo!("explicit slug")
}

/// The first free `talaria-<slug>` / `talaria/<slug>` pair, then `-2` through `-99`.
///
/// `taken_branches` holds local branch names and remote-tracking names with `origin/` removed.
#[must_use]
pub fn pick_names(
    slug: &str,
    taken_dirs: &BTreeSet<String>,
    taken_branches: &BTreeSet<String>,
) -> Option<WorktreeNames> {
    let _ = (slug, taken_dirs, taken_branches);
    todo!("pick names")
}
