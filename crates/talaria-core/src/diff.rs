//! A diff as files, hunks and rows. Issue 221. Styling lives elsewhere.

/// One parsed diff.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DiffModel {
    /// Files in the order they were opened.
    pub files: Vec<DiffFile>,
    /// Lines that are not rows of any file, such as a binary notice.
    pub notes: Vec<String>,
}

/// One file inside a diff.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DiffFile {
    /// Path before the change. Absent for a new file.
    pub old_path: Option<String>,
    /// Path after the change. Absent for a deletion.
    pub new_path: Option<String>,
    /// Hunks in order.
    pub hunks: Vec<DiffHunk>,
}

/// One hunk.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DiffHunk {
    /// The whole `@@` line.
    pub header: String,
    /// Old start line when the header parsed.
    pub old_start: Option<u32>,
    /// New start line when the header parsed.
    pub new_start: Option<u32>,
    /// Rows of this hunk.
    pub rows: Vec<DiffRow>,
}

/// One row. `text` does not include the leading sign.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DiffRow {
    /// Added, removed, context or a note inside the hunk.
    pub kind: RowKind,
    /// Old line number when this row has one.
    pub old_no: Option<u32>,
    /// New line number when this row has one.
    pub new_no: Option<u32>,
    /// Row text without the sign character.
    pub text: String,
}

/// What a diff row is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RowKind {
    /// A line that was added.
    Added,
    /// A line that was removed.
    Removed,
    /// A line that is unchanged.
    Context,
    /// A note inside a hunk, such as "no newline".
    Note,
}

/// Counts of files and changed rows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DiffStats {
    /// How many files the model holds.
    pub files: usize,
    /// Added rows.
    pub added: usize,
    /// Removed rows.
    pub removed: usize,
}

impl DiffModel {
    /// Added and removed rows, and the file count.
    #[must_use]
    pub fn stats(&self) -> DiffStats {
        todo!()
    }

    /// True when there is no file and no note.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        todo!()
    }

    /// The single file's new path, or its old path, without a leading `a/` or `b/`.
    #[must_use]
    pub fn display_path(&self) -> Option<&str> {
        todo!()
    }

    /// Rows, plus a header after the first hunk, plus a file row when there are several files.
    #[must_use]
    pub fn row_count(&self) -> usize {
        todo!()
    }
}

/// Read a Hermes diff, a unified diff, or `git diff` text. Never panics.
///
/// Issue 221. The text is not read yet.
#[must_use]
pub fn parse_diff(text: &str) -> DiffModel {
    let _ = text;
    todo!()
}
