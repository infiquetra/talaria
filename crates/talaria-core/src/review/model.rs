//! Review data: changed files and the branch head. Issue #245.

/// How many file rows the list keeps before it cuts with a notice.
pub const REVIEW_FILE_CAP: usize = 2_000;
/// How many bytes of `git status` tail the untracked count scan reads at most.
pub const UNTRACKED_COUNT_MAX_BYTES: u64 = 1_048_576;
/// How many diff lines the diff panel keeps before it cuts with a notice.
pub const DIFF_MAX_LINES: usize = 20_000;
/// A file larger than this is never diffed.
pub const DIFF_MAX_FILE_BYTES: u64 = 8 * 1_048_576;

/// The status letter of one changed file.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FileStatus {
    Modified,
    Added,
    Deleted,
    Renamed,
    Copied,
    TypeChanged,
    Untracked,
    Conflicted,
}

/// One changed file.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChangedFile {
    /// New path, repository-relative, `/` separators.
    pub path: String,
    /// Old path, for `Renamed` and `Copied`.
    pub orig_path: Option<String>,
    /// The status letter.
    pub status: FileStatus,
    /// The index differs from `HEAD` for this path.
    pub staged: bool,
    /// The working tree differs from the index, or the file is untracked.
    pub unstaged: bool,
    /// Added lines. `None` means binary, too large, or a folder.
    pub added: Option<u32>,
    /// Removed lines. `None` means binary, too large, or a folder.
    pub removed: Option<u32>,
    /// An untracked folder reported as one row.
    pub is_dir: bool,
}

/// The branch head of a listing.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct BranchHead {
    /// The commit id. `None` on an unborn branch.
    pub oid: Option<String>,
    /// The branch name. `None` when detached.
    pub head: Option<String>,
    /// The upstream ref, when one is set.
    pub upstream: Option<String>,
    /// Commits ahead of the upstream.
    pub ahead: u32,
    /// Commits behind the upstream.
    pub behind: u32,
}
