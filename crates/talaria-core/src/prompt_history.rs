//! Submitted prompt history entries shared by the store and the search box.
//!
//! Pure value types. Behavior contract: issue 228.

/// One submitted prompt.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct HistoryEntry {
    /// The prompt exactly as submitted.
    pub text: String,
    /// Unix seconds when it was submitted.
    pub ts: i64,
    /// Whether it was submitted from the current workspace.
    pub this_folder: bool,
}

/// Which entries the search box lists.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Scope {
    /// Entries from the current workspace only.
    ThisFolder,
    /// Entries from every workspace.
    AllFolders,
}
