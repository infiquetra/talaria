//! Pure parsers for git's machine output. Issue #245.

use std::collections::BTreeMap;

use crate::review::model::{BranchHead, ChangedFile, FileStatus};

/// A parse failure. The message is for the notice line, after sanitising.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParseError(pub String);

/// Numstat counts by path: added and removed lines, `None` for binary files.
pub type Numstat = BTreeMap<String, (Option<u32>, Option<u32>)>;

/// What a `git push --porcelain` report said.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PushReport {
    /// One `(flag, ref)` pair per pushed ref.
    pub refs: Vec<(char, String)>,
    /// Any ref was rejected.
    pub rejected: bool,
}

/// Parse `git status --porcelain=v2 -z --branch` output.
pub fn parse_status_v2(raw: &[u8]) -> Result<(BranchHead, Vec<ChangedFile>), ParseError> {
    let _ = raw;
    todo!()
}

/// Parse `git diff --numstat -z` output. Binary files give `None` counts.
pub fn parse_numstat_z(raw: &[u8]) -> Result<Numstat, ParseError> {
    let _ = raw;
    todo!()
}

/// Parse `git diff --name-status -z` output: status, new path, old path.
pub fn parse_name_status_z(
    raw: &[u8],
) -> Result<Vec<(FileStatus, String, Option<String>)>, ParseError> {
    let _ = raw;
    todo!()
}

/// Merge staged and unstaged numstat counts into the rows, keyed by path.
pub fn merge_counts(files: &mut [ChangedFile], staged: &Numstat, unstaged: &Numstat) {
    let _ = (files, staged, unstaged);
    todo!()
}

/// Count the lines of an untracked file. `None` when a NUL byte is present.
pub fn count_lines(bytes: &[u8]) -> Option<u32> {
    let _ = bytes;
    todo!()
}

/// Parse `git push --porcelain` standard output.
pub fn parse_push_porcelain(stdout: &str) -> PushReport {
    let _ = stdout;
    todo!()
}
