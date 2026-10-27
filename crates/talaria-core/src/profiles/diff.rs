//! Line diffs for soul reviews. Issue #253.

/// What changed between two soul texts.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SoulDiff {
    /// Inserted lines.
    pub added: usize,
    /// Deleted lines.
    pub removed: usize,
    /// The first changed lines in diff order, each prefixed `+ ` or `- `.
    pub first_lines: Vec<String>,
}

/// Diff two soul texts line by line. Issue #253.
#[must_use]
pub fn line_diff(_old: &str, _new: &str) -> SoulDiff {
    todo!("profiles stub")
}
