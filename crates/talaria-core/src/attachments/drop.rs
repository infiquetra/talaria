//! Candidates in one pasted line. Nothing here reads the file system.

use std::path::{Path, PathBuf};

/// One path to try, and the text that follows it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DropCandidate {
    /// Absolute, or relative when the paste was relative.
    pub path: PathBuf,
    /// Text after the path. Empty when the path was the whole line.
    pub remainder: String,
}

/// Candidates in try order. Relative paths stay relative.
#[must_use]
pub fn drop_candidates(text: &str, home: &Path) -> Vec<DropCandidate> {
    let _ = (text, home);
    unimplemented!()
}
