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
pub fn line_diff(old: &str, new: &str) -> SoulDiff {
    let mut result = SoulDiff::default();
    for change in similar::TextDiff::from_lines(old, new).iter_all_changes() {
        let prefix = match change.tag() {
            similar::ChangeTag::Insert => {
                result.added += 1;
                "+ "
            }
            similar::ChangeTag::Delete => {
                result.removed += 1;
                "- "
            }
            similar::ChangeTag::Equal => continue,
        };
        if result.first_lines.len() < 10 {
            result.first_lines.push(format!(
                "{prefix}{}",
                change.value().trim_end_matches(['\r', '\n'])
            ));
        }
    }
    result
}
